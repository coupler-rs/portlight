use std::{ptr, slice};

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString};
use objc2_core_graphics::kCGColorSpaceSRGB;
use objc2_core_video::kCVPixelFormatType_32BGRA;
use objc2_io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceBytesPerRow, kIOSurfaceColorSpace, kIOSurfaceHeight,
    kIOSurfacePixelFormat, kIOSurfaceWidth, IOSurfaceLockOptions, IOSurfaceRef,
};

use libc::kern_return_t;

use super::OsError;
use crate::{Bitmap, Error, Result};

#[allow(non_upper_case_globals)]
const kIOSurfaceSuccess: kern_return_t = 0;

const BYTES_PER_ELEMENT: usize = 4;

pub struct Surface {
    surface: CFRetained<IOSurfaceRef>,
    width: usize,
    height: usize,
    stride: usize,
}

impl Surface {
    pub fn new(width: usize, height: usize) -> Result<Surface> {
        let bytes_per_row = IOSurfaceRef::align_property(
            unsafe { kIOSurfaceBytesPerRow },
            width * BYTES_PER_ELEMENT,
        );
        let stride = bytes_per_row / BYTES_PER_ELEMENT;

        let properties = CFDictionary::<CFString, CFNumber>::from_slices(
            &[
                unsafe { kIOSurfaceWidth },
                unsafe { kIOSurfaceHeight },
                unsafe { kIOSurfaceBytesPerElement },
                unsafe { kIOSurfaceBytesPerRow },
                unsafe { kIOSurfacePixelFormat },
            ],
            &[
                &CFNumber::new_i32(width as i32),
                &CFNumber::new_i32(height as i32),
                &CFNumber::new_i32(BYTES_PER_ELEMENT as i32),
                &CFNumber::new_i32(bytes_per_row as i32),
                &CFNumber::new_i32(kCVPixelFormatType_32BGRA as i32),
            ],
        );

        let Some(surface) = (unsafe { IOSurfaceRef::new(properties.as_opaque()) }) else {
            return Err(Error::Os(OsError::Other("could not create IOSurface")));
        };

        unsafe {
            surface.set_value(kIOSurfaceColorSpace, kCGColorSpaceSRGB);
        }

        Ok(Surface {
            surface,
            width,
            height,
            stride,
        })
    }

    pub fn as_ptr(&self) -> *const IOSurfaceRef {
        CFRetained::as_ptr(&self.surface).as_ptr()
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn update(&self, bitmap: Bitmap) {
        let ret = unsafe { self.surface.lock(IOSurfaceLockOptions::empty(), ptr::null_mut()) };
        if ret != kIOSurfaceSuccess {
            return;
        }

        let addr = self.surface.base_address().as_ptr();
        let len = self.stride * self.height;
        let buffer = unsafe { slice::from_raw_parts_mut(addr as *mut u32, len) };

        let copy_width = bitmap.width().min(self.width);
        let copy_height = bitmap.height().min(self.height);

        for row in 0..copy_height {
            let src = &bitmap.data()[row * bitmap.width()..row * bitmap.width() + copy_width];
            let dst = &mut buffer[row * self.stride..row * self.stride + copy_width];
            dst.copy_from_slice(src);
        }

        unsafe {
            self.surface.unlock(IOSurfaceLockOptions::empty(), ptr::null_mut());
        }
    }
}
