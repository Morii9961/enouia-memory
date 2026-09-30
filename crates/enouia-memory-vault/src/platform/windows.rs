//! Win32 primitives. Every `unsafe` block in this crate lives here, each with
//! the invariant it relies on. Nothing here logs paths or SIDs.

use super::{AclEntry, AclPrincipal, AclReport, DriveKind, VolumeInfo};
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SET_ACCESS,
    SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    CreateWellKnownSid, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
    GetSecurityDescriptorControl, GetTokenInformation, INHERITED_ACE,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
    SECURITY_MAX_SID_SIZE, SUB_CONTAINERS_AND_OBJECTS_INHERIT, TOKEN_QUERY, TOKEN_USER, TokenUser,
    WinBuiltinAdministratorsSid, WinLocalSystemSid,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ALL_ACCESS, GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
const ACCESS_DENIED_ACE_TYPE: u8 = 1;

fn wide(path: &OsStr) -> Vec<u16> {
    path.encode_wide().chain(std::iter::once(0)).collect()
}

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn check_win32(code: u32) -> io::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(code as i32))
    }
}

pub fn fill_random(buffer: &mut [u8]) -> io::Result<()> {
    for chunk in buffer.chunks_mut(u32::MAX as usize) {
        // SAFETY: the pointer/length pair describes a live, writable slice;
        // a null algorithm handle is valid with BCRYPT_USE_SYSTEM_PREFERRED_RNG.
        let status = unsafe {
            BCryptGenRandom(
                ptr::null_mut(),
                chunk.as_mut_ptr(),
                chunk.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if status != 0 {
            return Err(io::Error::other("system RNG failed"));
        }
    }
    Ok(())
}

/// Same-volume atomic replacement with write-through: the rename is on disk
/// before this returns (MOVEFILE_WRITE_THROUGH), not merely in the cache.
pub fn replace_durable(from: &Path, to: &Path) -> io::Result<()> {
    let from = wide(from.as_os_str());
    let to = wide(to.as_os_str());
    // A reader (or scanner) can briefly hold the destination in a state where
    // NTFS refuses replacement with ACCESS_DENIED or SHARING_VIOLATION. Retry
    // the same rename only; never unlink CURRENT or report a failed publication
    // as committed. Permanent permission errors still fail within one second.
    retry_replacement(|| {
        // SAFETY: both buffers are NUL-terminated UTF-16 strings that outlive the call.
        check(unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
    })
}

fn retry_replacement(mut replace: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match replace() {
            Err(error)
                if matches!(error.raw_os_error(), Some(5 | 32))
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            outcome => return outcome,
        }
    }
}

/// NTFS journals directory metadata, and every rename above is write-through,
/// so there is no separate directory flush on Windows.
pub fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

pub fn volume_info(path: &Path) -> io::Result<VolumeInfo> {
    let path_w = wide(path.as_os_str());
    let mut root = vec![0u16; 1024];
    // SAFETY: `root` is a writable buffer of the stated length.
    check(unsafe { GetVolumePathNameW(path_w.as_ptr(), root.as_mut_ptr(), root.len() as u32) })?;
    // SAFETY: GetVolumePathNameW wrote a NUL-terminated string into `root`.
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
    let mut fs_name = [0u16; 64];
    let (mut serial, mut max_component, mut flags) = (0u32, 0u32, 0u32);
    // SAFETY: all out-pointers are valid; the name buffer length is stated.
    check(unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            ptr::null_mut(),
            0,
            &mut serial,
            &mut max_component,
            &mut flags,
            fs_name.as_mut_ptr(),
            fs_name.len() as u32,
        )
    })?;
    let fs_len = fs_name
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(fs_name.len());
    let mut free = 0u64;
    // SAFETY: valid NUL-terminated path; optional out-pointers may be null.
    check(unsafe {
        GetDiskFreeSpaceExW(path_w.as_ptr(), &mut free, ptr::null_mut(), ptr::null_mut())
    })?;
    Ok(VolumeInfo {
        filesystem: String::from_utf16_lossy(&fs_name[..fs_len]),
        drive: match drive_type {
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_REMOTE => DriveKind::Remote,
            _ => DriveKind::Other,
        },
        free_bytes: free,
    })
}

/// A SID copied into owned storage.
struct Sid([u8; SECURITY_MAX_SID_SIZE as usize]);

impl Sid {
    fn as_psid(&self) -> PSID {
        self.0.as_ptr() as PSID
    }

