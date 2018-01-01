// THIS seems to be difficult currently, since a JSClass in rust doesn't have a call field :(

// use tokio_core::reactor::{Core, Handle, Timeout};
// use futures::Future;
// use futures::future;
// // use futures::future::{FutureResult};
// // use tokio_timer::{Timer, TimerError};
// use futures::sync::oneshot;

//use libc;

// use std::os::raw::c_void;
// use mozjs::jsapi::CallArgs;
// use mozjs::jsapi::CompartmentOptions;
// use mozjs::jsapi::Heap;
// use mozjs::jsapi::JSClass;
// use mozjs::jsapi::JSFunctionSpec;
// use mozjs::jsapi::JSAutoCompartment;
//use mozjs::jsapi::JSContext;
//use mozjs::jsapi::JSFunction;
// use mozjs::jsapi::JS_CallFunctionValue;
//use mozjs::jsapi::JS_DefineFunction;
// use mozjs::jsapi::JS_EncodeStringToUTF8;
// use mozjs::jsapi::JS_free;
// use mozjs::jsapi::JS_GetRuntime;
// use mozjs::jsapi::JS_GetRuntimePrivate;
// use mozjs::jsapi::JS_Init;
// use mozjs::jsapi::JS_InitStandardClasses;
// use mozjs::jsapi::JS_NewGlobalObject;
// use mozjs::jsapi::JS_ReportError;
// // use mozjs::jsapi::JS_SetGCZeal; // seems to be missing
// use mozjs::jsapi::JS_SetRuntimePrivate;
// use mozjs::jsapi::OnNewGlobalHookOption;
//use mozjs::jsapi::Value;
// use mozjs::jsval::{NullValue, UndefinedValue};
//use mozjs::jsapi::{ HandleObject};
// use mozjs::jsapi::{JS_InitClass, JSCLASS_HAS_PRIVATE};
// use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};

// use std::ptr;
// use std::env;
// use std::fs::File;
// // use std::io;
// use std::ffi::CStr;
// use std::str;
// use std::io::Read;
// use std::time::{Duration};
// use std::sync::{Arc, Weak};
//use std::ffi::CString;

// // const METHODS: &'static [JSFunctionSpec] = &[
// //     JSFunctionSpec {
// //         name: b"addEventListener\0" as *const u8 as *const libc::c_char,
// //         call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
// //         nargs: 2,
// //         flags: JSPROP_ENUMERATE as u16,
// //         selfHostedName: 0 as *const libc::c_char
// //     },
// //     JSFunctionSpec {
// //         name: b"removeEventListener\0" as *const u8 as *const libc::c_char,
// //         call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
// //         nargs: 2,
// //         flags: JSPROP_ENUMERATE as u16,
// //         selfHostedName: 0 as *const libc::c_char
// //     },
// //     JSFunctionSpec {
// //         name: b"dispatchEvent\0" as *const u8 as *const libc::c_char,
// //         call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
// //         nargs: 1,
// //         flags: JSPROP_ENUMERATE as u16,
// //         selfHostedName: 0 as *const libc::c_char
// //     },
// //     JSFunctionSpec {
// //         name: ptr::null(),
// //         call: JSNativeWrapper { op: None, info: ptr::null() },
// //         nargs: 0,
// //         flags: 0,
// //         selfHostedName: ptr::null()
// //     }
// // ];

// pub struct JSFullClass {
//     pub name: *const ::std::os::raw::c_char,
//     pub flags: u32,
//     pub cOps: *const JSClassOps,
//     pub reserved: [*mut ::std::os::raw::c_void; 3usize],
// }

// static SAFE_FUNCTION_CLASS: JSClass = JSClass {
//     name: b"RustSafeFunction\0" as *const u8 as *const libc::c_char,
//     call: safe_function,
//     flags: JSCLASS_HAS_PRIVATE,
//     cOps: 0 as *const _,
//     reserved: [0 as *mut _; 3]
// };

// unsafe extern "C" fn safe_function(_: *mut JSContext, _: u32, _: *mut Value) -> bool {
//     true
// }

// unsafe fn initSafeFunctions(cx: *mut JSContext, obj: HandleObject) {
//     JS_InitClass(cx, global, NullValue(), &SAFE_FUNCTION_CLASS, ptr::null(), 0,
//                ptr::null(), ptr::null(), ptr::null(), ptr::null());
// }

/*pub unsafe fn myDefineFunction(
    cx: *mut JSContext, 
    this: HandleObject, 
    name: &str, 
    f: unsafe extern "C" fn(cx: *mut JSContext, argc: u32, vp: *mut Value) -> bool,
    nargs: u32,
    flags: u32) -> *mut JSFunction {
    JS_DefineFunction(cx, this, CString::new(name).unwrap().as_ptr() as *const libc::c_char, Some(f), nargs, flags)
}
*/
