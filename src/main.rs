
#![feature(const_fn)]
#![feature(libc)]
#![feature(trace_macros)]
#![recursion_limit="10000"]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate mozjs;
extern crate libc;
#[macro_use]
extern crate rjs;
extern crate tokio_core;
extern crate futures;
extern crate glutin;
extern crate gl;

use rjs::jslib::eventloop;
use tokio_core::reactor::Timeout;
use tokio_core::reactor::Core;
use futures::future::Future;

use std::os::raw::c_void;
use mozjs::jsapi;
use jsapi::CallArgs;
use jsapi::CompartmentOptions;
use jsapi::JSAutoCompartment;
use jsapi::JSContext;
use jsapi::JSObject;
use jsapi::JS_NewObjectForConstructor;
//use jsapi::JSFunction;
use jsapi::JS_CallFunctionValue;
//use jsapi::JS_DefineFunction;
//use jsapi::JS_EncodeStringToUTF8;
//use jsapi::JS_free;
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
use rjs::jslib::jsclass::JSCLASS_HAS_PRIVATE;
use mozjs::jsval;
use jsval::JSVal;
use jsval::{ObjectValue, UndefinedValue};
use jsapi::{HandleObject};
use mozjs::jsapi::{ JSPROP_ENUMERATE, JSPROP_SHARED };
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use mozjs::conversions::{FromJSValConvertible, ToJSValConvertible};
use mozjs::conversions::ConversionResult;
//use rjs::jslib::jsclass;
use rjs::jslib::jsfn::{JSRet, RJSFn};
use rjs::jslib::jsclass::{JSClassInitializer, null_function, null_property, null_wrapper, jsclass_has_reserved_slots};
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSFunctionSpec;
use mozjs::jsapi::JSNativeWrapper;
use mozjs::jsapi::JSPropertySpec;
use mozjs::jsapi::{JS_GetContext, JS_SetPrivate, JS_GetPrivate, JS_InitStandardClasses};
use futures::sync::mpsc::{unbounded, UnboundedSender, UnboundedReceiver};
use futures::Stream;

use glutin::GlContext;

use std::ptr;
use std::env;
use std::fs;
use std::fs::File;
use std::path::Path;
// use std::io;
//use std::ffi::CStr;
use std::str;
use std::io::Read;
use std::time::{Duration};
use std::ffi::CString;
//use std::marker::PhantomData;
//use std::fmt;
//use std::fmt::Display;
use std::sync::{Once, ONCE_INIT};
use std::thread;





