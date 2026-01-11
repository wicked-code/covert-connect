#![allow(non_upper_case_globals, non_camel_case_types)]
use std::os::raw::{c_char, c_void};
use block2::Block;

#[repr(C)]
pub struct dispatch_object_s { _private: [u8; 0] }

pub type dispatch_queue_attr_t = *const dispatch_object_s;

// TODO: ??? remove
//pub const DISPATCH_QUEUE_SERIAL: dispatch_queue_attr_t = 0 as dispatch_queue_attr_t;
pub static DISPATCH_QUEUE_CONCURRENT: &'static dispatch_object_s = unsafe { &_dispatch_queue_attr_concurrent };

pub type nw_path_t = *mut c_void;

pub enum dispatch_queue { }
pub type dispatch_queue_t = *mut dispatch_queue;

pub enum nw_path_monitor {}
pub type nw_path_monitor_t = *mut nw_path_monitor;

#[link(name = "System", kind = "dylib")]
extern {
    static _dispatch_queue_attr_concurrent: dispatch_object_s;

    pub fn dispatch_queue_create(label: *const c_char, attr: dispatch_queue_attr_t) -> dispatch_queue_t;
}

#[link(name = "Network", kind = "framework")]
extern {
     pub fn nw_path_monitor_create() -> nw_path_monitor_t;

    // Obj-c signature:
    // void nw_path_monitor_set_update_handler(nw_path_monitor_t monitor, nw_path_monitor_update_handler_t update_handler);
    // typedef void (^nw_path_monitor_update_handler_t)(nw_path_t path);
    pub fn nw_path_monitor_set_update_handler(
        monitor: nw_path_monitor_t,
        update_handler: &Block<dyn Fn(nw_path_t)>,
    );

    pub fn nw_path_monitor_set_queue(monitor: nw_path_monitor_t, queue: dispatch_queue_t);
    pub fn nw_path_monitor_start(monitor: nw_path_monitor_t);
    pub fn nw_path_monitor_cancel(monitor: nw_path_monitor_t);

    pub fn nw_release(obj: *mut c_void);
}