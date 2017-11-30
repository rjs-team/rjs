
#![feature(const_fn)]
#![feature(libc)]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate mozjs;
extern crate libc;
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;
extern crate rjs;


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
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use mozjs::conversions::{FromJSValConvertible, ToJSValConvertible};
use mozjs::conversions::ConversionResult;

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

use rjs::jslib::safefn::myDefineFunction;

macro_rules! js_fn {
    ($name:ident |$($param:ident : $type:ty),*| -> Result<JSVal, Option<String>> $body:tt) => (
    	#[allow(non_snake_case)] 
    	unsafe extern "C" fn $name (cx: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
			let args = CallArgs::from_vp(vp, argc);
			let rt = JS_GetRuntime(cx);
			let rcx = JS_GetRuntimePrivate(rt) as *mut RJSContext;
            assert!((*rcx).cx == cx);

			let result = (|$($param : $type),*| -> Result<JSVal, Option<String>> $body) (cx, &*rcx, args);
            match result {
                Ok(v) => {
                    args.rval().set(v);
                    true
                },
                Err(Some(s)) => {
                    let cstr = CString::new(s).unwrap();
		            JS_ReportError(cx, cstr.as_ptr() as *const libc::c_char);
                    false
                },
                Err(None) => {
                    false
                },
            }

    	}
    )
}


macro_rules! js_callback {
    ($rcx:ident, move |$($param:ident : $type:ty),*| $body:tt) => (
    	(move |tx: Arc<oneshot::Sender<()>>| {
    		move |$($param : $type),*| {
				let _ac = JSAutoCompartment::new($rcx.cx, $rcx.global.get());

				let ret = (|$($param : $type),*| $body) ($($param),*);

				drop(tx);

				ret
			}
		})($rcx.tx.upgrade().unwrap())
    )
}


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

    let res = core.run(future::ok(()).then(|_: Result<(), ()>| {


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
    })));

    match res {
        Ok(a) => println!("done: {:?}", a),
        Err(e) => println!("cancelled: {:?}", e),
    }
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


js_fn!{puts |cx: *mut JSContext, _rcx: &'static RJSContext, args: CallArgs| -> Result<JSVal, Option<String>> {
	if args._base.argc_ != 1 {
		return Err(Some("puts() requires exactly 1 argument".into()));
	}

	// let arg = args.get(0);
	// let arg = String::from_jsval(cx, arg, ()).unwrap();
	// let arg = arg.get_success_value().unwrap();

	let arg = String::from_jsval(cx, args.get(0), ()).to_result().unwrap_or_else(|err| String::from(err.unwrap_or("".into())));
    //let arg = str_from_js(cx, args.get(0));
	println!("{}", arg);

	// rooted!(in(cx) let message_root = mozjs::rust::ToString(cx, arg));
	// let message_ptr = JS_EncodeStringToUTF8(cx, message_root.handle());
	// let message = CStr::from_ptr(message_ptr);
	// println!("{}", str::from_utf8(message.to_bytes()).unwrap());
	// JS_free(cx, message_ptr as *mut c_void);

	//args.rval().set(UndefinedValue());
	//return true;
    Ok(UndefinedValue())
}}


js_fn!{setTimeout |cx: *mut JSContext, rcx: &'static RJSContext, args: CallArgs| -> Result<JSVal, Option<String>> {
	if args._base.argc_ != 2 {
		return Err(Some("setTimeout() requires exactly 2 arguments".into()));
	}

	let callback = args.get(0);
	let timeout = args.get(1);

	let callback = Heap::new(callback.get());
	let timeout = mozjs::rust::ToUint64(cx, timeout).unwrap();
	
	let timeout = Timeout::new(Duration::from_millis(timeout), &rcx.handle).unwrap();

	rcx.handle.spawn(
		timeout.map_err(|_|()).and_then(js_callback!(rcx, move|_a:()| {
			rooted!(in(cx) let this_val = (*rcx).global.get());
			rooted!(in(cx) let mut rval = UndefinedValue());

			let ok = JS_CallFunctionValue(
				cx, 
				this_val.handle(),
				callback.handle(),
				&jsapi::HandleValueArray {
					elements_: ptr::null_mut(),
					length_: 0,
				},
				rval.handle_mut());

			if !ok {
				println!("error!");
				report_pending_exception(cx);
			}
				
			
			Ok(())
		}))
	);

	//args.rval().set(UndefinedValue());
	//true
    Ok(UndefinedValue())
}}

js_fn!{getFileSync |cx: *mut JSContext, _rcx: &'static RJSContext, args: CallArgs| -> Result<JSVal, Option<String>> {
	if args._base.argc_ != 1 {
		return Err(Some("getFileSync() requires exactly 1 arguments".into()));
	}

	let path = String::from_jsval(cx, args.get(0), ()).to_result()?;

	if let Ok(mut file) = File::open(path) {
		let mut contents = String::new();
		file.read_to_string(&mut contents).unwrap();

        rooted!{in(cx) let mut ret = UndefinedValue()};
		contents.to_jsval(cx, ret.handle_mut());
        Ok(*ret)
	} else {
		//args.rval().set(UndefinedValue());
        Ok(UndefinedValue())
	}
	// args.rval().set();
	//true
}}

js_fn!{readDir |cx: *mut JSContext, _rcx: &'static RJSContext, args: CallArgs| -> Result<JSVal, Option<String>> {
	if args._base.argc_ != 1 {
		return Err(Some("readDir() requires exactly 1 arguments".into()));
	}

	rooted!(in(cx) let arr = JS_NewArrayObject1(cx, 0));

	let path = args.get(0);
	let path = String::from_jsval(cx, path, ()).unwrap();
	let path = path.get_success_value().unwrap();

	rooted!(in(cx) let mut temp = UndefinedValue());

	for (i, entry) in fs::read_dir(Path::new(path)).unwrap().enumerate() {
        let entry = entry.unwrap();
        let path = entry.path();

        path.to_str().unwrap().to_jsval(cx, temp.handle_mut());
        JS_SetElement(cx, arr.handle(), i as u32, temp.handle());
    }

    arr.to_jsval(cx, args.rval());
	// args.rval().set(arr.get());
	Ok(ObjectValue(*arr))
}}

unsafe fn report_pending_exception(cx: *mut JSContext) {
	rooted!(in(cx) let mut ex = UndefinedValue());
	if !jsapi::JS_GetPendingException(cx, ex.handle_mut())
		{ return; }

	let ex = String::from_jsval(cx, ex.handle(), ()).unwrap();
	let ex = ex.get_success_value().unwrap();
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
