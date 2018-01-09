#![feature(fnbox)]
#![feature(const_fn)]
#![feature(libc)]
#![feature(alloc)]
#![recursion_limit = "100"]

extern crate alloc;
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
            JSContext, JSObject, JS_CallFunctionValue, JS_NewArrayObject1, JS_NewGlobalObject,
            JS_NewObjectForConstructor, JS_ReportError, JS_SetElement, OnNewGlobalHookOption,
            Value};
use jsval::{JSVal, ObjectValue, UndefinedValue};
use mozjs::conversions::{ConversionBehavior, ConversionResult, FromJSValConvertible,
                         ToJSValConvertible};
use mozjs::jsapi::{Handle, HandleObject, JSClass, JSClassOps, JSFunctionSpec, JSNativeWrapper,
                   JSPropertySpec, JS_GetPrivate, JS_GetProperty, JS_InitStandardClasses,
                   JS_NewPlainObject, JS_SetPrivate, JS_SetProperty, JSPROP_ENUMERATE,
                   JSPROP_SHARED};
use mozjs::jsapi;
use mozjs::jsval::NullValue;
use mozjs::jsval;
use mozjs::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use rjs::jslib::context;
use rjs::jslib::context::{RJSContext, RJSHandle};
use rjs::jslib::eventloop;
use rjs::jslib::jsclass::{null_function, null_property, null_wrapper, JSClassInitializer,
                          JSCLASS_HAS_PRIVATE};
