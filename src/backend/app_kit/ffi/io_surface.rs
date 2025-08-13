#![allow(unused)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]

use std::ffi::{c_int, c_void};

use core_foundation::base::CFTypeRef;
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::string::CFStringRef;

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
    pub static kIOSurfaceWidth: CFStringRef;
    pub static kIOSurfaceHeight: CFStringRef;
    pub static kIOSurfaceBytesPerElement: CFStringRef;
    pub static kIOSurfacePixelFormat: CFStringRef;
    pub static kIOSurfaceColorSpace: CFStringRef;

    pub static kCGColorSpaceSRGB: CFStringRef;

    pub fn IOSurfaceCreate(properties: CFDictionaryRef) -> IOSurfaceRef;
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
    pub fn IOSurfaceSetValue(buffer: IOSurfaceRef, key: CFStringRef, value: CFTypeRef);
    pub fn IOSurfaceIsInUse(buffer: IOSurfaceRef) -> Boolean;
}
