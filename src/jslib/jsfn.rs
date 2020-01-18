use mozjs::jsapi::HandleObject;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSFunction;
use mozjs::jsapi::JS_DefineFunction;
use mozjs::jsapi::Value;

use std::ffi::CStr;

pub type JSRet<T> = Result<T, Option<String>>;

pub type RJSNativeRaw = unsafe extern "C" fn(*mut JSContext, u32, *mut Value) -> bool;

pub trait RJSFn {
    fn func(&self) -> RJSNativeRaw;
    fn name(&self) -> &'static CStr;
    fn nargs(&self) -> u32;
    #[allow(clippy::missing_safety_doc)]
    unsafe fn define_on(
        &self,
        cx: *mut JSContext,
        this: HandleObject,
        flags: u32,
    ) -> *mut JSFunction {
        JS_DefineFunction(
            cx,
            this,
            self.name().as_ptr(),
            Some(self.func()),
            self.nargs(),
            flags,
        )
    }
}

#[macro_export]
macro_rules! js_fn_raw {
    (fn $name:ident($($param:ident : $type:ty),*) -> JSRet<$ret:ty> $body:tt) => (
        #[allow(non_snake_case)]
        unsafe extern "C" fn $name (cx: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
            let args = CallArgs::from_vp(vp, argc);
            let handle = context::get_handle(cx).unwrap();
            let rcx = handle.get();
            assert!(rcx.cx == cx);

            fn rustImpl($($param : $type),*) -> JSRet<$ret> $body

            let result = rustImpl(rcx, &handle, args);
            match result {
                Ok(v) => {
                    v.to_jsval(cx, mozjs::rust::MutableHandle::from_raw(args.rval()));
                    true
                },
                Err(Some(s)) => {
                    let cstr = CString::new(s).unwrap();
                    JS_ReportErrorUTF8(cx, cstr.as_ptr() as *const libc::c_char);
                    false
                },
                Err(None) => {
                    false
                },
            }

        }
    )
}

#[macro_export]
macro_rules! js_fn {
    (fn $name:ident ($($args:tt)*) -> JSRet<$ret:ty> $body:tt) => {
        #[allow(non_camel_case_types)]
        pub struct $name;

        impl $name {

            js_fn_raw!{fn rawfunc (_rcx: &RJSContext,
                                   _handle: &RJSHandle,
                                   args: CallArgs) -> JSRet<$ret> {
                js_unpack_args!({stringify!($name), _rcx, _handle, args} ($($args)*));

                $body

            }}
        }

        impl RJSFn for $name {

            fn func(&self) -> jslib::jsfn::RJSNativeRaw {
                $name::rawfunc
            }

            fn name(&self) -> &'static ::std::ffi::CStr {
                unsafe {
                    ::std::ffi::CStr::from_bytes_with_nul_unchecked(
                         concat!(stringify!($name), "\0").as_bytes())
                }
            }

            fn nargs(&self) -> u32 {
                _js_unpack_args_count!($($args)*,)
            }

        }


    }

}

#[macro_export]
macro_rules! js_callback {
    ($rcx:ident, move |$($param:ident : $type:ty),*| $body:tt) => (
        (move |tx: Arc<oneshot::Sender<()>>| {
            move |$($param : $type),*| {
                let _ac = JSAutoRealm::new($rcx.cx, $rcx.global.get());

                let ret = (|$($param : $type),*| $body) ($($param),*);

                drop(tx); // this drops the handle that keeps the main thread alive

                ret
            }
        })($rcx.tx.upgrade().unwrap()) // this passes a handle to keep the main thread alive
    )
}

#[macro_export]
macro_rules! js_unpack_args {
    ({$fn:expr, $rcx:expr, $remote:expr, $callargs:expr} (, $($args:tt)*)) => {
        js_unpack_args!({$fn, $rcx, $remote, $callargs} ($($args)*));
    };
    ({$fn:expr, $rcx:expr, $remote:expr, $callargs:expr} ($($args:tt)*)) => {
        #[allow(unused_imports)]
        use mozjs::conversions::FromJSValConvertible;

        if $callargs.argc_ != _js_unpack_args_count!($($args)*,) {
            return Err(Some(format!("{}() requires exactly {} argument", $fn,
                _js_unpack_args_count!($($args)*,)).into()));
        }
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, 0) $($args)*,);
    };
}

#[macro_export]
macro_rules! _js_unpack_args_count {
    () => {
        0
    };
    ($name:ident: @$special:ident, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: @$special:ident $type:ty, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: @$special:ident $type:ty {$opt:expr}, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: &RJSContext, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: &RJSHandle, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: CallArgs, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: $ty:ty, $($args:tt)*) => {
        1 + _js_unpack_args_count!($($args)*)
    };
    ($name:ident: $ty:ty {$opt:expr}, $($args:tt)*) => {
        1 + _js_unpack_args_count!($($args)*)
    };
    ($(,)+ $($rest:tt)*) => {
        _js_unpack_args_count!($($rest)*)
    };
}

#[macro_export]
macro_rules! _js_unpack_args_unwrap_args {
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $(,)*) => {
    };
    // special: @this
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : @this $type:ty, $($args:tt)*) => {
        let $name = unsafe {
            <$type as FromJSValConvertible>::from_jsval($rcx.cx, mozjs::rust::Handle::from_raw($callargs.thisv()), ())
                .to_result()? };
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n) $($args)*);
    };
    // special: @this with options
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : @this $type:ty {$opt:expr}, $($args:tt)*) => {
        let $name = unsafe {
            <$type as FromJSValConvertible>::from_jsval($rcx.cx, $callargs.thisv(), $opt)
                .to_result()? };
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n) $($args)*);
    };
    // RJSContext
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : &RJSContext, $($args:tt)*) => {
        let $name: &RJSContext = $rcx;
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n) $($args)*);
    };
    // RJSHandle
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : &RJSHandle, $($args:tt)*) => {
        let $name: &RJSHandle = $remote;
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n) $($args)*);
    };
    // CallArgs
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : CallArgs, $($args:tt)*) => {
        let $name = $callargs;
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n) $($args)*);
    };
    // options
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : $type:ty {$opt:expr}, $($args:tt)*) => {
        let $name = unsafe {
            <$type as FromJSValConvertible>::from_jsval($rcx.cx, mozjs::rust::Handle::from_raw($callargs.get($n)), $opt)
                .to_result()? };
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n+1) $($args)*);
    };
    // no options
    (($rcx:expr, $remote:expr, $callargs:expr, $n:expr)
     $name:ident : $type:ty, $($args:tt)*) => {
        let $name = unsafe {
            <$type as FromJSValConvertible>::from_jsval($rcx.cx, mozjs::rust::Handle::from_raw($callargs.get($n)), ())
                .to_result()? };
        _js_unpack_args_unwrap_args!(($rcx, $remote, $callargs, $n+1) $($args)*);
    };
}
