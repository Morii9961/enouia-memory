//! Protected local pipes and explicitly enrolled live process identities.
//! Win32 invariants: docs/adr/077-windows-host-transport.md.
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::workspace::HostSurface;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, ERROR_SUCCESS, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID,
    TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, CopySid, EqualSid, GetTokenInformation, InitializeSecurityDescriptor, PSID,
    SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE, SetSecurityDescriptorDacl,
    TOKEN_QUERY, TOKEN_USER, TokenSessionId, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ALL_ACCESS, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn handle(raw: HANDLE) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: only successful handle-producing API results reach this point;
    // the caller transfers their single owned reference here.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
fn raw(h: &OwnedHandle) -> HANDLE {
    h.as_raw_handle()
}
fn denied() -> io::Error {
    io::Error::from(io::ErrorKind::PermissionDenied)
}

#[repr(align(8))]
struct Sid([u8; SECURITY_MAX_SID_SIZE as usize]);
struct User {
    sid: Sid,
    session: u32,
}
fn user(process: HANDLE) -> io::Result<User> {
    let mut token = ptr::null_mut();
    // SAFETY: live process handle and initialized token out-pointer.
    check(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) })?;
    let token = handle(token)?;
    let mut buffer = [0u64; 64];
    let mut needed = 0;
    // SAFETY: aligned 512-byte buffer exceeds TOKEN_USER plus maximum SID.
    check(unsafe {
        GetTokenInformation(
            raw(&token),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            512,
            &mut needed,
        )
    })?;
    // SAFETY: successful TokenUser query returns TOKEN_USER pointing into buffer.
    let token_user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = Sid([0; SECURITY_MAX_SID_SIZE as usize]);
    // SAFETY: valid source SID, aligned maximum-size destination.
    check(unsafe {
        CopySid(
            SECURITY_MAX_SID_SIZE,
            sid.0.as_mut_ptr().cast(),
            token_user.User.Sid,
        )
    })?;
    let mut session = 0u32;
    // SAFETY: TokenSessionId writes one DWORD into the stated buffer.
    check(unsafe {
        GetTokenInformation(
            raw(&token),
            TokenSessionId,
            (&mut session as *mut u32).cast(),
            4,
            &mut needed,
        )
    })?;
    Ok(User { sid, session })
}

/// Kernel process/account evidence only. It grants no Owner role; the trusted
/// local bootstrap command separately registers the authenticated owner-self.
pub(crate) struct CurrentProcessSession {
    peer: ProcessPeer,
    account: Sha256Hex,
    session: u32,
}
impl CurrentProcessSession {
    pub(crate) fn capture() -> io::Result<Self> {
        let peer = ProcessPeer::enroll(std::process::id(), HostSurface::Workspace)?;
        let identity = user(raw(&peer.process))?;
        Ok(Self {
            peer,
            account: enouia_memory_contract::hash::sha256(&identity.sid.0),
            session: identity.session,
        })
    }
    pub(crate) fn account(&self) -> &Sha256Hex {
        &self.account
    }
    pub(crate) fn verify(&self) -> io::Result<()> {
        self.peer.live()?;
        let current = user(raw(&self.peer.process))?;
        if current.session != self.session
            || enouia_memory_contract::hash::sha256(&current.sid.0) != self.account
        {
            return Err(denied());
        }
        Ok(())
    }
}

