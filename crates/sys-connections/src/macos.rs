use anyhow::{Result, bail};
use libc::{c_int, proc_pidpath, PROC_PIDPATHINFO_MAXSIZE};
use std::{ffi::OsString, os::unix::ffi::OsStringExt};

pub fn get_name_by_pid(pid: u32) -> Result<String> {
    let mut buffer = vec![0u8; PROC_PIDPATHINFO_MAXSIZE as usize];
    let len = unsafe {
        proc_pidpath(
            pid as c_int,
            buffer.as_mut_ptr() as *mut _,
            buffer.len() as u32,
        )
    };

    if len <= 0 {
        bail!("proc_pidpath failed pid: {}, err: {}", pid, get_errno());
    }
    buffer.truncate(len as usize);

    let os_str = OsString::from_vec(buffer);
    Ok(os_str.to_string_lossy().into_owned())
}

fn get_errno() -> i32 {
    unsafe { *libc::__error() }
}
