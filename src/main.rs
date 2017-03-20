
#![feature(const_fn)]
#![feature(libc)]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate js;
extern crate libc;

use std::os::raw::c_void;
use js::jsapi::CallArgs;
use js::jsapi::CompartmentOptions;
use js::jsapi::JSAutoCompartment;
use js::jsapi::JSContext;
use js::jsapi::JS_DefineFunction;
use js::jsapi::JS_EncodeStringToUTF8;
use js::jsapi::JS_GetRuntimePrivate;
use js::jsapi::JS_Init;
use js::jsapi::JS_InitStandardClasses;
use js::jsapi::JS_NewGlobalObject;
use js::jsapi::JS_ReportError;
// use js::jsapi::JS_SetGCZeal; // seems to be missing
use js::jsapi::JS_SetRuntimePrivate;
use js::jsapi::OnNewGlobalHookOption;
use js::jsapi::Value;
use js::jsval::UndefinedValue;
use js::rust::{Runtime, SIMPLE_GLOBAL_CLASS};

use std::ptr;
use std::env;
use std::fs::File;
// use std::io;
use std::ffi::CStr;
use std::str;
use std::io::Read;

fn main() {
	let filename = env::args().nth(1)
		.expect("Expected a filename as the first argument");

	let mut file = File::open(&filename).expect("File is missing");
	let mut contents = String::new();
	file.read_to_string(&mut contents).expect("Cannot read file");

	unsafe {

		JS_Init();

		println!("JS_Init()");


		let rt = Runtime::new();
		// JS_SetGCZeal(rt.rt(), 2, 1);

		let cx = rt.cx();

		rooted!(in(cx) let global_root =
			JS_NewGlobalObject(cx, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(),
							   OnNewGlobalHookOption::FireOnNewGlobalHook,
							   &CompartmentOptions::default())
		);
		let global = global_root.handle();
		println!("JS_NewGlobalObject()");

		// let res = JS_InitStandardClasses(cx, global);
		// println!("JS_InitStandardClasses()");

		let _ac = JSAutoCompartment::new(cx, global.get());
		let _ = JS_DefineFunction(cx, global, b"puts\0".as_ptr() as *const libc::c_char,
										 Some(puts), 1, 0);

		let rcx = Box::into_raw(Box::new(RJSContext {}));
		JS_SetRuntimePrivate(rt.rt(), rcx as *mut c_void);


		rooted!(in(cx) let mut rval = UndefinedValue());
		assert!(rt.evaluate_script(global, &contents,
								   &filename, 1, rval.handle_mut()).is_ok());

		let js = js::rust::ToString(cx, rval.handle());
		rooted!(in(cx) let message_root = js);
		let message = JS_EncodeStringToUTF8(cx, message_root.handle());
		let message = CStr::from_ptr(message);
		println!("script result: {}", str::from_utf8(message.to_bytes()).unwrap());

	}

}

unsafe extern "C" fn puts(context: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
	let args = CallArgs::from_vp(vp, argc);

	if args._base.argc_ != 1 {
		JS_ReportError(context, b"puts() requires exactly 1 argument\0".as_ptr() as *const libc::c_char);
		return false;
	}

	let arg = args.get(0);
	let js = js::rust::ToString(context, arg);
	rooted!(in(context) let message_root = js);
	let message = JS_EncodeStringToUTF8(context, message_root.handle());
	let message = CStr::from_ptr(message);
	println!("{}", str::from_utf8(message.to_bytes()).unwrap());

	args.rval().set(UndefinedValue());
	return true;
}

#[derive(Debug)]
struct RJSContext {
	
}
