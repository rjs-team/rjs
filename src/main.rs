#![recursion_limit = "100"]

extern crate futures;
extern crate gl;
extern crate glutin;
extern crate libc;
#[macro_use]
extern crate mozjs;
extern crate tokio_core;

use futures::future::{loop_fn, Future, Loop};
use futures::sync::mpsc;
use futures::Stream;
use glutin::GlContext;
use mozjs::conversions::{
    ConversionBehavior, ConversionResult, FromJSValConvertible, ToJSValConvertible,
};
use mozjs::jsapi;
use mozjs::jsapi::StackFormat;
use mozjs::jsapi::{
    CallArgs, HandleValue, HandleValueArray, JSAutoRealm, JSContext, JSObject,
    JS_CallFunctionValue, JS_NewArrayObject1, JS_NewGlobalObject, JS_NewObjectForConstructor,
    JS_ReportErrorUTF8, JS_SetElement, OnNewGlobalHookOption, Value,
};
use mozjs::jsapi::{
    Handle, HandleObject, JSClass, JSClassOps, JSFunctionSpec, JSNativeWrapper, JSPropertySpec,
    JS_EnumerateStandardClasses, JS_GetPrivate, JS_GetProperty, JS_NewPlainObject, JS_SetPrivate,
    JS_SetProperty, JSPROP_ENUMERATE,
};
use mozjs::jsval;
use mozjs::jsval::NullValue;
use mozjs::jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::rust::{JSEngine, RealmOptions, Runtime, SIMPLE_GLOBAL_CLASS};
#[macro_use]
extern crate downcast;
extern crate lazy_static;
#[macro_use]
extern crate slab;

#[cfg(test)]
mod tests;

#[macro_use]
pub mod jslib;
use core::ptr;
use gl::types::*;
use jslib::context;
use jslib::context::{RJSContext, RJSHandle};
use jslib::eventloop;
use jslib::jsclass::{
    null_function, null_property, null_wrapper, JSClassInitializer, JSCLASS_HAS_PRIVATE,
};
use jslib::jsfn::{JSRet, RJSFn};

use std::env;
use std::ffi::CString;
use std::fmt::Debug;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant};
use tokio_core::reactor::{Core, Timeout};

macro_rules! setprop {
    (in($cx:expr, $tval:expr) ($obj:expr) . $prop:ident = $val:expr) => {
        unsafe {
            $val.to_jsval($cx, $tval.handle_mut());
            JS_SetProperty($cx, $obj, c_str!(stringify!($prop)), $tval.handle().into());
        }
    };
}

macro_rules! gl_set_props {
    ([$cx:expr, $obj:expr, $temp:expr] $($prop:ident)*) => {

        $(
            setprop!(in($cx, $temp) ($obj).$prop = gl::$prop);
        )*
    };
}