/// Native owner explicitly enrolls this process and its maximum surface.
/// Holding the process object and checking liveness prevents PID-reuse grants.
/// This is not enrollment from a page, a claimed PID or an executable name.
pub struct ProcessPeer {
    process: OwnedHandle,
    pid: u32,
    pub(crate) surface: HostSurface,
}
impl ProcessPeer {
    pub fn enroll(pid: u32, surface: HostSurface) -> io::Result<Self> {
        // SAFETY: OpenProcess checks access; failed/null handles are rejected.
        let process = handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        })?;
        let peer = Self {
            process,
            pid,
            surface,
        };
        peer.live()?;
        // SAFETY: pseudo-handle is used only in this process, never closed.
        let current = user(unsafe { GetCurrentProcess() })?;
        let target = user(raw(&peer.process))?;
        // SAFETY: both copied SIDs are valid and live throughout comparison.
        let same_user = unsafe {
            EqualSid(
                current.sid.0.as_ptr() as PSID,
                target.sid.0.as_ptr() as PSID,
            )
        } != 0;
        if !same_user || current.session != target.session {
            return Err(denied());
        }
        // SAFETY: retained handle names the actual enrolled process object.
        if unsafe { GetProcessId(raw(&peer.process)) } != pid {
            return Err(denied());
        }
        Ok(peer)
    }
    pub fn live(&self) -> io::Result<()> {
        // SAFETY: owned process handle includes SYNCHRONIZE and remains open.
        match unsafe { WaitForSingleObject(raw(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(()),
            WAIT_OBJECT_0 => Err(denied()),
            _ => Err(io::Error::last_os_error()),
        }
    }
    fn verify(&self, pipe: &Pipe, server_end: bool) -> io::Result<()> {
        if pipe.server != server_end {
            return Err(denied());
        }
        self.live()?;
        let mut pid = 0;
        // SAFETY: connected pipe and valid PID out-pointer; use the matching end.
        check(unsafe {
            if server_end {
                GetNamedPipeClientProcessId(raw(&pipe.handle), &mut pid)
            } else {
                GetNamedPipeServerProcessId(raw(&pipe.handle), &mut pid)
            }
        })?;
        if pid != self.pid {
            return Err(denied());
        }
        self.live()
    }
    pub fn verify_client(&self, pipe: &Pipe) -> io::Result<()> {
        self.verify(pipe, true)
    }
    pub fn verify_server(&self, pipe: &Pipe) -> io::Result<()> {
        self.verify(pipe, false)
    }
}

pub struct Pipe {
    handle: OwnedHandle,
    server: bool,
}
fn name(id: &Sha256Hex) -> Vec<u16> {
    format!(r"\\.\pipe\EnouiaMemory-{id}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
}
impl Pipe {
    pub fn listen(id: &Sha256Hex) -> io::Result<Self> {
        // SAFETY: local pseudo-handle, never owned/closed.
        let mut owner = user(unsafe { GetCurrentProcess() })?;
        let entry = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: SET_ACCESS,
            grfInheritance: 0,
            Trustee: TRUSTEE_W {
                pMultipleTrustee: ptr::null_mut(),
                MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: owner.sid.0.as_mut_ptr().cast(),
            },
        };
        let mut acl: *mut ACL = ptr::null_mut();
        // SAFETY: entry/SID remain live; a new ACL is returned via initialized pointer.
        let status = unsafe { SetEntriesInAclW(1, &entry, ptr::null(), &mut acl) };
        if status != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let result = (|| {
            // SAFETY: zero is a valid initial representation before initialization.
            let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
            let descriptor_ptr = (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast();
            // SAFETY: writable absolute descriptor, revision 1; ACL remains live.
            check(unsafe { InitializeSecurityDescriptor(descriptor_ptr, 1) })?;
            check(unsafe { SetSecurityDescriptorDacl(descriptor_ptr, 1, acl, 0) })?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor_ptr,
                bInheritHandle: 0,
            };
            let name = name(id);
            // SAFETY: bounded NUL-terminated local name; explicit owner DACL copied
            // by creation. One instance reserves this endpoint until handle drop.
            let handle = handle(unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    4096,
                    4096,
                    0,
                    &attributes,
                )
            })?;
            Ok(Self {
                handle,
                server: true,
            })
        })();
        // SAFETY: allocated once by SetEntriesInAclW; no descriptor references remain.
        unsafe { LocalFree(acl.cast()) };
        result
    }
    pub fn connect(id: &Sha256Hex) -> io::Result<Self> {
        let name = name(id);
        // SAFETY: local NUL-terminated name; no inherited handle or default file creation.
        let handle = handle(unsafe {
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                ptr::null_mut(),
            )
        })?;
        Ok(Self {
            handle,
            server: false,
        })
    }
    fn operation(
        &mut self,
        deadline: Instant,
        connect: bool,
        start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> i32,
    ) -> io::Result<u32> {
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        // SAFETY: unnamed manual-reset event, noninherited and initially nonsignaled.
        let event = handle(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
        // SAFETY: zeroed OVERLAPPED is initialized with its owned event before use.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = raw(&event);
        let started = start(raw(&self.handle), &mut overlapped);
        if started != 0 {
            let mut transferred = 0;
            // SAFETY: successful synchronous completion has no pending buffer
            // borrow. Do not wait on an event that need not have been signaled.
            check(unsafe {
                GetOverlappedResult(raw(&self.handle), &overlapped, &mut transferred, 0)
            })?;
            return Ok(transferred);
        }
        if started == 0 {
            let e = io::Error::last_os_error();
            if connect && e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                return Ok(0);
            }
            if e.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                return Err(e);
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let wait_ms = remaining.as_millis().min(u128::from(u32::MAX - 1)) as u32;
        // SAFETY: the live event belongs exclusively to this OVERLAPPED.
        let waited = unsafe { WaitForSingleObject(raw(&event), wait_ms) };
        let wait_error = if waited == WAIT_OBJECT_0 {
            None
        } else if waited == WAIT_TIMEOUT {
            Some(io::Error::from(io::ErrorKind::TimedOut))
        } else {
            Some(io::Error::last_os_error())
        };
        if wait_error.is_some() {
            // SAFETY: cancel only this operation; it may already have completed.
            unsafe { CancelIoEx(raw(&self.handle), &overlapped) };
        }
        let mut transferred = 0;
        // SAFETY: completion MUST be drained even after timeout/cancel before
        // OVERLAPPED, event or caller buffer drops. CancelIoEx alone is not completion.
        let completed = check(unsafe {
            GetOverlappedResult(raw(&self.handle), &overlapped, &mut transferred, 1)
        });
        if let Some(e) = wait_error {
            return Err(e);
        }
        completed?;
        Ok(transferred)
    }
    pub fn accept(&mut self, timeout: Duration) -> io::Result<()> {
        if !self.server {
            return Err(denied());
        }
        self.operation(Instant::now() + timeout, true, |h, o| {
            // SAFETY: live pipe/event/OVERLAPPED retained by operation until completion.
            unsafe { ConnectNamedPipe(h, o) }
        })?;
        Ok(())
    }
    pub fn disconnect(&mut self) -> io::Result<()> {
        if !self.server {
            return Err(denied());
        }
        // SAFETY: no outstanding I/O; server owns this instance and never flush-waits on clients.
        check(unsafe { DisconnectNamedPipe(raw(&self.handle)) })
    }
    pub fn read_exact(&mut self, mut bytes: &mut [u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            let len = bytes.len().min(u32::MAX as usize) as u32;
            let n = self.operation(deadline, false, |h, o| {
                // SAFETY: output buffer remains borrowed until operation drains completion.
                unsafe { ReadFile(h, bytes.as_mut_ptr(), len, ptr::null_mut(), o) }
            })? as usize;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            bytes = &mut bytes[n..];
        }
        Ok(())
    }
    pub fn write_all(&mut self, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
        while !bytes.is_empty() {
            let len = bytes.len().min(u32::MAX as usize) as u32;
            let n = self.operation(deadline, false, |h, o| {
                // SAFETY: input buffer remains borrowed until operation drains completion.
                unsafe { WriteFile(h, bytes.as_ptr(), len, ptr::null_mut(), o) }
            })? as usize;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[n..];
        }
        Ok(())
    }
}