fn main() {
    let filename = env::args().nth(1)
        .expect("Expected a filename as the first argument");

    let mut file = File::open(&filename).expect("File is missing");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Cannot read file");

    unsafe { JS_Init(); }


    let rt = Runtime::new().unwrap();
    // JS_SetGCZeal(rt.rt(), 2, 1);

    let cx = rt.cx();

    rooted!(in(cx) let global_root =
        unsafe { JS_NewGlobalObject(cx, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(),
                           OnNewGlobalHookOption::FireOnNewGlobalHook,
                           &CompartmentOptions::default()) }
    );
    let global = global_root.handle();
    let rcx = RJSContext {
        cx: cx,
        global: global,
    };

    eventloop::run(&rt, &rcx, |handle| {

        let privatebox : Box<(&RJSContext, eventloop::WeakHandle<RJSContext>)> = Box::new((&rcx, handle.downgrade()));
        unsafe { JS_SetRuntimePrivate(rt.rt(), Box::into_raw(privatebox) as *mut c_void) };

        let _ac = JSAutoCompartment::new(cx, global.get());

        let _ = unsafe { JS_InitStandardClasses(cx, global) };
        // println!("JS_InitStandardClasses()");

        unsafe {
            let _ = puts{}.define_on(cx, global, 0);
            let _ = setTimeout{}.define_on(cx, global, 0);
            let _ = getFileSync{}.define_on(cx, global, 0);
            let _ = readDir{}.define_on(cx, global, 0);

            Test::init_class(cx, global);
            Window::init_class(cx, global);
        }


        rooted!(in(cx) let mut rval = UndefinedValue());
        let res = rt.evaluate_script(global, &contents,
                                   &filename, 1, rval.handle_mut());
        if !res.is_ok() {
            unsafe { report_pending_exception(cx); }
        }

        let str = unsafe { String::from_jsval(cx, rval.handle(), ()) }.to_result().unwrap();

        println!("script result: {}", str);

    });
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


js_fn!{fn puts(arg: String) -> JSRet<()> {
    println!("puts: {}", arg);
    Ok(())
}}


js_fn!{fn setTimeout(rcx: &RJSContext, handle: &RJSHandle, callback: JSVal, timeout: u64 {mozjs::conversions::ConversionBehavior::Default}) -> JSRet<()> {
    rooted!(in(rcx.cx) let callback = callback);
    let remote = handle.remote().clone();

    let timeout = Timeout::new(Duration::from_millis(timeout), handle.core_handle()).unwrap();

    let callback_ref = handle.store_new(callback.get());

    handle.core_handle().spawn(
        timeout.map_err(|_|()).and_then(move|_| {
            remote.spawn(move|rcx, handle| {
                let _ac = JSAutoCompartment::new(rcx.cx, rcx.global.get());

                rooted!(in(rcx.cx) let this_val = rcx.global.get());
                rooted!(in(rcx.cx) let mut rval = UndefinedValue());

                rooted!(in(rcx.cx) let callback = handle.retrieve(&callback_ref).unwrap());

                //println!("setTimeout callback");

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
                //println!("setTimeout callback done");
            });


            Ok(())
        })
    );

    Ok(())
}}

js_fn!{fn getFileSync(path: String) -> JSRet<Option<String>> {
    if let Ok(mut file) = File::open(path) {
        let mut contents = String::new();
        match file.read_to_string(&mut contents) {
            Ok(_) => Ok(Some(contents)),
            Err(e) => Err(Some(format!("Error reading contents: {}", e))),
        }
    } else {
        Ok(None)
    }
    // args.rval().set();
    //true
}}

js_fn!{fn readDir(rcx: &RJSContext, path: String) -> JSRet<JSVal> {
    unsafe {
        rooted!(in(rcx.cx) let arr = JS_NewArrayObject1(rcx.cx, 0));
        rooted!(in(rcx.cx) let mut temp = UndefinedValue());

        for (i, entry) in fs::read_dir(Path::new(&path)).unwrap().enumerate() {
            let entry = entry.unwrap();
            let path = entry.path();

            path.to_str().unwrap().to_jsval(rcx.cx, temp.handle_mut());
            JS_SetElement(rcx.cx, arr.handle(), i as u32, temp.handle());
        }

        Ok(ObjectValue(*arr))
    }
}}

unsafe fn report_pending_exception(cx: *mut JSContext) {
    rooted!(in(cx) let mut ex = UndefinedValue());
    if !jsapi::JS_GetPendingException(cx, ex.handle_mut())
        { return; }

    let ex = String::from_jsval(cx, ex.handle(), ()).to_result().unwrap();
    println!("Exception!: {}", ex);
}

#[derive(Debug)]
pub struct RJSContext {
    cx: *mut JSContext,
    global: HandleObject,
}

pub type RJSHandle = eventloop::Handle<RJSContext>;
pub type RJSRemote = eventloop::Remote<RJSContext>;



struct Test {

}

js_class!{ Test
    [JSCLASS_HAS_PRIVATE]

    @constructor
    fn Test_constructor(rcx: &RJSContext, args: CallArgs) -> JSRet<*mut JSObject> {
        let obj = unsafe { JS_NewObjectForConstructor(rcx.cx, Test::class(), &args) };

        Ok(obj)
    }

    fn test_puts(arg: String) -> JSRet<()> {
        println!("{}", arg);
        Ok(())
    }

    @prop test_prop {
        get fn Test_get_test_prop() -> JSRet<String> {
            Ok(String::from("Test prop"))
        }
    }

}

struct Window {
    thread: thread::JoinHandle<()>,
    send_msg: UnboundedSender<WindowMsg>,
}

js_class!{ Window
    [JSCLASS_HAS_PRIVATE]

    @constructor
    fn Window_constr(rcx: &RJSContext, handle: &RJSHandle, args: CallArgs) -> JSRet<*mut JSObject> {
        let obj = unsafe { JS_NewObjectForConstructor(rcx.cx, Window::class(), &args) };

        let handle = handle.clone();
        let remote = handle.remote();

        let (send_events, recv_events) = unbounded();
        let (send_msg, recv_msg) = unbounded();

        let thread = thread::spawn(move || {
            window_thread(recv_msg, send_events);
        });

        {
            let handle2 = handle.clone();
            let handle = Rc::new(RefCell::new(handle.clone()));
            //let obj = handle2.store_new(obj);
            handle2.core_handle().spawn(
                recv_events.for_each(move |event| -> Result<(), ()> {
                    println!("Window event: {:?}", &event);
                    let h: &RJSHandle = &handle.borrow();
                    //let obj = handle.retrieve_copy(&obj);
                    match event {
                        WindowEvent::Closed => (),
                    }

                    Ok(())
                })
            );
        }


        let window = Box::new(Window {
            thread: thread,
            send_msg: send_msg,
        });
        unsafe {
            JS_SetPrivate(obj, Box::into_raw(window) as *mut _);
        }
        println!("window constructed");

        Ok(obj)
    }

    fn ping(this: @this) -> JSRet<()> {
        unsafe {
            let win = JS_GetPrivate(this.to_object()) as *mut Window;

            (*win).send_msg.unbounded_send(
                WindowMsg::Ping
            );
        }

        Ok(())
    }

    fn close(this: @this) -> JSRet<()> {
        unsafe {
            let win = JS_GetPrivate(this.to_object()) as *mut Window;

            (*win).send_msg.unbounded_send(
                WindowMsg::Close
            );
        }

        Ok(())
    }

    @op(finalize)
    fn Window_finalize(free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        let rt = (*free).runtime_;
        let cx = JS_GetContext(rt);

        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, 0 as *mut _);
        let win = Box::from_raw(private as *mut Window);
        win.send_msg.unbounded_send(WindowMsg::Close);
        win.thread.join();
        //drop(win);
        println!("window dropped");
    }
}

