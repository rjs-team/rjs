
#![feature(const_fn)]
#![feature(libc)]
#![feature(trace_macros)]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate mozjs;
extern crate libc;
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;
#[macro_use]
extern crate rjs;
#[macro_use]
extern crate lazy_static;



use tokio_core::reactor::{Core, Handle, Timeout};
use futures::Future;
use futures::future;
// use futures::future::{FutureResult};
// use tokio_timer::{Timer, TimerError};
use futures::sync::oneshot;

use std::os::raw::c_void;
use mozjs::jsapi;
use jsapi::CallArgs;
use jsapi::CompartmentOptions;
use jsapi::Heap;
use jsapi::JSAutoCompartment;
use jsapi::JSContext;
//use jsapi::JSFunction;
use jsapi::JS_CallFunctionValue;
//use jsapi::JS_DefineFunction;
use jsapi::JS_EncodeStringToUTF8;
use jsapi::JS_free;
use jsapi::JS_GetRuntime;
use jsapi::JS_GetRuntimePrivate;
use jsapi::JS_Init;
//use jsapi::JS_InitStandardClasses;
use jsapi::JS_NewGlobalObject;
use jsapi::JS_ReportError;
use jsapi::{JS_NewArrayObject1, JS_SetElement};
// use jsapi::JS_SetGCZeal; // seems to be missing
use jsapi::JS_SetRuntimePrivate;
use jsapi::OnNewGlobalHookOption;
use jsapi::Value;
use mozjs::jsval;
use jsval::JSVal;
use jsval::{ObjectValue, UndefinedValue};
use jsapi::{HandleObject};
use mozjs::jsapi::{ JSPROP_ENUMERATE, JSPROP_SHARED };
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use mozjs::conversions::{FromJSValConvertible, ToJSValConvertible};
use mozjs::conversions::ConversionResult;
//use rjs::jslib::jsclass;
use rjs::jslib::jsfn::{JSRet};
use rjs::jslib::jsclass::{JSClassInitializer, null_function, null_property, null_wrapper, jsclass_has_reserved_slots};
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSFunctionSpec;
use mozjs::jsapi::JSNativeWrapper;
use mozjs::jsapi::JSPropertySpec;

use std::ptr;
use std::env;
use std::fs;
use std::fs::File;
use std::path::Path;
// use std::io;
use std::ffi::CStr;
use std::str;
use std::io::Read;
use std::time::{Duration};
use std::sync::{Arc, Weak};
use std::ffi::CString;
use std::marker::PhantomData;
use std::fmt;
use std::fmt::Display;
use std::sync::Mutex;
use std::sync::{Once, ONCE_INIT};

use rjs::jslib::safefn::myDefineFunction;




fn main() {
    let filename = env::args().nth(1)
        .expect("Expected a filename as the first argument");

    let mut file = File::open(&filename).expect("File is missing");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Cannot read file");

    unsafe { JS_Init(); }

    // println!("JS_Init()");


    let rt = Runtime::new().unwrap();
    // JS_SetGCZeal(rt.rt(), 2, 1);

    let cx = rt.cx();

    rooted!(in(cx) let global_root =
        unsafe { JS_NewGlobalObject(cx, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(),
                           OnNewGlobalHookOption::FireOnNewGlobalHook,
                           &CompartmentOptions::default()) }
    );
    let global = global_root.handle();
    // println!("JS_NewGlobalObject()");

    let mut core = Core::new().unwrap();

    // this is used to keep track of all pending daemons and callbacks, when there are no more handles to tx, rx will join and the main thread will exit
    let (tx, rx) = oneshot::channel::<()>(); 
    let tx = Arc::new(tx);

    let rcx = Box::into_raw(Box::new(RJSContext {
        cx: cx,
        global: global,
        handle: core.handle(),
        tx: Arc::downgrade(&tx),
    }));
    unsafe { JS_SetRuntimePrivate(rt.rt(), rcx as *mut c_void) };



    // (*rcx).handle.spawn_fn(|| -> Result<(), ()> {
    // });

    core.run(future::ok(()).then(|_: Result<(), ()>| {


        // let res = JS_InitStandardClasses(cx, global);
        // println!("JS_InitStandardClasses()");

        let _ac = JSAutoCompartment::new(cx, global.get());
        unsafe {
            let _ = myDefineFunction(cx, global, "puts", puts, 1, 0);
            let _ = myDefineFunction(cx, global, "setTimeout", setTimeout, 2, 0);
            let _ = myDefineFunction(cx, global, "getFileSync", getFileSync, 1, 0);
            let _ = myDefineFunction(cx, global, "readDir", readDir, 1, 0);
        }


        rooted!(in(cx) let mut rval = UndefinedValue());
        assert!(rt.evaluate_script(global, &contents,
                                   &filename, 1, rval.handle_mut()).is_ok());

        println!("script result: {}", str_from_js(cx, rval));


        Ok(drop(tx))

    }).join(rx.then(|res| -> Result<(), ()> {
        match res {
            Ok(()) => println!("done"),
            Err(e) => println!("cancelled {:?}", e),
        }

        Ok(())
    }))).unwrap();
}

