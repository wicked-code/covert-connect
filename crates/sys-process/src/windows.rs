use anyhow::Result;
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE, MAX_PATH},
    System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    },
};

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                if let Err(err) = CloseHandle(self.0) {
                    tracing::error!("Failed to close handle: {:?}", err);
                }
            };
        }
    }
}

pub fn path_by_pid(pid: u32) -> Result<String> {
    unsafe {
        let _guard = HandleGuard(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?);
        let process = _guard.0;

        const DEFAULT_BUFFER: u32 = MAX_PATH + 1;
        let mut buffer_size: u32 = DEFAULT_BUFFER;
        let mut path_buffer = [0u16; DEFAULT_BUFFER as usize];

        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path_buffer.as_mut_ptr()),
            &mut buffer_size,
        )?;

        Ok(if buffer_size < DEFAULT_BUFFER {
            String::from_utf16(&path_buffer[0..buffer_size as usize])?
        } else {
            buffer_size = 32769;
            let mut path_buffer = vec![0u16; buffer_size as usize];
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(path_buffer.as_mut_ptr()),
                &mut buffer_size,
            )?;

            String::from_utf16(&path_buffer[0..buffer_size as usize])?
        })
    }
}
