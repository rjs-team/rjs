#![feature(fnbox)]
#![feature(const_fn)]
#![feature(libc)]
#![recursion_limit = "100"]

extern crate futures;
extern crate gl;
extern crate glutin;
extern crate libc;
#[macro_use]
extern crate mozjs;
#[macro_use]
extern crate rjs;
extern crate tokio_core;

use futures::Stream;
use futures::future::{loop_fn, Future, Loop};
use futures::sync::mpsc;
use glutin::GlContext;
use jsapi::{CallArgs, CompartmentOptions, HandleValue, HandleValueArray, JSAutoCompartment,
            JSContext, JSObject, JS_CallFunctionValue, JS_GetRuntime, JS_GetRuntimePrivate,
            JS_NewArrayObject1, JS_NewGlobalObject, JS_NewObjectForConstructor, JS_ReportError,
            JS_SetElement, JS_SetRuntimePrivate, OnNewGlobalHookOption, Value};
use jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::conversions::{ConversionBehavior, ConversionResult, FromJSValConvertible,
                         ToJSValConvertible};
use mozjs::jsapi::{Handle, HandleObject, JSClass, JSClassOps, JSFunctionSpec, JSNativeWrapper,
                   JSPropertySpec, JS_GetInstancePrivate, JS_GetPrivate, JS_GetProperty,
                   JS_InitStandardClasses, JS_NewPlainObject, JS_SetPrivate, JS_SetProperty,
                   JSPROP_ENUMERATE, JSPROP_SHARED};
use mozjs::jsapi;
use mozjs::jsval::NullValue;
use mozjs::jsval;
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use rjs::jslib::context::{RJSContext, RJSHandle};
use rjs::jslib::eventloop;
use rjs::jslib::jsclass::{null_function, null_property, null_wrapper, JSClassInitializer,
                          JSCLASS_HAS_PRIVATE};
use rjs::jslib::jsfn::{JSRet, RJSFn, RuntimePrivate};
use std::boxed::FnBox;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::fs;
use std::io::Read;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr;
use std::str;
use std::sync::{Once, ONCE_INIT};
use std::thread;
use std::time::{Duration, Instant};
use tokio_core::reactor::{Core, Timeout};

fn main() {
    let filename = env::args()
        .nth(1)
        .expect("Expected a filename as the first argument");

    let mut file = File::open(&filename).expect("File is missing");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Cannot read file");

    let rt = Runtime::new().unwrap();
    #[cfg(debugmozjs)]
    unsafe { jsapi::JS_SetGCZeal(rt.rt(), 2, 1) };

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
        let privatebox: Box<RuntimePrivate> = Box::new(handle.downgrade());
        unsafe { JS_SetRuntimePrivate(rt.rt(), Box::into_raw(privatebox) as *mut c_void) };

        let _ac = JSAutoCompartment::new(cx, global.get());

        let _ = unsafe { JS_InitStandardClasses(cx, global) };
        // println!("JS_InitStandardClasses()");

        unsafe {
            let _ = puts.define_on(cx, global, 0);
            let _ = setTimeout.define_on(cx, global, 0);
            let _ = getFileSync.define_on(cx, global, 0);
            let _ = readDir.define_on(cx, global, 0);

            Test::init_class(cx, global, HandleObject::null());
            Window::init_class(cx, global, HandleObject::null());
        }

        rooted!(in(cx) let mut rval = UndefinedValue());
        let res = rt.evaluate_script(global, &contents, &filename, 1, rval.handle_mut());
        if !res.is_ok() {
            unsafe {
                report_pending_exception(cx);
            }
        }

        let str = unsafe { String::from_jsval(cx, rval.handle(), ()) }
            .to_result()
            .unwrap();

        println!("script result: {}", str);
    });

    let _: Box<RuntimePrivate> = unsafe { Box::from_raw(JS_GetRuntimePrivate(rt.rt()) as *mut _) };
}

