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
use objc2_quartz_core::{kCAFilterNearest, kCAGravityBottomLeft, CALayer};

use libc::kern_return_t;

use super::OsError;
use crate::{Bitmap, Error, Result};

#[allow(non_upper_case_globals)]
const kIOSurfaceSuccess: kern_return_t = 0;

const BYTES_PER_ELEMENT: usize = 4;

fn set_contents_opaque(layer: &CALayer, contents_opaque: bool) {
    unsafe {
        let () = msg_send![layer, setContentsOpaque: contents_opaque];
    }
}

fn set_contents_changed(layer: &CALayer) {
    unsafe {
        let () = msg_send![layer, setContentsChanged];
    }
}

pub struct Surface {
    pub layer: Retained<CALayer>,
    pub surface: CFRetained<IOSurfaceRef>,
    pub width: usize,
    pub height: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Result<Surface> {
        let properties = CFDictionary::<CFString, CFNumber>::from_slices(
            &[
                unsafe { kIOSurfaceWidth },
                unsafe { kIOSurfaceHeight },
                unsafe { kIOSurfaceBytesPerElement },
                unsafe { kIOSurfacePixelFormat },
            ],
            &[
                &CFNumber::new_i32(width as i32),
                &CFNumber::new_i32(height as i32),
                &CFNumber::new_i32(BYTES_PER_ELEMENT as i32),
                &CFNumber::new_i32(kCVPixelFormatType_32BGRA as i32),
            ],
        );

        let Some(surface) = (unsafe { IOSurfaceRef::new(properties.as_opaque()) }) else {
            return Err(Error::Os(OsError::Other("could not create IOSurface")));
        };

        unsafe {
            surface.set_value(kIOSurfaceColorSpace, kCGColorSpaceSRGB);
        }

        let layer = CALayer::layer();
        let surface_ptr = CFRetained::as_ptr(&surface).as_ptr();
        unsafe {
            layer.setContents(Some(&*(surface_ptr as *const AnyObject)));
        }
        layer.setOpaque(true);
        set_contents_opaque(&layer, true);
        layer.setContentsGravity(unsafe { kCAGravityBottomLeft });
        layer.setMagnificationFilter(unsafe { kCAFilterNearest });

        Ok(Surface {
            layer,
            surface,
            width,
            height,
        })
    }

    pub fn present(&self, bitmap: Bitmap) {
        let ret = unsafe { self.surface.lock(IOSurfaceLockOptions::empty(), ptr::null_mut()) };
        if ret != kIOSurfaceSuccess {
            return;
        }

        let addr = self.surface.base_address().as_ptr();
        let len = self.width * self.height;
        let buffer = unsafe { slice::from_raw_parts_mut(addr as *mut u32, len) };

        let copy_width = bitmap.width().min(self.width);
        let copy_height = bitmap.height().min(self.height);

        for row in 0..copy_height {
            let src = &bitmap.data()[row * bitmap.width()..row * bitmap.width() + copy_width];
            let dst = &mut buffer[row * self.width..row * self.width + copy_width];
            dst.copy_from_slice(src);
        }

        unsafe {
            self.surface.unlock(IOSurfaceLockOptions::empty(), ptr::null_mut());
        }

        set_contents_changed(&self.layer);
    }
}