use rjs::jslib::jsfn::{JSRet, RJSFn};
use std::boxed::FnBox;
use std::env;
use std::ffi::CString;
use std::fs::File;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::ptr;
use std::sync::{Once, ONCE_INIT};
use std::thread;
use std::time::{Duration, Instant};
use tokio_core::reactor::{Core, Timeout};
use gl::types::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::marker::PhantomData;
use std::fmt::Debug;

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
    let rcx = RJSContext::new(cx, global);

    eventloop::run(&rt, rcx, |handle| {
        let rcx = handle.get();
        let _ac = JSAutoCompartment::new(cx, global.get());

        context::store_private(cx, &handle);

        let _ = unsafe { JS_InitStandardClasses(cx, global) };

        unsafe {
            let _ = puts.define_on(cx, global, 0);
            let _ = setTimeout.define_on(cx, global, 0);
            let _ = getFileSync.define_on(cx, global, 0);
            let _ = readDir.define_on(cx, global, 0);

            Test::init_class(rcx, global);
            Window::init_class(rcx, global);
            WebGLShader::init_class(rcx, global);
            WebGLProgram::init_class(rcx, global);
            WebGLBuffer::init_class(rcx, global);
            WebGLUniformLocation::init_class(rcx, global);
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

    let _ac = JSAutoCompartment::new(cx, global.get());
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
    rooted!(in(cx) let stackstr = jsval::StringValue(&*stackstr.get()));
    let stackstr = String::from_jsval(cx, stackstr.handle(), ())
        .to_result()
        .unwrap();
    println!("{}", stackstr);
}

struct Test {}

js_class!{ Test extends ()
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

    fn sync_do_on_thread<F, R: Debug + Send + 'static>(&self, f: F) -> R
    where
        F: for<'r> FnBox(&'r glutin::GlWindow) -> R + Send + 'static,
    {
        let fbox = Box::new(f);

        let (send, recv) = futures::sync::oneshot::channel();

        self.do_on_thread(move |win: &glutin::GlWindow| {
            let out = fbox.call_box((win,));
            send.send(out).unwrap();
        });

        recv.wait().unwrap()
    }
}

macro_rules! setprop {
    (in($cx:expr) ($obj:expr) . $prop:ident = $val:expr) => {
        rooted!(in($cx) let mut val = NullValue());
        unsafe {
            $val.to_jsval($cx, val.handle_mut());
            JS_SetProperty($cx, $obj, c_str!(stringify!($prop)),
                val.handle());
        }
    }
}

fn glutin_event_to_js(cx: *mut JSContext, obj: HandleObject, event: glutin::Event) {
    use glutin::Event::*;

    let set_mouse_scroll_delta = |delta: glutin::MouseScrollDelta| {
        use glutin::MouseScrollDelta::*;
        match delta {
            LineDelta(x, y) => {
                setprop!(in(cx) (obj).deltakind = "line");
                setprop!(in(cx) (obj).x = x);
                setprop!(in(cx) (obj).y = y);
            }
            PixelDelta(x, y) => {
                setprop!(in(cx) (obj).deltakind = "pixel");
                setprop!(in(cx) (obj).x = x);
                setprop!(in(cx) (obj).y = y);
            }
        }
    };

    match event {
        DeviceEvent { .. } => {
            setprop!(in(cx) (obj).kind = "device");
        }
        WindowEvent { event, .. } => {
            use glutin::WindowEvent::*;
            setprop!(in(cx) (obj).kind = "window");
            match event {
                Resized(w, h) => {
                    setprop!(in(cx) (obj).type = "resized");
                    setprop!(in(cx) (obj).width = w);
                    setprop!(in(cx) (obj).height = h);
                }
                Moved(x, y) => {
                    setprop!(in(cx) (obj).type = "moved");
                    setprop!(in(cx) (obj).x = x);
                    setprop!(in(cx) (obj).y = y);
                }
                Closed => {
                    setprop!(in(cx) (obj).type = "closed");
                }
                ReceivedCharacter(c) => {
                    setprop!(in(cx) (obj).type = "char");
                    setprop!(in(cx) (obj).char = c.encode_utf8(&mut [0; 4]));
                }
                Focused(focused) => {
                    setprop!(in(cx) (obj).type = "focused");
                    setprop!(in(cx) (obj).focused = focused);
                }
                KeyboardInput { input, .. } => {
                    setprop!(in(cx) (obj).type = "key");
                    setprop!(in(cx) (obj).scancode = input.scancode);
                }
                CursorMoved { position, .. } => {
                    setprop!(in(cx) (obj).type = "cursormoved");
                    setprop!(in(cx) (obj).x = position.0);
                    setprop!(in(cx) (obj).y = position.1);
                }
                CursorEntered { .. } => {
                    setprop!(in(cx) (obj).type = "cursorentered");
                }
                CursorLeft { .. } => {
                    setprop!(in(cx) (obj).type = "cursorleft");
                }
                MouseWheel { delta, .. } => {
                    setprop!(in(cx) (obj).type = "wheel");
                    set_mouse_scroll_delta(delta);
                }
                MouseInput { state, button, .. } => {
                    use glutin::ElementState::*;
                    use glutin::MouseButton::*;
                    setprop!(in(cx) (obj).type = "mouse");
                    setprop!(in(cx) (obj).pressed = state == Pressed);
                    setprop!(in(cx) (obj).button = match button {
                        Left => 0,
                        Right => 1,
                        Middle => 2,
                        Other(n) => n,
                    });
                }
                Refresh => {
                    setprop!(in(cx) (obj).type = "refresh");
                }
                Touch(touch) => {
                    use glutin::TouchPhase::*;
                    setprop!(in(cx) (obj).type = "touch");
                    setprop!(in(cx) (obj).phase = match touch.phase {
                        Started => "started",
                        Moved => "moved",
                        Ended => "ended",
                        Cancelled => "cancelled",
                    });
                    setprop!(in(cx) (obj).pressed = touch.phase == Started ||
                             touch.phase == Moved);
                    setprop!(in(cx) (obj).x = touch.location.0);
                    setprop!(in(cx) (obj).y = touch.location.1);
                    setprop!(in(cx) (obj).id = touch.id);
                }
                _ => (),
            }
        }
        Suspended { .. } => {
            setprop!(in(cx) (obj).kind = "suspended");
        }
        Awakened => {
            setprop!(in(cx) (obj).kind = "awakened");
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
        value: HandleValue,
        _: (),
    ) -> Result<ConversionResult<Object<T>>, ()> {
        use alloc::borrow::Cow;

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

js_class!{ WebGLShader extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLShader,

    @constructor
    fn WebGLShader_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
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

js_class!{ WebGLProgram extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLProgram,

    @constructor
    fn WebGLProgram_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
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

js_class!{ WebGLBuffer extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLBuffer,

    @constructor
    fn WebGLBuffer_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
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

js_class!{ WebGLUniformLocation extends ()
    [JSCLASS_HAS_PRIVATE]
    private: WebGLUniformLocation,

    @constructor
    fn WebGLUniformLocation_constr() -> JSRet<*mut JSObject> {
        Ok(ptr::null_mut())
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

js_class!{ Window extends ()
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
                        WindowEvent::Glutin(ge) => glutin_event_to_js(rcx.cx, obj.handle(), ge),
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
