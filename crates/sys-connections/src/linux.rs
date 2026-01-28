use anyhow::Result;
use std::fs;

pub fn get_name_by_pid(pid: u32) -> Result<String> {
    let exe_path = format!("/proc/{}/exe", pid);

    match fs::read_link(&exe_path) {
        Ok(path) => {
            return Ok(path.display().to_string());
        }
        Err(_) => {
            let comm_path = format!("/proc/{}/comm", pid);
            let comm = fs::read_to_string(comm_path)?;
            Ok(comm.trim_end().to_string())
        }
    }
}
