use std::ptr::NonNull;
use std::{mem, ptr, slice};

use objc2::msg_send;
use objc2::rc::Retained;

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_quartz_core::{kCAFilterNearest, kCAGravityTopLeft, CALayer};

use super::ffi::io_surface::*;
use super::OsError;
use crate::{Error, Result};

const BYTES_PER_ELEMENT: usize = 4;

unsafe fn set_contents_opaque(layer: &CALayer, contents_opaque: bool) {
    let () = msg_send![layer, setContentsOpaque: contents_opaque];
}

unsafe fn set_contents_changed(layer: &CALayer) {
    let () = msg_send![layer, setContentsChanged];
}

pub struct Surface {
    pub layer: Retained<CALayer>,
    pub surface: IOSurfaceRef,
    pub width: usize,
    pub height: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Result<Surface> {
        unsafe {
            let properties = CFDictionary::<CFString, CFNumber>::from_slices(
                &[
                    kIOSurfaceWidth,
                    kIOSurfaceHeight,
                    kIOSurfaceBytesPerElement,
                    kIOSurfacePixelFormat,
                ],
                &[
                    &CFNumber::new_i32(width as i32),
                    &CFNumber::new_i32(height as i32),
                    &CFNumber::new_i32(BYTES_PER_ELEMENT as i32),
                    &CFNumber::new_i32(kCVPixelFormatType_32BGRA),
                ],
            );

            let surface = IOSurfaceCreate(CFRetained::as_ptr(&properties).as_ptr() as *mut _);
            if surface.is_null() {
                return Err(Error::Os(OsError::Other("could not create IOSurface")));
            }

            IOSurfaceSetValue(
                surface,
                kIOSurfaceColorSpace,
                kCGColorSpaceSRGB as *const CFString as *const CFType,
            );

            let layer = CALayer::layer();
            layer.setContents(Some(mem::transmute(surface)));
            layer.setOpaque(true);
            set_contents_opaque(&layer, true);
            layer.setContentsGravity(kCAGravityTopLeft);
            layer.setMagnificationFilter(kCAFilterNearest);

            Ok(Surface {
                layer,
                surface,
                width,
                height,
            })
        }
    }

    pub fn with_buffer<F: FnOnce(&mut [u32])>(&mut self, f: F) {
        unsafe {
            if IOSurfaceLock(self.surface, 0, ptr::null_mut()) != kIOSurfaceSuccess {
                return;
            }

            let addr = IOSurfaceGetBaseAddress(self.surface);
            let buffer = slice::from_raw_parts_mut(addr as *mut u32, self.width * self.height);
            f(buffer);

            IOSurfaceUnlock(self.surface, 0, ptr::null_mut());
        }
    }

    pub fn present(&self) {
        unsafe {
            set_contents_changed(&self.layer);
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            drop(CFRetained::from_raw(NonNull::new_unchecked(
                self.surface as *mut CFType,
            )));
        }
    }
}