struct JSString<'a> {
    js_str: *mut libc::c_char,
    cx: *mut JSContext,
    marker: PhantomData<&'a JSString<'a>>,
}

fn str_from_js<'a>(cx: *mut JSContext, val: mozjs::rust::RootedGuard<'a, mozjs::jsval::JSVal>) -> JSString<'a> {
    rooted!(in(cx) let message_root = unsafe { mozjs::rust::ToString(cx, val.handle()) });
    let message_ptr = unsafe { JS_EncodeStringToUTF8(cx, message_root.handle()) };

    JSString { js_str: message_ptr, cx: cx, marker: PhantomData }
}

impl<'a> Display for JSString<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let message = unsafe { CStr::from_ptr(self.js_str) };

        f.write_str(unsafe { str::from_utf8_unchecked(message.to_bytes()) })
    }
}

impl<'a> Drop for JSString<'a> {
    fn drop(&mut self) {
        unsafe { JS_free(self.cx, self.js_str as *mut c_void); }
    }

}

trait ToResult<T> {
    fn to_result(self) -> Result<T, Option<String> >;
}

impl<T> ToResult<T> for Result<mozjs::conversions::ConversionResult<T>, ()> {
    fn to_result(self) -> Result<T, Option<String>> {
        match self {
            Ok(ConversionResult::Success(v)) => Result::Ok(v),
            Ok(ConversionResult::Failure(reason)) => Result::Err(Some(reason.into_owned())),
            Err(()) => Result::Err(None),
        }
    }
}


js_fn!{fn puts(_rcx: &'static RJSContext, arg: String) -> JSRet<()> {
    println!("{}", arg);
    Ok(())
}}


js_fn!{fn setTimeout(rcx: &'static RJSContext, callback: Heap<JSVal>, timeout: u64 {mozjs::conversions::ConversionBehavior::Default}) -> JSRet<()> {
    let timeout = Timeout::new(Duration::from_millis(timeout), &rcx.handle).unwrap();

    rcx.handle.spawn(
        timeout.map_err(|_|()).and_then(js_callback!(rcx, move|_a:()| {
            rooted!(in(rcx.cx) let this_val = (*rcx).global.get());
            rooted!(in(rcx.cx) let mut rval = UndefinedValue());

            unsafe {
            let ok = JS_CallFunctionValue(
                rcx.cx, 
                this_val.handle(),
                callback.handle(),
                &jsapi::HandleValueArray {
                    elements_: ptr::null_mut(),
                    length_: 0,
                },
                rval.handle_mut());

            if !ok {
                println!("error!");
                report_pending_exception(rcx.cx);
            }
            }


            Ok(())
        }))
    );

    //args.rval().set(UndefinedValue());
    //true
    Ok(())
}}

js_fn!{fn getFileSync(rcx: &'static RJSContext, path: String) -> JSRet<Option<String>> {
    if let Ok(mut file) = File::open(path) {
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();

        Ok(Some(contents))
    } else {
        //args.rval().set(UndefinedValue());
        Ok(None)
    }
    // args.rval().set();
    //true
}}

js_fn!{fn readDir(rcx: &'static RJSContext, path: String) -> JSRet<JSVal> {
    unsafe{
    rooted!(in(rcx.cx) let arr = JS_NewArrayObject1(rcx.cx, 0));
    rooted!(in(rcx.cx) let mut temp = UndefinedValue());

    for (i, entry) in fs::read_dir(Path::new(&path)).unwrap().enumerate() {
        let entry = entry.unwrap();
        let path = entry.path();

        path.to_str().unwrap().to_jsval(rcx.cx, temp.handle_mut());
        JS_SetElement(rcx.cx, arr.handle(), i as u32, temp.handle());
    }

    //arr.to_jsval(rcx.cx, args.rval());
    // args.rval().set(arr.get());
    Ok(ObjectValue(*arr))
    }
}}

unsafe fn report_pending_exception(cx: *mut JSContext) {
    rooted!(in(cx) let mut ex = UndefinedValue());
    if !jsapi::JS_GetPendingException(cx, ex.handle_mut())
        { return; }

    let ex = String::from_jsval(cx, ex.handle(), ()).to_result().unwrap();
    println!("Exception!: {}", ex);

    // rooted!(in(cx) let message_root = mozjs::rust::ToString(cx, ex.handle()));
    // let message_ptr = JS_EncodeStringToUTF8(cx, message_root.handle());
    // let message = CStr::from_ptr(message_ptr);
    // println!("{}", str::from_utf8(message.to_bytes()).unwrap());
    // JS_free(cx, message_ptr as *mut c_void);
}

#[derive(Debug)]
struct RJSContext {
    cx: *mut JSContext,
    global: HandleObject,
    handle: Handle,
    tx: Weak<oneshot::Sender<()>>,
    // timer: Timer,
}

impl RJSContext {
    // fn enter_js(f: fn() -> bool) {

    // }
}


struct Test {

}

js_class!{ Test

    fn test_puts(_rcx: &'static RJSContext, arg: String) -> JSRet<()> {
        println!("{}", arg);
        Ok(())
    }

    @prop test_prop {
        get fn Test_get_test_prop(_rcx: &'static RJSContext) -> JSRet<String> {
            Ok(String::from("Test prop"))
        }
    }

}
