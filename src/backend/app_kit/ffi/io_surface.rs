#![allow(unused)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use std::ffi::{c_int, c_void};

use objc2_core_foundation::{CFDictionary, CFString, CFType};

use super::Boolean;

#[repr(C)]
pub struct __IOSurface(c_void);

pub type IOSurfaceRef = *const __IOSurface;

pub type kern_return_t = c_int;

pub type IOSurfaceLockOptions = u32;

pub const kIOSurfaceLockReadOnly: IOSurfaceLockOptions = 0x00000001;
pub const kIOSurfaceLockAvoidSync: IOSurfaceLockOptions = 0x00000002;

pub const kIOSurfaceSuccess: kern_return_t = 0;

pub const kCVPixelFormatType_32BGRA: i32 = 0x42475241; // 'BGRA'

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    pub static kIOSurfaceWidth: &'static CFString;
    pub static kIOSurfaceHeight: &'static CFString;
    pub static kIOSurfaceBytesPerElement: &'static CFString;
    pub static kIOSurfacePixelFormat: &'static CFString;
    pub static kIOSurfaceColorSpace: &'static CFString;

    pub static kCGColorSpaceSRGB: &'static CFString;

    pub fn IOSurfaceCreate(properties: *const CFDictionary) -> IOSurfaceRef;
    pub fn IOSurfaceLock(
        buffer: IOSurfaceRef,
        options: IOSurfaceLockOptions,
        seed: *mut u32,
    ) -> kern_return_t;
    pub fn IOSurfaceUnlock(
        buffer: IOSurfaceRef,
        options: IOSurfaceLockOptions,
        seed: *mut u32,
    ) -> kern_return_t;
    pub fn IOSurfaceGetBaseAddress(buffer: IOSurfaceRef) -> *mut c_void;
    pub fn IOSurfaceSetValue(buffer: IOSurfaceRef, key: *const CFString, value: *const CFType);
    pub fn IOSurfaceIsInUse(buffer: IOSurfaceRef) -> Boolean;
}