fn main() {
    let filename = env::args()
        .nth(1)
        .expect("Expected a filename as the first argument");

    let mut file = File::open(&filename).expect("File is missing");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .expect("Cannot read file");
    let engine = JSEngine::init().unwrap();
    let rt = Runtime::new(engine.handle());
    #[cfg(debugmozjs)]
    unsafe {
        jsapi::JS_SetGCZeal(rt.rt(), 2, 1)
    };

    let cx = rt.cx();

    rooted!(in(cx) let global_root =
        unsafe { JS_NewGlobalObject(cx, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(),
                           OnNewGlobalHookOption::FireOnNewGlobalHook,
                           &*RealmOptions::default()) }
    );
    let global = global_root.handle();
    let rcx = RJSContext::new(cx, global.into());

    eventloop::run(&rt, rcx, |handle| {
        let rcx = handle.get();
        let _ac = JSAutoRealm::new(cx, global.get());

        context::store_private(cx, &handle);

        let _ = unsafe { JS_EnumerateStandardClasses(cx, global.into()) };

        let wininfo;

        unsafe {
            let _ = puts.define_on(cx, global.into(), 0);
            let _ = setTimeout.define_on(cx, global.into(), 0);
            let _ = getFileSync.define_on(cx, global.into(), 0);
            let _ = readDir.define_on(cx, global.into(), 0);

            Test::init_class(rcx, global.into());
            wininfo = Window::init_class(rcx, global.into());
            WebGLShader::init_class(rcx, global.into());
            WebGLProgram::init_class(rcx, global.into());
            WebGLBuffer::init_class(rcx, global.into());
            WebGLUniformLocation::init_class(rcx, global.into());
        }

        rooted!(in(rcx.cx) let winproto = wininfo.prototype);
        let winproto = winproto.handle();
        rooted!(in(rcx.cx) let mut temp = NullValue());
        // TODO: Add all constants, organize them in a nice way
        gl_set_props!([rcx.cx, winproto.into(), temp]
            FRAGMENT_SHADER VERTEX_SHADER
            COMPILE_STATUS LINK_STATUS
            ARRAY_BUFFER
            STATIC_DRAW
            COLOR_BUFFER_BIT DEPTH_BUFFER_BIT
            POINTS TRIANGLES TRIANGLE_STRIP
            FLOAT
            DEPTH_TEST
        );

        rooted!(in(cx) let mut rval = UndefinedValue());
        let res = rt.evaluate_script(global.into(), &contents, &filename, 1, rval.handle_mut());
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

    let _ac = JSAutoRealm::new(cx, global.get());
    context::clear_private(cx);
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

js_fn! {fn puts(arg: String) -> JSRet<()> {
    println!("puts: {}", arg);
    Ok(())
}}

js_fn! {fn setTimeout(rcx: &RJSContext,
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
            let _ac = JSAutoRealm::new(rcx.cx, rcx.global.get());

            rooted!(in(rcx.cx) let this_val = rcx.global.get());
            rooted!(in(rcx.cx) let mut rval = UndefinedValue());

            rooted!(in(rcx.cx) let callback = handle2.retrieve(&callback_ref).unwrap());

            unsafe {
                let ok = JS_CallFunctionValue(
                    rcx.cx,
                    this_val.handle().into(),
                    callback.handle().into(),
                    &jsapi::HandleValueArray {
                        elements_: ptr::null_mut(),
                        length_: 0,
                    },
                    rval.handle_mut().into());

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

js_fn! {fn getFileSync(path: String) -> JSRet<Option<String>> {
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

js_fn! {fn readDir(rcx: &RJSContext, path: String) -> JSRet<JSVal> {
    unsafe {
        rooted!(in(rcx.cx) let arr = JS_NewArrayObject1(rcx.cx, 0));
        rooted!(in(rcx.cx) let mut temp = UndefinedValue());

        for (i, entry) in fs::read_dir(Path::new(&path)).unwrap().enumerate() {
            let entry = entry.unwrap();
            let path = entry.path();

            path.to_str().unwrap().to_jsval(rcx.cx, temp.handle_mut());
            JS_SetElement(rcx.cx, arr.handle().into(), i as u32, temp.handle().into());
        }

        Ok(ObjectValue(*arr))
    }
}}

unsafe fn report_pending_exception(cx: *mut JSContext) {
    rooted!(in(cx) let mut ex = UndefinedValue());
    if !jsapi::JS_GetPendingException(cx, ex.handle_mut().into()) {
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
        let filename = report._base.filename as *const u8;
        if filename.is_null() {
            "<no filename>".to_string()
        } else {
            let length = (0..).find(|i| *filename.offset(*i) == 0).unwrap();
            let filename = ::std::slice::from_raw_parts(filename, length as usize);
            String::from_utf8_lossy(filename).into_owned()
        }
    };

    let message = {
        let message = report._base.message_.data_ as *const u8;
        let length = (0..).find(|i| *message.offset(*i) == 0).unwrap();
        let message = ::std::slice::from_raw_parts(message, length as usize);
        String::from_utf8_lossy(message)
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
        filename, report._base.lineno, report._base.column, message
    );
    //println!("{:?}", report);
    capture_stack!(in(cx) let stack);
    let str_stack = stack
        .unwrap()
        .as_string(None, StackFormat::SpiderMonkey)
        .unwrap();
    println!("{}", str_stack);
}

struct Test {}

js_class! { Test extends ()
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
        F: for<'r> FnOnce(&'r glutin::GlWindow) + Send + 'static,
    {
        let _ = self.send_msg.unbounded_send(WindowMsg::Do(Box::new(f)));
    }

    fn sync_do_on_thread<F, R>(&self, f: F) -> R
    where
        F: for<'r> FnOnce(&'r glutin::GlWindow) -> R + Send,
        R: Debug + Send + 'static,
    {
        // using transmute to force adding 'static, since this function is
        // enforcing a shorter lifetime
        let fbox: Box<dyn for<'r> FnOnce(&'r glutin::GlWindow) -> R + Send> = Box::new(f);
        let fbox: Box<dyn for<'r> FnOnce(&'r glutin::GlWindow) -> R + Send + 'static> =
            unsafe { ::std::mem::transmute(fbox) };

        let (send, recv) = futures::sync::oneshot::channel();

        self.do_on_thread(move |win: &glutin::GlWindow| {
            let out = fbox(win);
            send.send(out).unwrap();
        });

        recv.wait().unwrap()
    }
}

fn glutin_event_to_js(cx: *mut JSContext, obj: HandleObject, event: glutin::Event) {
    use glutin::Event::*;
    rooted!(in(cx) let mut val = NullValue());

    let set_mouse_scroll_delta = |delta: glutin::MouseScrollDelta| {
        use glutin::MouseScrollDelta::*;
        rooted!(in(cx) let mut val = NullValue());
        match delta {
            LineDelta(x, y) => {
                setprop!(in(cx, val) (obj).deltakind = "line");
                setprop!(in(cx, val) (obj).x = x);
                setprop!(in(cx, val) (obj).y = y);
            }
            PixelDelta(x, y) => {
                setprop!(in(cx, val) (obj).deltakind = "pixel");
                setprop!(in(cx, val) (obj).x = x);
                setprop!(in(cx, val) (obj).y = y);
            }
        }
    };

    match event {
        DeviceEvent { .. } => {
            setprop!(in(cx, val) (obj).kind = "device");
        }
        WindowEvent { event, .. } => {
            use glutin::WindowEvent::*;
            setprop!(in(cx, val) (obj).kind = "window");
            match event {
                Resized(w, h) => {
                    setprop!(in(cx, val) (obj).type = "resized");
                    setprop!(in(cx, val) (obj).width = w);
                    setprop!(in(cx, val) (obj).height = h);
                }
                Moved(x, y) => {
                    setprop!(in(cx, val) (obj).type = "moved");
                    setprop!(in(cx, val) (obj).x = x);
                    setprop!(in(cx, val) (obj).y = y);
                }
                CloseRequested => {
                    setprop!(in(cx, val) (obj).type = "closed");
                }
                ReceivedCharacter(c) => {
                    setprop!(in(cx, val) (obj).type = "char");
                    setprop!(in(cx, val) (obj).char = c.encode_utf8(&mut [0; 4]));
                }
                Focused(focused) => {
                    setprop!(in(cx, val) (obj).type = "focused");
                    setprop!(in(cx, val) (obj).focused = focused);
                }
                KeyboardInput { input, .. } => {
                    setprop!(in(cx, val) (obj).type = "key");
                    setprop!(in(cx, val) (obj).scancode = input.scancode);
                }
                CursorMoved { position, .. } => {
                    setprop!(in(cx, val) (obj).type = "cursormoved");
                    setprop!(in(cx, val) (obj).x = position.0);
                    setprop!(in(cx, val) (obj).y = position.1);
                }
                CursorEntered { .. } => {
                    setprop!(in(cx, val) (obj).type = "cursorentered");
                }
                CursorLeft { .. } => {
                    setprop!(in(cx, val) (obj).type = "cursorleft");
                }
                MouseWheel { delta, .. } => {
                    setprop!(in(cx, val) (obj).type = "wheel");
                    set_mouse_scroll_delta(delta);
                }
                MouseInput { state, button, .. } => {
                    use glutin::ElementState::*;
                    use glutin::MouseButton::*;
                    setprop!(in(cx, val) (obj).type = "mouse");
                    setprop!(in(cx, val) (obj).pressed = state == Pressed);
                    setprop!(in(cx, val) (obj).button = match button {
                        Left => 0,
                        Right => 1,
                        Middle => 2,
                        Other(n) => n,
                    });
                }
                Refresh => {
                    setprop!(in(cx, val) (obj).type = "refresh");
                }
                Touch(touch) => {
                    use glutin::TouchPhase::*;
                    setprop!(in(cx, val) (obj).type = "touch");
                    setprop!(in(cx, val) (obj).phase = match touch.phase {
                        Started => "started",
                        Moved => "moved",
                        Ended => "ended",
                        Cancelled => "cancelled",
                    });
                    setprop!(in(cx, val) (obj).pressed = touch.phase == Started ||
                             touch.phase == Moved);
                    setprop!(in(cx, val) (obj).x = touch.location.0);
                    setprop!(in(cx, val) (obj).y = touch.location.1);
                    setprop!(in(cx, val) (obj).id = touch.id);
                }
                _ => (),
            }
        }
        Suspended { .. } => {
            setprop!(in(cx, val) (obj).kind = "suspended");
        }
        Awakened => {
            setprop!(in(cx, val) (obj).kind = "awakened");
        }
    }
}

pub struct Object<T: JSClassInitializer> {
    obj: *mut JSObject,
    marker: PhantomData<T>,
}

impl<T: JSClassInitializer> Object<T> {
    fn jsobj(&self) -> *mut JSObject {
        self.obj
    }

    fn private(&self) -> &T::Private {
        // private has already been verified by from_jsval
        unsafe { &*(JS_GetPrivate(self.obj) as *const _) }
    }
}

impl<T: JSClassInitializer> FromJSValConvertible for Object<T> {
    type Config = ();

    unsafe fn from_jsval(
        cx: *mut JSContext,
        value: mozjs::rust::HandleValue,
        _: (),
    ) -> Result<ConversionResult<Object<T>>, ()> {
        use std::borrow::Cow;

        if !value.is_object() {
            return Ok(ConversionResult::Failure(Cow::Borrowed(
                "value is not an object",
            )));
        }

        let obj = value.to_object();

        if !jsapi::JS_InstanceOf(
            cx,
            Handle::from_marked_location(&obj),
            T::class(),
            ptr::null_mut(),
        ) {
            return Ok(ConversionResult::Failure(Cow::Borrowed(
                "value is not instanceof the class",
            )));
        }

        let private = JS_GetPrivate(obj);
        if private.is_null() {
            return Ok(ConversionResult::Failure(Cow::Borrowed(
                "value has no private",
            )));
        }

        Ok(ConversionResult::Success(Object {
            obj: obj,
            marker: PhantomData,
        }))
    }
}

pub struct WebGLShader {
    id: Arc<AtomicUsize>,
}

js_class! { WebGLShader extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLShader,

    @constructor
    fn WebGLShader_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
    }

    fn toString(this: @this Object<WebGLShader>) -> JSRet<String> {
        Ok(format!("{{WebGLShader {}}}", this.private().id.load(Ordering::Relaxed)))
    }

    @op(finalize)
    fn WebGLShader_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, ptr::null_mut() as *mut _);
        let _ = Box::from_raw(private as *mut WebGLShader);
    }
}

pub struct WebGLProgram {
    id: Arc<AtomicUsize>,
}

js_class! { WebGLProgram extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLProgram,

    @constructor
    fn WebGLProgram_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
    }

    fn toString(this: @this Object<WebGLProgram>) -> JSRet<String> {
        Ok(format!("{{WebGLProgram {}}}", this.private().id.load(Ordering::Relaxed)))
    }

    @op(finalize)
    fn WebGLProgram_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, ptr::null_mut() as *mut _);
        let _ = Box::from_raw(private as *mut WebGLProgram);
    }
}

