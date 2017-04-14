
#![feature(const_fn)]
#![feature(libc)]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate js;
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
use js::jsapi::CallArgs;
use js::jsapi::CompartmentOptions;
use js::jsapi::Heap;
use js::jsapi::JSAutoCompartment;
use js::jsapi::JSContext;
use js::jsapi::JSFunction;
use js::jsapi::JS_CallFunctionValue;
use js::jsapi::JS_DefineFunction;
use js::jsapi::JS_EncodeStringToUTF8;
use js::jsapi::JS_free;
use js::jsapi::JS_GetRuntime;
use js::jsapi::JS_GetRuntimePrivate;
use js::jsapi::JS_Init;
use js::jsapi::JS_InitStandardClasses;
use js::jsapi::JS_NewGlobalObject;
use js::jsapi::JS_ReportError;
use js::jsapi::{JS_NewArrayObject1, JS_SetElement};
// use js::jsapi::JS_SetGCZeal; // seems to be missing
use js::jsapi::JS_SetRuntimePrivate;
use js::jsapi::OnNewGlobalHookOption;
use js::jsapi::Value;
use js::jsval::{NullValue, UndefinedValue};
use js::jsapi::{ HandleObject};
use js::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use js::conversions::{FromJSValConvertible, ToJSValConvertible};

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

use rjs::jslib::safefn::myDefineFunction;

macro_rules! js_fn {
    ($name:ident |$($param:ident : $type:ty),*| -> bool $body:tt) => (
    	#[allow(non_snake_case)] 
    	unsafe extern "C" fn $name (cx: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
			let args = CallArgs::from_vp(vp, argc);
			let rt = JS_GetRuntime(cx);
			let rcx = JS_GetRuntimePrivate(rt) as *mut RJSContext;

			(|$($param : $type),*| $body) (cx, &*rcx, args)
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

	unsafe {

		JS_Init();

		// println!("JS_Init()");


		let rt = Runtime::new().unwrap();
		// JS_SetGCZeal(rt.rt(), 2, 1);

		let cx = rt.cx();

		rooted!(in(cx) let global_root =
			JS_NewGlobalObject(cx, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(),
							   OnNewGlobalHookOption::FireOnNewGlobalHook,
							   &CompartmentOptions::default())
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
		JS_SetRuntimePrivate(rt.rt(), rcx as *mut c_void);



		// (*rcx).handle.spawn_fn(|| -> Result<(), ()> {
		// });

		let res = core.run(future::ok(()).then(|_: Result<(), ()>| {


			// let res = JS_InitStandardClasses(cx, global);
			// println!("JS_InitStandardClasses()");

			let _ac = JSAutoCompartment::new(cx, global.get());
			let _ = myDefineFunction(cx, global, "puts", puts, 1, 0);
			let _ = myDefineFunction(cx, global, "setTimeout", setTimeout, 2, 0);
			let _ = myDefineFunction(cx, global, "getFileSync", getFileSync, 1, 0);
			let _ = myDefineFunction(cx, global, "readDir", readDir, 1, 0);


			rooted!(in(cx) let mut rval = UndefinedValue());
			assert!(rt.evaluate_script(global, &contents,
									   &filename, 1, rval.handle_mut()).is_ok());

			rooted!(in(cx) let message_root = js::rust::ToString(cx, rval.handle()));
			let message_ptr = JS_EncodeStringToUTF8(cx, message_root.handle());
			let message = CStr::from_ptr(message_ptr);
			println!("script result: {}", str::from_utf8(message.to_bytes()).unwrap());
			JS_free(cx, message_ptr as *mut c_void);


			Ok(drop(tx))

		}).join(rx.then(|res| -> Result<(), ()> {
			match res {
				Ok(()) => println!("done"),
				Err(_) => println!("cancelled"),
			}

			Ok(())
		})));

		match res {
			Ok(a) => println!("done: ", ),
			Err(e) => println!("cancelled: ", ),
		}
	}
}


js_fn!{puts |cx: *mut JSContext, rcx: &'static RJSContext, args: CallArgs| -> bool {
	if args._base.argc_ != 1 {
		JS_ReportError(cx, b"puts() requires exactly 1 argument\0".as_ptr() as *const libc::c_char);
		return false;
	}

	let arg = args.get(0);
	let arg = String::from_jsval(cx, arg, ()).unwrap();
	let arg = arg.get_success_value().unwrap();
	println!("{}", arg);

	// rooted!(in(cx) let message_root = js::rust::ToString(cx, arg));
	// let message_ptr = JS_EncodeStringToUTF8(cx, message_root.handle());
	// let message = CStr::from_ptr(message_ptr);
	// println!("{}", str::from_utf8(message.to_bytes()).unwrap());
	// JS_free(cx, message_ptr as *mut c_void);

	args.rval().set(UndefinedValue());
	return true;
}}


js_fn!{setTimeout |cx: *mut JSContext, rcx: &'static RJSContext, args: CallArgs| -> bool {
	if args._base.argc_ != 2 {
		JS_ReportError(cx, b"setTimeout() requires exactly 2 arguments\0".as_ptr() as *const libc::c_char);
		return false;
	}

	let callback = args.get(0);
	let timeout = args.get(1);

	let callback = Heap::new(callback.get());
	let timeout = js::rust::ToUint64(cx, timeout).unwrap();
	
	let timeout = Timeout::new(Duration::from_millis(timeout), &rcx.handle).unwrap();

	rcx.handle.spawn(
		timeout.map_err(|_|()).and_then(js_callback!(rcx, move|a:()| {
			rooted!(in(cx) let this_val = (*rcx).global.get());
			rooted!(in(cx) let mut rval = UndefinedValue());

			let ok = JS_CallFunctionValue(
				cx, 
				this_val.handle(),
				callback.handle(),
				&js::jsapi::HandleValueArray {
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

	args.rval().set(UndefinedValue());
	true
}}

js_fn!{getFileSync |cx: *mut JSContext, rcx: &'static RJSContext, args: CallArgs| -> bool {
	if args._base.argc_ != 1 {
		JS_ReportError(cx, b"getFileSync() requires exactly 1 arguments\0".as_ptr() as *const libc::c_char);
		return false;
	}

	let path = args.get(0);
	let path = String::from_jsval(cx, path, ()).unwrap();
	let path = path.get_success_value().unwrap();

	if let Ok(mut file) = File::open(path) {
		let mut contents = String::new();
		file.read_to_string(&mut contents).unwrap();

		contents.to_jsval(cx, args.rval());
	} else {
		args.rval().set(UndefinedValue());
	}
	// args.rval().set();
	true
}}

js_fn!{readDir |cx: *mut JSContext, rcx: &'static RJSContext, args: CallArgs| -> bool {
	if args._base.argc_ != 1 {
		JS_ReportError(cx, b"readDir() requires exactly 1 arguments\0".as_ptr() as *const libc::c_char);
		return false;
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
	true
}}

unsafe fn report_pending_exception(cx: *mut JSContext) {
	rooted!(in(cx) let mut ex = UndefinedValue());
	if !js::jsapi::JS_GetPendingException(cx, ex.handle_mut())
		{ return; }

	let ex = String::from_jsval(cx, ex.handle(), ()).unwrap();
	let ex = ex.get_success_value().unwrap();
	println!("Exception!: {}", ex);

	// rooted!(in(cx) let message_root = js::rust::ToString(cx, ex.handle()));
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
