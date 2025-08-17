use std::{ptr, slice};

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString};
use objc2_core_graphics::kCGColorSpaceSRGB;
use objc2_core_video::kCVPixelFormatType_32BGRA;
use objc2_io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceColorSpace, kIOSurfaceHeight, kIOSurfacePixelFormat,
    kIOSurfaceWidth, IOSurfaceLockOptions, IOSurfaceRef,
};
use objc2_quartz_core::{kCAFilterNearest, kCAGravityTopLeft, CALayer};

use libc::kern_return_t;

use super::OsError;
use crate::{Error, Result};

#[allow(non_upper_case_globals)]
const kIOSurfaceSuccess: kern_return_t = 0;

const BYTES_PER_ELEMENT: usize = 4;

unsafe fn set_contents_opaque(layer: &CALayer, contents_opaque: bool) {
    let () = msg_send![layer, setContentsOpaque: contents_opaque];
}

unsafe fn set_contents_changed(layer: &CALayer) {
    let () = msg_send![layer, setContentsChanged];
}

pub struct Surface {
    pub layer: Retained<CALayer>,
    pub surface: CFRetained<IOSurfaceRef>,
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
                    &CFNumber::new_i32(kCVPixelFormatType_32BGRA as i32),
                ],
            );

            let Some(surface) = IOSurfaceRef::new(properties.as_opaque()) else {
                return Err(Error::Os(OsError::Other("could not create IOSurface")));
            };

            surface.set_value(kIOSurfaceColorSpace, kCGColorSpaceSRGB);

            let layer = CALayer::layer();
            let surface_ptr = CFRetained::as_ptr(&surface).as_ptr();
            layer.setContents(Some(&*(surface_ptr as *const AnyObject)));
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
            let ret = self.surface.lock(IOSurfaceLockOptions::empty(), ptr::null_mut());
            if ret != kIOSurfaceSuccess {
                return;
            }

            let addr = self.surface.base_address().as_ptr();
            let buffer = slice::from_raw_parts_mut(addr as *mut u32, self.width * self.height);
            f(buffer);

            self.surface.unlock(IOSurfaceLockOptions::empty(), ptr::null_mut());
        }
    }

    pub fn present(&self) {
        unsafe {
            set_contents_changed(&self.layer);
        }
    }
}
