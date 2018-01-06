use jslib::eventloop;
use mozjs::jsapi::{HandleObject, JSContext, JS_GetCompartmentPrivate, JS_SetCompartmentPrivate};
use mozjs::rust::get_context_compartment;

use std::collections::HashMap;
use std::any::TypeId;
use std::os::raw::c_void;
use std::ptr;

pub struct RJSContext {
    pub cx: *mut JSContext,
    pub global: HandleObject,
    cls_protos: HashMap<TypeId, ClassInfo>,
}

struct ClassInfo {}

impl RJSContext {
    pub fn new(cx: *mut JSContext, global: HandleObject) -> RJSContext {
        RJSContext {
            cx: cx,
            global: global,
            cls_protos: HashMap::new(),
        }
    }
}

pub type RJSHandle = eventloop::Handle<RJSContext>;
pub type RJSRemote = eventloop::Remote<RJSContext>;

pub type RuntimePrivate = eventloop::WeakHandle<RJSContext>;

pub fn store_private(cx: *mut JSContext, handle: &RJSHandle) {
    let compartment = unsafe { get_context_compartment(cx) };
    let private = Box::new(handle.downgrade());
    unsafe {
        JS_SetCompartmentPrivate(compartment, Box::into_raw(private) as *mut c_void);
    }
}

pub fn clear_private(cx: *mut JSContext) {
    let compartment = unsafe { get_context_compartment(cx) };
    let private = unsafe { JS_GetCompartmentPrivate(compartment) as *mut RuntimePrivate };
    if !private.is_null() {
        unsafe {
            let _ = Box::from_raw(private);
            JS_SetCompartmentPrivate(compartment, ptr::null_mut());
        }
    }
}

pub fn get_handle(cx: *mut JSContext) -> Option<RJSHandle> {
    let compartment = unsafe { get_context_compartment(cx) };
    let private = unsafe { JS_GetCompartmentPrivate(compartment) as *const RuntimePrivate };
    if private.is_null() {
        None
    } else {
        unsafe { (*private).upgrade() }
    }
}
