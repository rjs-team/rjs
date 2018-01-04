use jslib::eventloop;
use mozjs::jsapi::{HandleObject, JSContext, JS_GetCompartmentPrivate, JS_SetCompartmentPrivate};
use mozjs::rust::get_context_compartment;

use std::os::raw::c_void;

#[derive(Debug)]
pub struct RJSContext {
    pub cx: *mut JSContext,
    pub global: HandleObject,
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

pub fn get_handle(cx: *mut JSContext) -> Option<RJSHandle> {
    let compartment = unsafe { get_context_compartment(cx) };
    let private = unsafe { JS_GetCompartmentPrivate(compartment) as *const RuntimePrivate };
    if private.is_null() {
        None
    } else {
        unsafe { (*private).upgrade() }
    }
}
