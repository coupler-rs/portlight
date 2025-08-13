use std::{mem, ptr, slice};

use objc2::msg_send;
use objc2::rc::Id;

use super::ffi::layer::*;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

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
    pub layer: Id<CALayer>,
    pub surface: IOSurfaceRef,
    pub width: usize,
    pub height: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Result<Surface> {
        unsafe {
            let properties = CFDictionary::from_CFType_pairs(&[
                (
                    CFString::wrap_under_get_rule(kIOSurfaceWidth),
                    CFNumber::from(width as i32).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kIOSurfaceHeight),
                    CFNumber::from(height as i32).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kIOSurfaceBytesPerElement),
                    CFNumber::from(BYTES_PER_ELEMENT as i32).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kIOSurfacePixelFormat),
                    CFNumber::from(kCVPixelFormatType_32BGRA).as_CFType(),
                ),
            ]);

            let surface = IOSurfaceCreate(properties.as_concrete_TypeRef());
            if surface.is_null() {
                return Err(Error::Os(OsError::Other("could not create IOSurface")));
            }

            IOSurfaceSetValue(
                surface,
                kIOSurfaceColorSpace,
                kCGColorSpaceSRGB as CFTypeRef,
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
            CFRelease(self.surface as CFTypeRef);
        }
    }
}
