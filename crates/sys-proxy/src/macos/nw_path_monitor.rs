// TODO: move to new create ? (network_framework_sys is not complete)
use std::{ffi::CString, os::raw::c_void};
use block2::StackBlock;
use super::ffi::*;

pub struct NWMonitor {
    queue: dispatch_queue_t,
    monitor: nw_path_monitor_t,
}

impl NWMonitor {
    pub fn start(f: impl Fn() + std::clone::Clone + 'static) -> Self {

        let res;
        let label = CString::new("com.covert-connect.net.monitor").unwrap();
        unsafe {
            let queue = dispatch_queue_create(label.as_ptr(), DISPATCH_QUEUE_CONCURRENT);

            let block = StackBlock::new( move |_path: nw_path_t| f());

            let monitor = nw_path_monitor_create();
            nw_path_monitor_set_queue(monitor, queue);

            nw_path_monitor_set_update_handler(monitor, &block);

            nw_path_monitor_start(monitor);

            res = Self {
                queue,
                monitor,
            }
        }

        res
    }
}

impl Drop for NWMonitor {
    fn drop(&mut self) {
        unsafe {
            nw_path_monitor_cancel(self.monitor);
            nw_release(self.monitor as *mut c_void);
            nw_release(self.queue as *mut c_void);
        }
    }
}
