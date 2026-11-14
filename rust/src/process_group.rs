//! Own the entire worker process tree so cancelling setup cannot leave helpers running.
use anyhow::Result;
#[cfg(windows)]
use anyhow::{Context, ensure};
use std::process::{Child, Command};

pub struct WorkerGroup {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    pid: libc::pid_t,
}
impl WorkerGroup {
    pub fn kill(&self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1);
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
    }
    pub fn configure(command: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
    }
    pub fn attach(child: &Child) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
            // Job objects are owned by this call, never by an arbitrary PID lookup.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                ensure!(
                    !handle.is_null(),
                    "Cannot create a worker job: {}",
                    std::io::Error::last_os_error()
                );
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let configured = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                );
                if configured == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle().cast()) == 0
                {
                    let error = std::io::Error::last_os_error();
                    CloseHandle(handle);
                    return Err(error).context("Cannot isolate the local worker process tree");
                }
                Ok(Self { handle })
            }
        }
        #[cfg(unix)]
        {
            Ok(Self {
                pid: child.id() as libc::pid_t,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = child;
            Ok(Self {})
        }
    }
}
impl Drop for WorkerGroup {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.kill();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
// Kernel jobs allow concurrent termination. Arc ownership keeps the handle open.
#[cfg(windows)]
unsafe impl Send for WorkerGroup {}
#[cfg(windows)]
unsafe impl Sync for WorkerGroup {}
