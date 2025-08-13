#![allow(unused)]
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;

use super::Boolean;

#[repr(C)]
pub struct __CVDisplayLink(c_void);

pub type CVDisplayLinkRef = *const __CVDisplayLink;

pub type CGDirectDisplayID = u32;

pub type CVOptionFlags = u64;

pub type CVReturn = i32;

pub const kCVReturnSuccess: CVReturn = 0;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CVTimeStamp {
    pub version: u32,
    pub videoTimeScale: i32,
    pub videoTime: i64,
    pub hostTime: u64,
    pub rateScalar: f64,
    pub videoRefreshPeriod: i64,
    pub smpteTime: CVSMPTETime,
    pub flags: u64,
    pub reserved: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CVSMPTETime {
    pub subframes: i16,
    pub subframeDivisor: i16,
    pub counter: u32,
    pub type_: u32,
    pub flags: u32,
    pub hours: i16,
    pub minutes: i16,
    pub seconds: i16,
    pub frames: i16,
}

pub type CVDisplayLinkOutputCallback = unsafe extern "C" fn(
    displayLink: CVDisplayLinkRef,
    inNow: *const CVTimeStamp,
    inOutputTime: *const CVTimeStamp,
    flagsIn: CVOptionFlags,
    flagsOut: *mut CVOptionFlags,
    displayLinkContext: *mut c_void,
) -> CVReturn;

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    pub fn CVDisplayLinkCreateWithCGDisplay(
        displayID: CGDirectDisplayID,
        displayLinkOut: *mut CVDisplayLinkRef,
    ) -> CVReturn;
    pub fn CVDisplayLinkSetOutputCallback(
        displayLink: CVDisplayLinkRef,
        callback: CVDisplayLinkOutputCallback,
        userInfo: *mut c_void,
    ) -> CVReturn;
    pub fn CVDisplayLinkStart(displayLink: CVDisplayLinkRef) -> CVReturn;
    pub fn CVDisplayLinkStop(displayLink: CVDisplayLinkRef) -> CVReturn;
    pub fn CVDisplayLinkIsRunning(displayLink: CVDisplayLinkRef) -> Boolean;
    pub fn CVDisplayLinkRelease(displayLink: CVDisplayLinkRef);
}