pub struct WebGLBuffer {
    id: Arc<AtomicUsize>,
}

js_class! { WebGLBuffer extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLBuffer,

    @constructor
    fn WebGLBuffer_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
    }

    fn toString(this: @this Object<WebGLBuffer>) -> JSRet<String> {
        Ok(format!("{{WebGLBuffer {}}}", this.private().id.load(Ordering::Relaxed)))
    }

    @op(finalize)
    fn WebGLBuffer_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, ptr::null_mut() as *mut _);
        let _ = Box::from_raw(private as *mut WebGLBuffer);
    }
}

pub struct WebGLUniformLocation {
    loc: Arc<AtomicUsize>,
}

js_class! { WebGLUniformLocation extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLUniformLocation,

    @constructor
    fn WebGLUniformLocation_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
    }

    fn toString(this: @this Object<WebGLUniformLocation>) -> JSRet<String> {
        Ok(format!("{{WebGLUniformLocation {}}}", this.private().loc.load(Ordering::Relaxed)))
    }

    @op(finalize)
    fn WebGLUniformLocation_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
        let private = JS_GetPrivate(this);
        if private.is_null() {
            return
        }

        JS_SetPrivate(this, ptr::null_mut() as *mut _);
        let _ = Box::from_raw(private as *mut WebGLUniformLocation);
    }
}

