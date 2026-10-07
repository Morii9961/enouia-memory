//! Opt-in current-user startup. Only this app's fixed Run value is exposed;
//! the page cannot choose a registry key, executable, arguments, or Vault.
use serde_json::{Value, json};
use std::path::Path;

const VALUE_NAME: &str = "EnouiaMemoryWorkspace";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const MAX_COMMAND: usize = 260;

fn command(exe: &Path) -> std::io::Result<String> {
    let path = exe
        .to_str()
        .ok_or_else(|| std::io::Error::other("startup_path"))?;
    if !exe.is_absolute() || path.contains(['"', '\0', '\r', '\n']) {
        return Err(std::io::Error::other("startup_path"));
    }
    let command = format!("\"{path}\" --autostart");
    if command.encode_utf16().count() > MAX_COMMAND {
        return Err(std::io::Error::other("startup_command_length"));
    }
    Ok(command)
}

fn state(current: Option<&str>, expected: &str) -> Value {
    json!({"supported":true,"enabled":current == Some(expected),"state": match current {
        None => "disabled", Some(value) if value == expected => "enabled", _ => "different_installation",
    }})
}

#[cfg(windows)]
mod registry {
    use super::MAX_COMMAND;
    use std::io;
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyW,
        RegDeleteValueW, RegGetValueW, RegSetValueExW,
    };

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: this handle was created by RegCreateKeyW and is owned here.
            unsafe { RegCloseKey(self.0) };
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    pub(super) fn read(key: &str, name: &str) -> io::Result<Option<String>> {
        let key = wide(key);
        let name = wide(name);
        let mut data = vec![0u16; MAX_COMMAND + 1];
        let mut size = (data.len() * 2) as u32;
        // SAFETY: NUL-terminated names and a properly aligned, bounded buffer.
        let code = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        if size < 2 || !size.is_multiple_of(2) || size as usize > data.len() * 2 {
            return Err(io::Error::other("startup_value"));
        }
        data.truncate(size as usize / 2);
        if data.pop() != Some(0) || data.contains(&0) {
            return Err(io::Error::other("startup_value"));
        }
        String::from_utf16(&data)
            .map(Some)
            .map_err(|_| io::Error::other("startup_value"))
    }
    pub(super) fn write(key: &str, name: &str, value: Option<&str>) -> io::Result<()> {
        let key_name = wide(key);
        let name = wide(name);
        let mut handle = std::ptr::null_mut();
        // SAFETY: the output handle is initialized only on success.
        let code = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, key_name.as_ptr(), &mut handle) };
        if code != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let key = Key(handle);
        let code = if let Some(value) = value {
            let data = wide(value);
            // SAFETY: the buffer remains alive through this synchronous call.
            unsafe {
                RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    data.as_ptr().cast(),
                    (data.len() * 2) as u32,
                )
            }
        } else {
            // SAFETY: delete only the named value in the opened key.
            unsafe { RegDeleteValueW(key.0, name.as_ptr()) }
        };
        if code == ERROR_SUCCESS || value.is_none() && code == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(code as i32))
        }
    }
}

#[cfg(windows)]
pub fn get() -> Result<Value, String> {
    let expected = command(&std::env::current_exe().map_err(|_| "startup_unavailable")?)
        .map_err(|_| "startup_unavailable")?;
    let current = registry::read(RUN_KEY, VALUE_NAME).map_err(|_| "startup_unavailable")?;
    Ok(state(current.as_deref(), &expected))
}

#[cfg(windows)]
pub fn set(enabled: bool) -> Result<Value, String> {
    let expected = command(&std::env::current_exe().map_err(|_| "startup_unavailable")?)
        .map_err(|_| "startup_unavailable")?;
    let current = registry::read(RUN_KEY, VALUE_NAME).map_err(|_| "startup_unavailable")?;
    if current.as_ref().is_some_and(|value| value != &expected) {
        return Err("startup_different_installation".into());
    }
    registry::write(RUN_KEY, VALUE_NAME, enabled.then_some(expected.as_str()))
        .map_err(|_| "startup_write_failed")?;
    get()
}

#[cfg(not(windows))]
pub fn get() -> Result<Value, String> {
    Ok(json!({"supported":false,"enabled":false,"state":"unsupported"}))
}
#[cfg(not(windows))]
pub fn set(_enabled: bool) -> Result<Value, String> {
    Err("startup_unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_quotes_only_the_current_executable_without_any_vault_or_shell() {
        assert_eq!(
            command(Path::new("C:\\Synthetic install\\Memory.exe")).unwrap(),
            "\"C:\\Synthetic install\\Memory.exe\" --autostart"
        );
        for path in ["relative.exe", "C:\\bad\"name.exe", "C:\\bad\nname.exe"] {
            assert!(command(Path::new(path)).is_err());
        }
        let long = format!("C:\\{}.exe", "x".repeat(260));
        assert!(command(Path::new(&long)).is_err());
    }
    #[test]
    fn startup_status_does_not_expose_other_installation_paths() {
        assert_eq!(state(None, "expected")["state"], "disabled");
        assert_eq!(state(Some("expected"), "expected")["enabled"], true);
        let other = state(Some("C:\\synthetic other.exe"), "expected");
        assert_eq!(other["state"], "different_installation");
        assert!(!other.to_string().contains("synthetic"));
    }
    #[cfg(windows)]
    #[test]
    fn registry_round_trip_uses_an_isolated_non_startup_test_key() {
        let key = format!(
            "Software\\EnouiaMemoryTests\\Startup-{}",
            std::process::id()
        );
        let value = "\"C:\\Synthetic install\\Memory.exe\" --autostart";
        assert_eq!(registry::read(&key, "test").unwrap(), None);
        registry::write(&key, "test", Some(value)).unwrap();
        assert_eq!(
            registry::read(&key, "test").unwrap().as_deref(),
            Some(value)
        );
        registry::write(&key, "test", None).unwrap();
        assert_eq!(registry::read(&key, "test").unwrap(), None);
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyW};
        let name: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        // SAFETY: this process's isolated test key contains no values now.
        assert_eq!(
            unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, name.as_ptr()) },
            0
        );
        // Remove the parent test key too when no other run still uses it.
        let parent: Vec<u16> = "Software\\EnouiaMemoryTests"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: deletes only the empty parent test key; fails harmlessly otherwise.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, parent.as_ptr()) };
    }
}
