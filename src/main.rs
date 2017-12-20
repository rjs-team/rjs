
#![feature(fnbox)]
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
use rjs::jslib::jsfn::RuntimePrivate;
use rjs::jslib::context::{RJSContext, RJSHandle, RJSRemote};
use tokio_core::reactor::Timeout;
use tokio_core::reactor::Core;
use futures::future::Future;
use futures::sync::mpsc;

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
use mozjs::jsapi::{JS_GetContext, JS_SetPrivate, JS_GetPrivate, JS_GetInstancePrivate, JS_InitStandardClasses};
use futures::Stream;
use tokio_core::reactor::Interval;
use mozjs::conversions::ConversionBehavior;
use mozjs::jsapi::Handle;
use mozjs::jsapi::{JS_NewPlainObject, JS_SetProperty};
use mozjs::jsval::{StringValue, NullValue};
use mozjs::jsapi::JS_SetGCZeal;


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
use std::boxed::FnBox;





fn main() {
    let filename = env::args().nth(1)
        .expect("Expected a filename as the first argument");

    let mut file = File::open(&filename).expect("File is missing");
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect("Cannot read file");



    let rt = Runtime::new().unwrap();
    unsafe { JS_SetGCZeal(rt.rt(), 2, 1) };

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

    eventloop::run(&rt, rcx, |handle| {

        let privatebox : Box<RuntimePrivate> = Box::new(handle.downgrade());
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

    let privatebox: Box<RuntimePrivate> = unsafe { Box::from_raw(JS_GetRuntimePrivate(rt.rt()) as *mut _ )};
    drop(privatebox);
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


js_fn!{fn setTimeout(rcx: &RJSContext, handle: &RJSHandle, callback: JSVal, timeout: u64 {ConversionBehavior::Default}) -> JSRet<()> {
    rooted!(in(rcx.cx) let callback = callback);
    let handle2: RJSHandle = handle.clone();
    //let remote = handle.remote().clone();

    let timeout = Timeout::new(Duration::from_millis(timeout), handle.core_handle()).unwrap();

    let callback_ref = handle.store_new(callback.get());

    handle.core_handle().spawn(
        timeout.map_err(|_|()).and_then(move|_| {
            //remote.spawn(move|rcx, handle| {
                let rcx = handle2.get();
                let _ac = JSAutoCompartment::new(rcx.cx, rcx.global.get());

                rooted!(in(rcx.cx) let this_val = rcx.global.get());
                rooted!(in(rcx.cx) let mut rval = UndefinedValue());

                rooted!(in(rcx.cx) let callback = handle2.retrieve(&callback_ref).unwrap());

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
            //});
            
            drop(callback);
            drop(rval);
            drop(this_val);
            drop(_ac);
            drop(rcx);


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
    if !jsapi::JS_GetPendingException(cx, ex.handle_mut()) {
        return;
    }
    jsapi::JS_ClearPendingException(cx);

    let ex = String::from_jsval(cx, ex.handle(), ()).to_result().unwrap();
    println!("Exception!: {}", ex);
}




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

impl Window {
    fn do_on_thread<F>(&self, f: F)
        where F: for<'r> FnBox(&'r glutin::GlWindow) + Send + 'static
    {
        drop(self.send_msg.unbounded_send(WindowMsg::Do(Box::new(f))));
    }

}

macro_rules! window_get_private {
    ($this:ident) => {
        unsafe {
            let win = JS_GetPrivate($this.to_object()) as *mut Window;
            &*win
        }
    }
}

js_class!{ Window
    [JSCLASS_HAS_PRIVATE]
    private: Window,

    @constructor
    fn Window_constr(rcx: &RJSContext, handle: &RJSHandle, args: CallArgs) -> JSRet<*mut JSObject> {
        let rcx = handle.get();
        let jswin = unsafe { JS_NewObjectForConstructor(rcx.cx, Window::class(), &args) };

        let handle: RJSHandle = handle.clone();
        let remote = handle.remote();

        let (send_events, recv_events) = mpsc::unbounded();
        let (send_msg, recv_msg) = mpsc::unbounded();

        let thread = thread::spawn(move || {
            window_thread(recv_msg, send_events);
        });

        {
            let handle2 = handle.clone();
            let jswin = handle2.store_new(jswin);
            handle.core_handle().spawn(
                recv_events.for_each(move |event| -> Result<(), ()> {
                    let rcx = handle2.get();
                    let jswin = handle2.retrieve_copy(&jswin).unwrap();
                    let _ac = JSAutoCompartment::new(rcx.cx, jswin);
                    rooted!(in(rcx.cx) let obj = unsafe { JS_NewPlainObject(rcx.cx) });
                    match event {
                        WindowEvent::Glutin(ge) => match ge {
                            glutin::Event::DeviceEvent {..} => {
                                rooted!(in(rcx.cx) let mut val = NullValue());
                                unsafe {
                                    "device".to_jsval(rcx.cx, val.handle_mut());
                                    JS_SetProperty(rcx.cx, obj.handle(), c_str!("kind"), val.handle());
                                }
                            },
                            glutin::Event::WindowEvent {..} => (),
                            glutin::Event::Suspended {..} => (),
                            glutin::Event::Awakened => (),
                        },
                        WindowEvent::Closed => { println!("WindowEvent closed"); },
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
            JS_SetPrivate(jswin, Box::into_raw(window) as *mut _);
        }
        println!("window constructed");

        Ok(jswin)
    }

    fn ping(this: @this, rcx: &RJSContext, args: CallArgs) -> JSRet<()> {
        let mut args = args;
        rooted!(in(rcx.cx) let this = this.to_object());
        let win = unsafe { &*Window::get_private(rcx.cx, this.handle(), &mut args).unwrap() };

        println!("ping");

        if let Err(e) = win.send_msg.unbounded_send(WindowMsg::Ping) {
            println!("ping failed: {}", e);
        }

        Ok(())
    }

    fn close(this: @this) -> JSRet<()> {
        let win = window_get_private!(this);

        if let Err(e) = win.send_msg.unbounded_send(WindowMsg::Close) {
            println!("close failed: {}", e);
        }

        Ok(())
    }

    fn clearColor(this: @this, r: f32, g: f32, b: f32, a: f32) -> JSRet<()> {
        let win = window_get_private!(this);

        win.do_on_thread(
            move |glwin: &glutin::GlWindow| {
                unsafe { gl::ClearColor(r, g, b, a) };
            });

        Ok(())
    }

    fn clear(this: @this, mask: u32 {ConversionBehavior::Default}) -> JSRet<()> {
        let win = window_get_private!(this);

        win.do_on_thread(
            move |glwin: &glutin::GlWindow| {
                unsafe { gl::Clear(mask) };
            });

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
        drop(win.send_msg.unbounded_send(WindowMsg::Close));
        win.thread.join();
        println!("window dropped");
    }
}

enum WindowMsg {
    Do(Box<FnBox(&glutin::GlWindow) + Send>),
    Ping,
    Close,
}


#[derive(Debug)]
enum WindowEvent {
    Glutin(glutin::Event),
    Closed,
}

use std::cell::RefCell;
use std::rc::Rc;


fn window_thread(recv_msg: mpsc::UnboundedReceiver<WindowMsg>, send_events: mpsc::UnboundedSender<WindowEvent>) {
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

    let (stop_send, stop_recv) = futures::sync::oneshot::channel();
    let stop_recv = stop_recv.map_err(|err| ());

    struct WindowStuff {
        gl_window: glutin::GlWindow,
        stop: Option<futures::sync::oneshot::Sender<()>>,
    }

    impl WindowStuff {
        fn stop(&mut self) {
            let stop = self.stop.take();
            stop.map(|stop| stop.send(()));
        }
    }

    let stuff = Rc::new(RefCell::new(
        WindowStuff {
            gl_window: gl_window,
            stop: Some(stop_send),
        }));


    let recv_msgs = {
        let stuff = stuff.clone();
        recv_msg.for_each(move |msg| -> Result<(), ()> {
            let mut stuff = stuff.borrow_mut();
            let stuff = &mut *stuff;
            match msg {
                WindowMsg::Do(func) => { println!("message Do");  func.call_box((&stuff.gl_window,)); },
                WindowMsg::Ping => { println!("pong"); },
                WindowMsg::Close => {
                    println!("close");
                    stuff.stop();
                    stuff.gl_window.hide();
                },
            }

            Ok(())
        }).then(|_| -> Result<(), ()> { Ok(()) })
    };

    let handle_window_events = Interval::new(Duration::from_millis(16), &core.handle()).unwrap()
        .map_err(|err| { println!("Interval err: {}", err); () })
        .for_each(move |()| -> Result<(), ()> {
            //println!("checking for events...");
            let mut stuff = stuff.borrow_mut();
            let stuff = &mut *stuff;

            unsafe {
                gl::Clear(gl::COLOR_BUFFER_BIT);
            }
            stuff.gl_window.swap_buffers().unwrap();

            events_loop.poll_events(|event| {
                //println!("glutin event: {:?}", event);
                match &event {
                    &glutin::Event::WindowEvent { ref event, .. } => match event {
                        &glutin::WindowEvent::Closed => {
                            stuff.stop();
                            stuff.gl_window.hide();
                            drop(send_events.unbounded_send(WindowEvent::Closed));
                        },
                        &glutin::WindowEvent::Resized(w, h) => stuff.gl_window.resize(w, h),
                        _ => ()
                    },
                    &glutin::Event::Awakened => { return; },
                    _ => ()
                };
                drop(send_events.unbounded_send(WindowEvent::Glutin(event)));
            });

            Ok(())
        }).then(|_| -> Result<(), ()> { Ok(()) });

    let streams = handle_window_events.select(recv_msgs).then(|_| -> Result<(),()> { Ok(()) });
    core.run(stop_recv.select(streams));

    println!("window_thread exiting");
}
