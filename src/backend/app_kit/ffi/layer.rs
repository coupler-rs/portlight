#![allow(non_snake_case)]

use objc2::mutability::InteriorMutable;
use objc2::rc::Id;
use objc2::runtime::AnyObject;
use objc2::{extern_class, extern_methods, ClassType};

use objc2_foundation::{CGFloat, NSObject, NSString};

pub type CALayerContentsFilter = NSString;

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {
    pub static kCAFilterNearest: &'static CALayerContentsFilter;
    pub static kCAGravityTopLeft: &'static CALayerContentsFilter;
}

pub type CALayerContentsGravity = NSString;

extern_class!(
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct CALayer;

    unsafe impl ClassType for CALayer {
        type Super = NSObject;
        type Mutability = InteriorMutable;
    }
);

extern_methods!(
    unsafe impl CALayer {
        #[method_id(@__retain_semantics Other layer)]
        pub fn layer() -> Id<Self>;

        #[method(setContents:)]
        pub unsafe fn setContents(&self, contents: Option<&AnyObject>);

        #[method(setOpaque:)]
        pub fn setOpaque(&self, opaque: bool);

        #[method(setContentsGravity:)]
        pub fn setContentsGravity(&self, contents_gravity: &CALayerContentsGravity);

        #[method(setMagnificationFilter:)]
        pub fn setMagnificationFilter(&self, magnification_filter: &CALayerContentsFilter);

        #[method(setContentsScale:)]
        pub fn setContentsScale(&self, contents_scale: CGFloat);
    }
);