trait ToResult<T> {
    fn to_result(self) -> Result<T, Option<String>>;
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

js_fn!{fn setTimeout(rcx: &RJSContext,
                     handle: &RJSHandle,
                     callback: JSVal,
                     timeout: u64 {ConversionBehavior::Default}
                     ) -> JSRet<()> {
    rooted!(in(rcx.cx) let callback = callback);
    let handle2: RJSHandle = handle.clone();
    //let remote = handle.remote().clone();

    let timeout = Timeout::new(Duration::from_millis(timeout), handle.core_handle()).unwrap();

    let callback_ref = handle.store_new(callback.get());

    handle.core_handle().spawn(
        timeout.map_err(|_|()).and_then(move|_| {
            let rcx = handle2.get();
            let _ac = JSAutoCompartment::new(rcx.cx, rcx.global.get());

            rooted!(in(rcx.cx) let this_val = rcx.global.get());
            rooted!(in(rcx.cx) let mut rval = UndefinedValue());

            rooted!(in(rcx.cx) let callback = handle2.retrieve(&callback_ref).unwrap());

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

    let exhandle = Handle::from_marked_location(&ex.get().to_object());

    let report = jsapi::JS_ErrorFromException(cx, exhandle);
    if report.is_null() {
        return;
    }
    let report = &*report;

    let filename = {
        let filename = report.filename as *const u8;
        if filename.is_null() {
            "<no filename>".to_string()
        } else {
            let length = (0..).find(|i| *filename.offset(*i) == 0).unwrap();
            let filename = ::std::slice::from_raw_parts(filename, length as usize);
            String::from_utf8_lossy(filename).into_owned()
        }
    };

    let message = {
        let message = report.ucmessage;
        let length = (0..).find(|i| *message.offset(*i) == 0).unwrap();
        let message = ::std::slice::from_raw_parts(message, length as usize);
        String::from_utf16_lossy(message)
    };

    /*let line = {
        let line = report.linebuf_;
        let length = report.linebufLength_;
        let line = ::std::slice::from_raw_parts(line, length as usize);
        String::from_utf16_lossy(line)
    };*/

    //let ex = String::from_jsval(cx, ex.handle(), ()).to_result().unwrap();
    println!(
        "Exception at {}:{}:{}: {}",
        filename, report.lineno, report.column, message
    );
    //println!("{:?}", report);

    rooted!(in(cx) let stack = jsapi::ExceptionStackOrNull(exhandle));
    if stack.is_null() {
        return;
    }

    rooted!(in(cx) let mut stackstr = jsapi::JS_GetEmptyStringValue(cx).to_string());

    let success = jsapi::BuildStackString(cx, stack.handle(), stackstr.handle_mut(), 2);
    if !success {
        return;
    }
    rooted!(in(cx) let stackstr = jsval::StringValue(&mut *stackstr.get()));
    let stackstr = String::from_jsval(cx, stackstr.handle(), ())
        .to_result()
        .unwrap();
    println!("{}", stackstr);
}

struct Test {}

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
    send_msg: mpsc::UnboundedSender<WindowMsg>,
}

impl Window {
    fn do_on_thread<F>(&self, f: F)
    where
        F: for<'r> FnBox(&'r glutin::GlWindow) + Send + 'static,
    {
        let _ = self.send_msg.unbounded_send(WindowMsg::Do(Box::new(f)));
    }
}

js_class!{ Window
    [JSCLASS_HAS_PRIVATE]
    private: Window,

    @constructor
    fn Window_constr(handle: &RJSHandle, args: CallArgs) -> JSRet<*mut JSObject> {
        let rcx = handle.get();
        let jswin = unsafe { JS_NewObjectForConstructor(rcx.cx, Window::class(), &args) };

        let handle: RJSHandle = handle.clone();

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
                    rooted!(in(rcx.cx) let jswin = handle2.retrieve_copy(&jswin).unwrap());
                    let _ac = JSAutoCompartment::new(rcx.cx, jswin.get());
                    rooted!(in(rcx.cx) let obj = unsafe { JS_NewPlainObject(rcx.cx) });

                    //println!("event received: {:?}", &event);
                    match event {
                        WindowEvent::Glutin(ge) => if let glutin::Event::DeviceEvent {..} = ge {
                            rooted!(in(rcx.cx) let mut val = NullValue());
                            unsafe {
                                "device".to_jsval(rcx.cx, val.handle_mut());
                                JS_SetProperty(rcx.cx, obj.handle(), c_str!("kind"), val.handle());
                            }
                        },
                        WindowEvent::Closed => { println!("WindowEvent closed"); },
                    }

                    rooted!(in(rcx.cx) let mut onevent = NullValue());
                    let success = unsafe {
                        JS_GetProperty(rcx.cx,
                                       jswin.handle(),
                                       c_str!("onevent"),
                                       onevent.handle_mut())
                    };
                    if !success || onevent.is_null_or_undefined() {
                        println!("success: {:?} onevent: {:?}", success, onevent.is_null());
                        return Ok(());
                    }

                    rooted!(in(rcx.cx) let mut ret = NullValue());
                    let args = &[ObjectValue(obj.get())];
                    let args = unsafe { HandleValueArray::from_rooted_slice(args) };
                    if ! unsafe {
                        jsapi::Call(rcx.cx,
                                    HandleValue::null(),
                                    onevent.handle(),
                                    &args,
                                    ret.handle_mut())
                        } {
                        // ignore?
                        unsafe { report_pending_exception(rcx.cx); }
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

    fn getContext(this: @this, str: String) -> JSRet<*mut JSObject> {
        println!("getContext: {}", str);
        Ok(this.to_object())
    }

    fn ping(this: @this, rcx: &RJSContext, args: CallArgs) -> JSRet<()> {
        let win = Window::get_private(rcx.cx, this, Some(args)).unwrap();

        println!("ping");

        if let Err(e) = win.send_msg.unbounded_send(WindowMsg::Ping) {
            println!("ping failed: {}", e);
        }

        Ok(())
    }

    fn close(this: @this, rcx: &RJSContext, args: CallArgs) -> JSRet<()> {
        let win = Window::get_private(rcx.cx, this, Some(args)).unwrap();

        if let Err(e) = win.send_msg.unbounded_send(WindowMsg::Close) {
            println!("close failed: {}", e);
        }

        Ok(())
    }

    fn clearColor(this: @this, rcx: &RJSContext, args: CallArgs, r: f32, g: f32, b: f32, a: f32)
        -> JSRet<()> {
        let win = Window::get_private(rcx.cx, this, Some(args)).unwrap();

        win.do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::ClearColor(r, g, b, a) };
            });

        Ok(())
    }

    fn clear(this: @this, rcx: &RJSContext, args: CallArgs, mask: u32 {ConversionBehavior::Default})
        -> JSRet<()> {
        let win = Window::get_private(rcx.cx, this, Some(args)).unwrap();

        win.do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::Clear(mask) };
            });