    fn well_known(kind: i32) -> io::Result<Self> {
        let mut sid = Sid([0; SECURITY_MAX_SID_SIZE as usize]);
        let mut size = SECURITY_MAX_SID_SIZE;
        // SAFETY: the buffer holds SECURITY_MAX_SID_SIZE bytes, as stated in `size`.
        check(unsafe {
            CreateWellKnownSid(kind, ptr::null_mut(), sid.0.as_mut_ptr() as PSID, &mut size)
        })?;
        Ok(sid)
    }

    fn current_user() -> io::Result<Self> {
        let mut token: HANDLE = ptr::null_mut();
        // SAFETY: GetCurrentProcess returns a pseudo-handle; `token` is a valid out-pointer.
        check(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
        let mut buffer = vec![0u64; 64]; // 512 bytes, 8-byte aligned for TOKEN_USER
        let mut needed = 0u32;
        // SAFETY: the buffer is writable for the stated byte length.
        let result = check(unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        });
        // SAFETY: `token` was opened above and is closed exactly once.
        unsafe { CloseHandle(token) };
        result?;
        // SAFETY: GetTokenInformation(TokenUser) filled an aligned TOKEN_USER
        // whose SID points into the same buffer, which is still alive.
        let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
        let mut sid = Sid([0; SECURITY_MAX_SID_SIZE as usize]);
        // SAFETY: source SID is valid; destination has SECURITY_MAX_SID_SIZE bytes.
        check(unsafe {
            windows_sys::Win32::Security::CopySid(
                SECURITY_MAX_SID_SIZE,
                sid.0.as_mut_ptr() as PSID,
                user.User.Sid,
            )
        })?;
        Ok(sid)
    }
}

fn classify(sid: PSID, known: &[(AclPrincipal, &Sid)]) -> AclPrincipal {
    for (principal, candidate) in known {
        // SAFETY: both SIDs are valid for the duration of the call.
        if unsafe { EqualSid(sid, candidate.as_psid()) } != 0 {
            return *principal;
        }
    }
    AclPrincipal::Other
}

pub fn inspect_acl(path: &Path) -> io::Result<AclReport> {
    let user = Sid::current_user()?;
    let system = Sid::well_known(WinLocalSystemSid)?;
    let admins = Sid::well_known(WinBuiltinAdministratorsSid)?;
    let known = [
        (AclPrincipal::CurrentUser, &user),
        (AclPrincipal::System, &system),
        (AclPrincipal::Administrators, &admins),
    ];
    let path_w = wide(path.as_os_str());
    let mut dacl: *mut ACL = ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: valid path; out-pointers are valid; descriptor is freed below.
    check_win32(unsafe {
        GetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut dacl,
            ptr::null_mut(),
            &mut descriptor,
        )
    })?;
    let result = (|| {
        let mut control = 0u16;
        let mut revision = 0u32;
        // SAFETY: `descriptor` came from GetNamedSecurityInfoW and is still alive.
        check(unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) })?;
        let mut entries = Vec::new();
        if dacl.is_null() {
            // A NULL DACL grants everyone full access.
            entries.push(AclEntry {
                principal: AclPrincipal::Other,
                allow: true,
                inherited: false,
            });
        } else {
            let mut info = ACL_SIZE_INFORMATION {
                AceCount: 0,
                AclBytesInUse: 0,
                AclBytesFree: 0,
            };
            // SAFETY: `dacl` points into `descriptor`; `info` has the stated size.
            check(unsafe {
                GetAclInformation(
                    dacl,
                    (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            })?;
            for index in 0..info.AceCount {
                let mut ace: *mut core::ffi::c_void = ptr::null_mut();
                // SAFETY: index < AceCount; GetAce returns a pointer into `dacl`.
                check(unsafe { GetAce(dacl, index, &mut ace) })?;
                // SAFETY: every ACE starts with an ACE_HEADER.
                let header = unsafe { *(ace as *const ACE_HEADER) };
                let inherited = u32::from(header.AceFlags) & INHERITED_ACE != 0;
                let (principal, allow) = match header.AceType {
                    // Allowed and denied ACEs share ACCESS_ALLOWED_ACE's layout;
                    // the SID starts at `SidStart`.
                    ACCESS_ALLOWED_ACE_TYPE | ACCESS_DENIED_ACE_TYPE => {
                        let body = ace as *const ACCESS_ALLOWED_ACE;
                        // SAFETY: layout per the ACE type checked above.
                        let sid = unsafe { ptr::addr_of!((*body).SidStart) } as PSID;
                        (
                            classify(sid, &known),
                            header.AceType == ACCESS_ALLOWED_ACE_TYPE,
                        )
                    }
                    _ => (AclPrincipal::Other, true),
                };
                entries.push(AclEntry {
                    principal,
                    allow,
                    inherited,
                });
            }
        }
        Ok(AclReport {
            protected: control & SE_DACL_PROTECTED != 0,
            entries,
        })
    })();
    // SAFETY: allocated by GetNamedSecurityInfoW; freed exactly once.
    unsafe { LocalFree(descriptor) };
    result
}

