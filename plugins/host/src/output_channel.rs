//! Keep plugin startup/debug output out of the parent's JSON response channel.
use std::{fs::File, io};

pub fn reserve() -> io::Result<File> {
    #[cfg(unix)]
    let output = {
        use std::os::fd::AsFd;
        File::from(io::stdout().as_fd().try_clone_to_owned()?)
    };
    #[cfg(windows)]
    let output = {
        use std::os::windows::io::AsHandle;
        File::from(io::stdout().as_handle().try_clone_to_owned()?)
    };
    // Native C/C++ plugins may use the CRT's stdout, while Rust/Win32 code may
    // use the process standard handle. Redirect both before loading a plugin;
    // only the private duplicated handle can write the final JSON response.
    if unsafe { libc::dup2(2, 1) } == -1 {
        return Err(io::Error::last_os_error());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        if unsafe { SetStdHandle(STD_OUTPUT_HANDLE, GetStdHandle(STD_ERROR_HANDLE)) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(output)
}