        Ok(())
    }

    @op(finalize)
    fn Window_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        //let rt = (*free).runtime_;
        //let cx = JS_GetContext(rt);

        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, ptr::null_mut() as *mut _);
        let win = Box::from_raw(private as *mut Window);
        let _ = win.send_msg.unbounded_send(WindowMsg::Close);
        win.thread.join().unwrap();
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

fn window_thread(
    recv_msg: mpsc::UnboundedReceiver<WindowMsg>,
    send_events: mpsc::UnboundedSender<WindowEvent>,
) {
    let mut core = Core::new().unwrap();

    let mut events_loop = glutin::EventsLoop::new();
    let window = glutin::WindowBuilder::new()
        .with_title("RJS Window")
        .with_dimensions(1024, 768);
    let context = glutin::ContextBuilder::new().with_vsync(true);
    let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();

    unsafe {
        gl_window.make_current().unwrap();
        gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);
    }

    let (stop_send, stop_recv) = futures::sync::oneshot::channel();
    let stop_recv = stop_recv.map_err(|_| ());

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

    let stuff = Rc::new(RefCell::new(WindowStuff {
        gl_window: gl_window,
        stop: Some(stop_send),
    }));

    let recv_msgs = {
        let stuff = Rc::clone(&stuff);
        recv_msg
            .for_each(move |msg| -> Result<(), ()> {
                let mut stuff = stuff.borrow_mut();
                match msg {
                    WindowMsg::Do(func) => {
                        println!("message Do");
                        func.call_box((&stuff.gl_window,));
                    }
                    WindowMsg::Ping => {
                        println!("pong");
                    }
                    WindowMsg::Close => {
                        println!("close");
                        stuff.stop();
                        stuff.gl_window.hide();
                    }
                }

                Ok(())
            })
            .then(|_| -> Result<(), ()> { Ok(()) })
    };

    // Interval doesn't work great here because it doesn't know how long things will take
    // and can get out of hand.

    let handle = &core.handle();

    let handle_window_events = loop_fn((), move |()| {
        let mut stuff = stuff.borrow_mut();

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        let now = Instant::now();
        stuff.gl_window.swap_buffers().unwrap();
        let swap_time = now.elapsed();
        let swap_ms = swap_time.subsec_nanos() as f32 / 1000000.0;
        if swap_ms > 1.0 {
            println!("swap took: {}ms", swap_ms);
        }

        events_loop.poll_events(|event| {
            match event {
                glutin::Event::WindowEvent { ref event, .. } => match *event {
                    glutin::WindowEvent::Closed => {
                        stuff.stop();
                        stuff.gl_window.hide();
                        let _ = send_events.unbounded_send(WindowEvent::Closed);
                    }
                    glutin::WindowEvent::Resized(w, h) => stuff.gl_window.resize(w, h),
                    _ => (),
                },
                glutin::Event::Awakened => {
                    return;
                }
                _ => (),
            };
            let _ = send_events.unbounded_send(WindowEvent::Glutin(event));
        });

        Timeout::new(Duration::from_millis(16), handle)
            .unwrap()
            .map(|_| Loop::Continue(()))
    }).map_err(|_| ());

    let streams = handle_window_events
        .select(recv_msgs)
        .then(|_| -> Result<(), ()> { Ok(()) });
    let _ = core.run(stop_recv.select(streams)).map_err(|_| "Oh crap!");
    println!("window_thread exiting");
}
