#![allow(clippy::not_unsafe_ptr_arg_deref)]
use crate::jslib::eventloop;
use mozjs::jsapi::GetCurrentRealmOrNull;
use mozjs::jsapi::{GetRealmPrivate, HandleObject, JSContext, JSObject, SetRealmPrivate};

use std::any::TypeId;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::ptr;
use std::sync::RwLock;

pub struct RJSContext {
    pub cx: *mut JSContext,
    pub global: HandleObject,
    cls_protos: RwLock<HashMap<TypeId, ClassInfo>>,
}

#[derive(Copy, Clone)]
pub struct ClassInfo {
    pub constr: *mut JSObject,
    pub prototype: *mut JSObject,
}

impl RJSContext {
    pub fn new(cx: *mut JSContext, global: HandleObject) -> RJSContext {
        RJSContext {
            cx,
            global,
            cls_protos: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_classinfo_for<T: 'static>(&self) -> Option<ClassInfo> {
        self.cls_protos
            .read()
            .unwrap()
            .get(&TypeId::of::<T>())
            .copied()
    }
    pub fn set_classinfo_for<T: 'static>(&self, ci: ClassInfo) {
        self.cls_protos
            .write()
            .unwrap()
            .insert(TypeId::of::<T>(), ci);
    }
}

pub type RJSHandle = eventloop::Handle<RJSContext>;
pub type RJSRemote = eventloop::Remote<RJSContext>;

pub type RuntimePrivate = eventloop::WeakHandle<RJSContext>;

pub fn store_private(cx: *mut JSContext, handle: &RJSHandle) {
    let compartment = unsafe { GetCurrentRealmOrNull(cx) };
    let private = Box::new(handle.downgrade());
    unsafe {
        SetRealmPrivate(compartment, Box::into_raw(private) as *mut c_void);
    }
}

pub fn clear_private(cx: *mut JSContext) {
    let compartment = unsafe { GetCurrentRealmOrNull(cx) };
    let private = unsafe { GetRealmPrivate(compartment) as *mut RuntimePrivate };
    if !private.is_null() {
        unsafe {
            let _ = Box::from_raw(private);
            SetRealmPrivate(compartment, ptr::null_mut());
        }
    }
}

pub fn get_handle(cx: *mut JSContext) -> Option<RJSHandle> {
    let compartment = unsafe { GetCurrentRealmOrNull(cx) };
    let private = unsafe { GetRealmPrivate(compartment) as *const RuntimePrivate };
    if private.is_null() {
        None
    } else {
        unsafe { (*private).upgrade() }
    }
}