#[derive(Debug)]
enum WindowMsg {
    Ping,
    Close,
}

#[derive(Debug)]
enum WindowEvent {
    Closed,
}

use std::cell::RefCell;
use std::rc::Rc;

fn window_thread(recv_msg: UnboundedReceiver<WindowMsg>, send_events: UnboundedSender<WindowEvent>) {
    let mut core = Core::new().unwrap();

    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new()
        .with_title("RJS Window")
        .with_dimensions(1024, 768);
    let context = glutin::ContextBuilder::new()
        .with_vsync(true);
    let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();

    unsafe {
        gl_window.make_current().unwrap();
        gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);
    }

    struct WindowStuff {
        gl_window: glutin::GlWindow,
        run: bool,
    }

    // This could just be a let mut if tokio used lifetimes
    let stuff = Rc::new(RefCell::new(WindowStuff {
        gl_window: gl_window,
        run: true,
    }));

    {
        let stuff = stuff.clone(); // grrr. Why no clone closures?
        core.handle().spawn(
            recv_msg.for_each(move |msg| -> Result<(), ()> {
                println!("Window msg: {:?}", &msg);
                let mut stuff = stuff.borrow_mut();
                let stuff = &mut *stuff;
                match msg {
                    WindowMsg::Ping => { println!("ping"); },
                    WindowMsg::Close => {
                        stuff.run = false;
                        stuff.gl_window.hide();
                    },
                }

                Ok(())
            })
        );
    }

    while (*stuff.borrow()).run {
        core.turn(Some(Duration::from_millis(16)));

        let mut stuff = stuff.borrow_mut();
        let stuff = &mut *stuff;

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        stuff.gl_window.swap_buffers().unwrap();

        events_loop.poll_events(|event| {
            println!("glutin event: {:?}", event);
            match event {
                glutin::Event::WindowEvent { event, .. } => match event {
                    glutin::WindowEvent::Closed => {
                        stuff.run = false;
                        stuff.gl_window.hide();
                        send_events.unbounded_send(WindowEvent::Closed);
                    },
                    glutin::WindowEvent::Resized(w, h) => stuff.gl_window.resize(w, h),
                    _ => ()
                },
                _ => ()
            }
        })
    }

    println!("window_thread exiting");

}