/// Replace the DACL with a protected one granting full control to the
/// current user, SYSTEM, and Administrators only, inherited by children.
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    let user = Sid::current_user()?;
    let system = Sid::well_known(WinLocalSystemSid)?;
    let admins = Sid::well_known(WinBuiltinAdministratorsSid)?;
    let entry = |sid: &Sid| EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.as_psid() as *mut u16,
        },
    };
    let entries = [entry(&user), entry(&system), entry(&admins)];
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: `entries` and the SIDs they point to outlive the call; a null
    // old ACL builds a fresh one, freed below.
    check_win32(unsafe {
        SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            ptr::null(),
            &mut acl,
        )
    })?;
    let path_w = wide(path.as_os_str());
    // SAFETY: valid path and ACL; PROTECTED stops inheritance from parents.
    let result = check_win32(unsafe {
        SetNamedSecurityInfoW(
            path_w.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null(),
        )
    });
    // SAFETY: allocated by SetEntriesInAclW; freed exactly once.
    unsafe { LocalFree(acl.cast()) };
    result
}

/// Cloud-file / offline attributes (OneDrive and other sync providers).
pub fn has_cloud_attributes(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const CLOUD: u32 = 0x0000_1000 // OFFLINE
        | 0x0004_0000 // RECALL_ON_OPEN
        | 0x0008_0000 // PINNED
        | 0x0010_0000 // UNPINNED
        | 0x0040_0000; // RECALL_ON_DATA_ACCESS
    metadata.file_attributes() & CLOUD != 0
}

pub fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0000_0400 != 0
}

#[cfg(test)]
mod replacement_tests {
    use super::*;

    #[test]
    fn transient_reader_errors_retry_the_same_publication() {
        let mut attempts = 0;
        retry_replacement(|| {
            attempts += 1;
            match attempts {
                1 => Err(io::Error::from_raw_os_error(5)),
                2 => Err(io::Error::from_raw_os_error(32)),
                _ => Ok(()),
            }
        })
        .unwrap();
        assert_eq!(attempts, 3);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let mut attempts = 0;
        let error = retry_replacement(|| {
            attempts += 1;
            Err(io::Error::from_raw_os_error(112))
        })
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.raw_os_error(), Some(112));
    }

    #[test]
    fn an_actual_open_destination_refuses_rename_then_releases_it() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = std::env::temp_dir().join(format!(
            "enouia-memory-rename-{}-{}",
            std::process::id(),
            crate::fs::hex(&super::super::random_bytes::<8>().unwrap())
        ));
        std::fs::create_dir_all(&root).unwrap();
        let from = root.join("new");
        let to = root.join("CURRENT");
        std::fs::write(&from, b"new").unwrap();
        std::fs::write(&to, b"old").unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&to)
            .unwrap();
        let from_w = wide(from.as_os_str());
        let to_w = wide(to.as_os_str());
        // SAFETY: live, NUL-terminated path buffers; isolated test files only.
        let first = check(unsafe {
            MoveFileExW(
                from_w.as_ptr(),
                to_w.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        })
        .unwrap_err();
        assert!(matches!(first.raw_os_error(), Some(5 | 32)));
        assert_eq!(std::fs::read(&from).unwrap(), b"new");
        let writer = std::thread::spawn(move || replace_durable(&from, &to));
        std::thread::sleep(std::time::Duration::from_millis(10));
        drop(held);
        writer.join().unwrap().unwrap();
        assert_eq!(std::fs::read(root.join("CURRENT")).unwrap(), b"new");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persistent_access_denial_is_bounded_and_never_success() {
        let started = std::time::Instant::now();
        let error = retry_replacement(|| Err(io::Error::from_raw_os_error(5))).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(5));
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }
}