js_class! { Window extends ()
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
                    let _ac = JSAutoRealm::new(rcx.cx, jswin.get());
                    rooted!(in(rcx.cx) let obj = unsafe { JS_NewPlainObject(rcx.cx) });

                    //println!("event received: {:?}", &event);
                    match event {
                        WindowEvent::Glutin(ge) => glutin_event_to_js(rcx.cx, obj.handle().into(), ge),
                        WindowEvent::Closed => { println!("WindowEvent closed"); },
                    }

                    rooted!(in(rcx.cx) let mut onevent = NullValue());
                    let success = unsafe {
                        JS_GetProperty(rcx.cx,
                                       jswin.handle().into(),
                                       c_str!("onevent"),
                                       onevent.handle_mut().into())
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
                                    onevent.handle().into(),
                                    &args,
                                    ret.handle_mut().into())
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

    fn getContext(this: @this Object<Window>, str: String) -> JSRet<*mut JSObject> {
        println!("getContext: {}", str);
        Ok(this.jsobj())
    }

    fn ping(this: @this Object<Window>) -> JSRet<()> {
        println!("ping");

        if let Err(e) = this.private().send_msg.unbounded_send(WindowMsg::Ping) {
            println!("ping failed: {}", e);
        }

        Ok(())
    }

    fn close(this: @this Object<Window>) -> JSRet<()> {
        if let Err(e) = this.private().send_msg.unbounded_send(WindowMsg::Close) {
            println!("close failed: {}", e);
        }

        Ok(())
    }

    fn getError(this: @this Object<Window>)
        -> JSRet<GLenum> {
        Ok(this.private().sync_do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::GetError() }
            }))
    }

    fn clearColor(this: @this Object<Window>, r: f32, g: f32, b: f32, a: f32)
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::ClearColor(r, g, b, a) };
            });

        Ok(())
    }

    fn clear(this: @this Object<Window>, mask: u32 {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::Clear(mask) };
            });

        Ok(())
    }

    fn createShader(this: @this Object<Window>, rcx: &RJSContext,
                    shadertype: GLenum {ConversionBehavior::Default})
                   -> JSRet<*mut JSObject> {
        let shader_priv = Box::new(WebGLShader {
            id: Arc::new(AtomicUsize::new(0)),
        });

        let idref = Arc::clone(&shader_priv.id);

        let shader = WebGLShader::jsnew_with_private(
            rcx, Box::into_raw(shader_priv) as *mut WebGLShader);
        rooted!(in(rcx.cx) let shader = shader);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = unsafe { gl::CreateShader(shadertype) };
                idref.store(id as usize, Ordering::Relaxed);
            });

        Ok(shader.get())
    }

    fn shaderSource(this: @this Object<Window>, shader: Object<WebGLShader>, source: String)
        -> JSRet<()> {
        let idref = Arc::clone(&shader.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                unsafe { gl::ShaderSource(id as u32,
                                          1,
                                          &(source.as_ptr() as *const _),
                                          &(source.len() as _)) };
            });

        Ok(())
    }

    fn compileShader(this: @this Object<Window>, shader: Object<WebGLShader>)
        -> JSRet<()> {
        let idref = Arc::clone(&shader.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                unsafe { gl::CompileShader(id as u32) };
            });

        Ok(())
    }

    fn getShaderInfoLog(this: @this Object<Window>, shader: Object<WebGLShader>)
        -> JSRet<String> {
        let idref = Arc::clone(&shader.private().id);

        Ok(this.private().sync_do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                let mut len = 0;
                unsafe { gl::GetShaderiv(id as u32, gl::INFO_LOG_LENGTH, &mut len) };

                let mut v = vec![0; len as usize];
                let mut outlen = len;

                unsafe { gl::GetShaderInfoLog(id as u32,
                                          len as i32,
                                          &mut outlen,
                                          v.as_mut_ptr() as *mut _) };

                String::from_utf8(v)
            }).unwrap())
    }

    fn getShaderParameter(this: @this Object<Window>, shader: Object<WebGLShader>,
                          param: GLenum {ConversionBehavior::Default})
                         -> JSRet<JSVal> {
        let idref = Arc::clone(&shader.private().id);

        let out = this.private().sync_do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                let mut out = -1;
                unsafe { gl::GetShaderiv(id as u32, param, &mut out) };
                out
            });

        match param {
            gl::COMPILE_STATUS => Ok(jsval::BooleanValue(out > 0)),
            gl::DELETE_STATUS => Ok(jsval::BooleanValue(out > 0)),
            _ => Ok(jsval::Int32Value(out)),
        }
    }

    fn createProgram(this: @this Object<Window>, rcx: &RJSContext)
        -> JSRet<*mut JSObject> {
        let program_priv = Box::new(WebGLProgram {
            id: Arc::new(AtomicUsize::new(0)),
        });

        let idref = Arc::clone(&program_priv.id);

        let program = WebGLProgram::jsnew_with_private(
            rcx, Box::into_raw(program_priv) as *mut WebGLProgram);
        rooted!(in(rcx.cx) let program = program);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = unsafe { gl::CreateProgram() };
                idref.store(id as usize, Ordering::Relaxed);
            });

        Ok(program.get())
    }

    fn attachShader(this: @this Object<Window>, program: Object<WebGLProgram>,
                    shader: Object<WebGLShader>)
                   -> JSRet<()> {
        let progidref = Arc::clone(&program.private().id);
        let shaderidref = Arc::clone(&shader.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let prog = progidref.load(Ordering::Relaxed);
                let shader = shaderidref.load(Ordering::Relaxed);
                unsafe { gl::AttachShader(prog as u32, shader as u32) };
            });

        Ok(())
    }

    fn linkProgram(this: @this Object<Window>, program: Object<WebGLProgram>)
        -> JSRet<()> {
        let progidref = Arc::clone(&program.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let prog = progidref.load(Ordering::Relaxed);
                unsafe { gl::LinkProgram(prog as u32) };
            });

        Ok(())
    }

    fn getProgramParameter(this: @this Object<Window>, program: Object<WebGLProgram>,
                           param: GLenum {ConversionBehavior::Default})
                          -> JSRet<JSVal> {
        let idref = Arc::clone(&program.private().id);

        let out = this.private().sync_do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                let mut out = -1;
                unsafe { gl::GetProgramiv(id as u32, param, &mut out) };
                out
            });

        match param {
            gl::LINK_STATUS => Ok(jsval::BooleanValue(out > 0)),
            gl::DELETE_STATUS => Ok(jsval::BooleanValue(out > 0)),
            gl::VALIDATE_STATUS => Ok(jsval::BooleanValue(out > 0)),
            _ => Ok(jsval::Int32Value(out)),
        }
    }

    fn useProgram(this: @this Object<Window>, program: Object<WebGLProgram>)
        -> JSRet<()> {
        let progidref = Arc::clone(&program.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let prog = progidref.load(Ordering::Relaxed);
                unsafe { gl::UseProgram(prog as u32) };
            });

        Ok(())
    }

    fn getAttribLocation(this: @this Object<Window>, program: Object<WebGLProgram>, name: String)
        -> JSRet<i32> {
        let idref = Arc::clone(&program.private().id);

        let name = CString::new(name).unwrap();

        Ok(this.private().sync_do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                unsafe { gl::GetAttribLocation(id as u32, name.as_ptr()) }
            }))
    }

    fn enableVertexAttribArray(this: @this Object<Window>, index: u32 {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::EnableVertexAttribArray(index) };
            });

        Ok(())
    }

    fn disableVertexAttribArray(this: @this Object<Window>,
                                index: u32 {ConversionBehavior::Default})
                               -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::DisableVertexAttribArray(index) };
            });

        Ok(())
    }

    fn vertexAttribPointer(this: @this Object<Window>,
                           index: GLuint {ConversionBehavior::Default},
                           size: GLint {ConversionBehavior::Default},
                           type_: GLenum {ConversionBehavior::Default},
                           normalized: bool,
                           stride: GLsizei {ConversionBehavior::Default},
                           offset: u32 {ConversionBehavior::Default})
        -> JSRet<()>{
        // TODO: Validate these values
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::VertexAttribPointer(
                            index,
                            size,
                            type_,
                            normalized as u8,
                            stride,
                            offset as *const _) };
            });

        Ok(())
    }

    fn getUniformLocation(this: @this Object<Window>, rcx: &RJSContext,
                          program: Object<WebGLProgram>, name: String)
                         -> JSRet<*mut JSObject> {
        let ul_priv = Box::new(WebGLUniformLocation {
            loc: Arc::new(AtomicUsize::new(0)),
        });

        let idref = Arc::clone(&program.private().id);
        let locref = Arc::clone(&ul_priv.loc);

        let name = CString::new(name).unwrap();

        let ul = WebGLUniformLocation::jsnew_with_private(
            rcx, Box::into_raw(ul_priv) as *mut WebGLUniformLocation);
        rooted!(in(rcx.cx) let ul = ul);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);

                let loc = unsafe { gl::GetUniformLocation(id as u32, name.as_ptr()) };
                locref.store(loc as usize, Ordering::Relaxed);
            });

        Ok(ul.get())
    }

    fn createBuffer(this: @this Object<Window>, rcx: &RJSContext)
        -> JSRet<*mut JSObject> {
        let buffer_priv = Box::new(WebGLBuffer {
            id: Arc::new(AtomicUsize::new(0)),
        });

        let idref = Arc::clone(&buffer_priv.id);

        let buffer = WebGLBuffer::jsnew_with_private(
            rcx, Box::into_raw(buffer_priv) as *mut WebGLBuffer);
        rooted!(in(rcx.cx) let buffer = buffer);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let mut buffer = 0;
                unsafe { gl::CreateBuffers(1, &mut buffer) };
                idref.store(buffer as usize, Ordering::Relaxed);
            });

        Ok(buffer.get())
    }

    fn bindBuffer(this: @this Object<Window>, target: GLenum {ConversionBehavior::Default},
                  buffer: Object<WebGLBuffer>)
                 -> JSRet<()> {
        let idref = Arc::clone(&buffer.private().id);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let id = idref.load(Ordering::Relaxed);
                unsafe { gl::BindBuffer(target, id as u32) }
            });

        Ok(())
    }

    fn bufferData(rcx: &RJSContext,
                  this: @this Object<Window>,
                  target: GLenum {ConversionBehavior::Default},
                  data: JSVal,
                  usage: GLenum {ConversionBehavior::Default})
                 -> JSRet<()> {
        if !data.is_object() {
            return Err(Some("data is not an object. size?".to_owned()));
        }

        rooted!(in(rcx.cx) let obj = data.to_object());

        typedarray!(in(rcx.cx) let buf: ArrayBuffer = obj.get());
        typedarray!(in(rcx.cx) let view: ArrayBufferView = obj.get());

        // This construct looks ugly, but it should avoid having to copy the buffer data
        // TODO: Should we always do this? For small amounts of data it might be faster
        //       to copy and let the thread continue.

        let do_it = |slice: &[u8]| {
            this.private().sync_do_on_thread(
                move |_: &glutin::GlWindow| {
                    unsafe { gl::BufferData(target,
                                            slice.len() as isize,
                                            slice.as_ptr() as *const _,
                                            usage) }
                });
        };

        if let Ok(buf) = buf {
            do_it(unsafe { buf.as_slice() })
        } else if let Ok(view) = view {
            do_it(unsafe { view.as_slice() })
        } else {
            panic!("Not ArrayBuffer or ArrayBufferView");
        };


        Ok(())
    }

    fn uniformMatrix4fv(rcx: &RJSContext,
                  this: @this Object<Window>,
                  location: Object<WebGLUniformLocation>,
                  transpose: bool,
                  value: *mut JSObject)
                 -> JSRet<()> {
        typedarray!(in(rcx.cx) let view: Float32Array = value);

        // Since this should always be given 16 element arrays, just always copy them

        let data = if let Ok(view) = view {
            unsafe { view.as_slice().to_vec() }
        } else {
            rooted!(in(rcx.cx) let value = ObjectValue(value));
            unsafe { Vec::<f32>::from_jsval(rcx.cx, value.handle(), ()).to_result()? }
        };

        let locidref = Arc::clone(&location.private().loc);

        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                let loc = locidref.load(Ordering::Relaxed);
                unsafe { gl::UniformMatrix4fv(loc as i32,
                                              1,
                                              transpose as GLboolean,
                                              data.as_ptr()) }
            });

        Ok(())
    }

    fn enable(this: @this Object<Window>, cap: GLenum {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::Enable(cap) };
            });

        Ok(())
    }

    fn disable(this: @this Object<Window>, cap: GLenum {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::Disable(cap) };
            });

        Ok(())
    }

    fn viewport(this: @this Object<Window>,
                x: GLint {ConversionBehavior::Default},
                y: GLint {ConversionBehavior::Default},
                width: GLsizei {ConversionBehavior::Default},
                height: GLsizei {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::Viewport(x, y, width, height) };
            });

        Ok(())
    }

    fn drawArrays(this: @this Object<Window>,
                  mode: GLenum {ConversionBehavior::Default},
                  first: GLint {ConversionBehavior::Default},
                  count: GLsizei {ConversionBehavior::Default})
        -> JSRet<()> {
        this.private().do_on_thread(
            move |_: &glutin::GlWindow| {
                unsafe { gl::DrawArrays(mode, first, count) };
            });

        Ok(())
    }

    @op(finalize)
    fn Window_finalize(_free: *mut jsapi::JSFreeOp, this: *mut JSObject) -> () {
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
    Do(Box<dyn FnOnce(&glutin::GlWindow) + Send>),
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
    let context = glutin::ContextBuilder::new()
        .with_gl_profile(glutin::GlProfile::Core)
        .with_vsync(true);
    let gl_window = glutin::GlWindow::new(window, context, &events_loop).unwrap();

    unsafe {
        gl_window.make_current().unwrap();
        gl::load_with(|symbol| gl_window.get_proc_address(symbol) as *const _);
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);

        // OpenGL and GLES 2.0 treat VAO 0 specially
        let mut vao = 0;
        gl::CreateVertexArrays(1, &mut vao);
        gl::BindVertexArray(vao);
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
                        //println!("message Do");
                        func(&stuff.gl_window);
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

        //unsafe {
        //    gl::Clear(gl::COLOR_BUFFER_BIT);
        //}
        let now = Instant::now();
        stuff.gl_window.swap_buffers().unwrap();
        let swap_time = now.elapsed();
        let swap_ms = swap_time.subsec_nanos() as f32 / 1000000.0;
        if swap_ms > 1.0 {
            println!("swap took: {}ms", swap_ms);
        }
        thread::sleep(Duration::from_secs(1));

        events_loop.poll_events(|event| {
            match event {
                glutin::Event::WindowEvent { ref event, .. } => match *event {
                    glutin::WindowEvent::CloseRequested => {
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
    })
    .map_err(|_| ());

    let streams = handle_window_events
        .select(recv_msgs)
        .then(|_| -> Result<(), ()> { Ok(()) });
    let _ = core.run(stop_recv.select(streams)).map_err(|_| "Oh crap!");
    println!("window_thread exiting");
}
