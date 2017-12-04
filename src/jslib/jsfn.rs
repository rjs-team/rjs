use mozjs::conversions::ToJSValConvertible;

pub type JSRet<T: ToJSValConvertible> = Result<T, Option<String>>;

#[macro_export]
macro_rules! js_fn_raw {
    (fn $name:ident($($param:ident : $type:ty),*) -> JSRet<$ret:ty> $body:tt) => (
        #[allow(non_snake_case)] 
        unsafe extern "C" fn $name (cx: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
            let args = CallArgs::from_vp(vp, argc);
            let rt = JS_GetRuntime(cx);
            let rcx = JS_GetRuntimePrivate(rt) as *mut RJSContext;
            assert!((*rcx).cx == cx);

            fn rustImpl($($param : $type),*) -> JSRet<$ret> $body

            let result = rustImpl(cx, &*rcx, args);
            match result {
                Ok(v) => {
                    v.to_jsval(cx, args.rval());
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

#[macro_export]
macro_rules! js_fn {
    (fn $name:ident ($rcx:ident : &'static RJSContext $($args:tt)*) -> JSRet<$ret:ty> $body:tt) => {
        js_fn_raw!{fn $name (cx: *mut JSContext, $rcx: &'static RJSContext, args: CallArgs) -> JSRet<$ret> {
            js_unpack_args!({stringify!($name), cx, args} ($($args)*));

            $body

        }}

    }

}


#[macro_export]
macro_rules! js_callback {
    ($rcx:ident, move |$($param:ident : $type:ty),*| $body:tt) => (
        (move |tx: Arc<oneshot::Sender<()>>| {
            move |$($param : $type),*| {
                let _ac = JSAutoCompartment::new($rcx.cx, $rcx.global.get());

                let ret = (|$($param : $type),*| $body) ($($param),*);

                drop(tx); // this drops the handle that keeps the main thread alive

                ret
            }
        })($rcx.tx.upgrade().unwrap()) // this passes a handle to keep the main thread alive
    )
}

#[macro_export]
macro_rules! js_unpack_args {
    ({$fn:expr, $cx:expr, $callargs:expr} (, $($args:tt)*)) => {
        js_unpack_args!({$fn, $cx, $callargs} ($($args)*));
    };
    ({$fn:expr, $cx:expr, $callargs:expr} ($($args:tt)*)) => {
        if $callargs._base.argc_ != _js_unpack_args_count!($($args)*,) {
            return Err(Some(format!("{}() requires exactly {} argument", $fn, _js_unpack_args_count!($($args)*,)).into()));
        }
        _js_unpack_args_unwrap_args!(($cx, $callargs, 0) $($args)*,);
    };
}

#[macro_export]
macro_rules! _js_unpack_args_count {
    ($(,)*) => {
        0
    };
    ($name:ident: @$special:ident, $($args:tt)*) => {
        _js_unpack_args_count!($($args)*)
    };
    ($name:ident: $ty:ty, $($args:tt)*) => {
        1 + _js_unpack_args_count!($($args)*)
    };
    ($name:ident: $ty:ty {$opt:expr}, $($args:tt)*) => {
        1 + _js_unpack_args_count!($($args)*)
    };
}

#[macro_export]
macro_rules! _js_unpack_args_unwrap_args {
    (($cx:expr, $callargs:expr, $n:expr) $(,)*) => {
    };
    // special: @this
    (($cx:expr, $callargs:expr, $n:expr) $name:ident : @this, $($args:tt)*) => {
        let $name = $callargs.thisv();
        _js_unpack_args_unwrap_args!(($cx, $args, $n+1) $($args)*);
    };
    // options
    (($cx:expr, $callargs:expr, $n:expr) $name:ident : $type:ty {$opt:expr}, $($args:tt)*) => {
        let $name = unsafe { <$type>::from_jsval($cx, $callargs.get($n), $opt).to_result()? };
        _js_unpack_args_unwrap_args!(($cx, $callargs, $n+1) $($args)*);
    };
    // no options
    (($cx:expr, $callargs:expr, $n:expr) $name:ident : $type:ty, $($args:tt)*) => {
        let $name = unsafe { <$type>::from_jsval($cx, $callargs.get($n), ()).to_result()? };
        _js_unpack_args_unwrap_args!(($cx, $callargs, $n+1) $($args)*);
    };
}
