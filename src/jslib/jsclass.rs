
use mozjs::jsapi::JSContext;
use mozjs::jsapi::JSObject;
use mozjs::jsapi::JS_InitClass;
use mozjs::jsapi::HandleObject;
use mozjs::jsapi::JSClass;
//use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSFunctionSpec;
use mozjs::jsapi::JSNativeWrapper;
use mozjs::jsapi::JSPropertySpec;
use mozjs::jsapi::JSCLASS_RESERVED_SLOTS_SHIFT;

//use mozjs::JSCLASS_GLOBAL_SLOT_COUNT;
//use mozjs::JSCLASS_IS_GLOBAL;
use mozjs::JSCLASS_RESERVED_SLOTS_MASK;

use jslib::jsfn::RJSFn;

//use libc::c_char;
use libc::c_uint;

use std::ptr;
//use std::sync::Mutex;

pub const JSCLASS_HAS_PRIVATE: c_uint = 1 << 0;
pub const fn jsclass_has_reserved_slots(n: c_uint) -> c_uint {
    (n & JSCLASS_RESERVED_SLOTS_MASK) << JSCLASS_RESERVED_SLOTS_SHIFT
}

pub const fn null_wrapper() -> JSNativeWrapper {
    JSNativeWrapper {
        op: None,
        info: ptr::null(),
    }
}

pub const fn null_property() -> JSPropertySpec {
    JSPropertySpec {
        name: ptr::null(),
        flags: 0,
        getter: null_wrapper(),
        setter: null_wrapper(),
    }
}

pub const fn null_function() -> JSFunctionSpec {
    JSFunctionSpec {
        name: ptr::null(),
        flags: 0,
        call: null_wrapper(),
        nargs: 0,
        selfHostedName: ptr::null(),
    }
}

pub trait JSClassInitializer {
    unsafe fn init_class(cx: *mut JSContext, obj: HandleObject) -> *mut JSObject {

        let parent_proto = HandleObject::null();
        let cls = Self::class();
        let constr = Self::constr();
        let (constrfn, constrnargs) = constr.map(|c| (Some(c.func()), c.nargs())).unwrap_or((None, 0));
        let props = Self::properties();
        let fns = Self::functions();
        let static_props = Self::static_properties();
        let static_fns = Self::static_functions();

        JS_InitClass(cx, obj, parent_proto, cls, constrfn, constrnargs, props, fns, static_props, static_fns)
    }
    fn class() -> *const JSClass;
    fn functions() -> *const JSFunctionSpec;
    fn properties() -> *const JSPropertySpec;
    fn static_functions() -> *const JSFunctionSpec {
        ptr::null()
    }
    fn static_properties() -> *const JSPropertySpec {
        ptr::null()
    }
    fn constr() -> Option<Box<RJSFn>> {
        None
    }
}


#[macro_export]
macro_rules! compute_once {
    ($type:ty = $static:expr ; $body:tt) => {
        unsafe {
            static mut VAL : $type = $static;
            static ONCE: Once = ONCE_INIT;

            ONCE.call_once(|| {
                VAL = $body;
            });

            VAL
        }
    }
}

#[macro_export]
macro_rules! c_str {
    ($str:expr) => {
        concat!($str, "\0").as_ptr() as *const ::std::os::raw::c_char
    }
}

#[macro_export]
macro_rules! js_class {

    ($name:ident [$flags:expr] $($body:tt)*) => {
        //trace_macros!{true}
        __jsclass_parse!{$name [$flags] [()] [] [] [] [] $($body)*}
    };
}

#[macro_export]
macro_rules! __jsclass_parsed {
    ($name:ident [$flags:expr] [$private:ty] [$($constr:tt)*] [$($fns:tt)*] [$($ops:tt)*] [$($props:tt)*]) => {

$( __jsclass_toplevel!{_constr $constr} )*
$( __jsclass_toplevel!{_fn $fns} )*
$( __jsclass_toplevel!{_op $ops} )*
$( __jsclass_toplevel!{_prop $props} )*


impl $name {

    fn get_private(cx: *mut JSContext, obj: Handle<*mut JSObject>, args: &mut CallArgs) -> Option<*mut $private> {
        unsafe {
            let ptr = JS_GetInstancePrivate(cx, obj, Self::class(), args as *mut CallArgs) as *mut $private;
            if ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        }
    }
}

impl JSClassInitializer for $name {
    fn class() -> *const JSClass {
        compute_once!{
            *const JSClass = ptr::null();
            {
                Box::into_raw(Box::new( JSClass {
                    //name: CString::new(stringify!($name)).unwrap().into_raw(),
                    name: c_str!(stringify!($name)),
                    flags: $flags,
                    cOps: __jsclass_ops!([] $($ops)*),
                    reserved: [0 as *mut _; 3],
                }))
            }
        }
    }


    fn constr() -> Option<Box<RJSFn>> {

        $(
            __jsclass_constrspec!{$constr}
        )*

        #[allow(unreachable_code)]
        None
    }

    fn functions() -> *const JSFunctionSpec {
        compute_once!{
            *const JSFunctionSpec = ptr::null();
            {
                let mut fspecs: Vec<JSFunctionSpec> = vec![];

                $(
                    __jsclass_functionspec!{fspecs $fns}
                )*
                fspecs.push(null_function());

                let fboxptr = Box::into_raw(fspecs.into_boxed_slice());
                &(*fboxptr)[0]
            }
        }
    }

    fn properties() -> *const JSPropertySpec {
        compute_once!{
            *const JSPropertySpec = ptr::null();
            {
                let mut pspecs: Vec<JSPropertySpec> = vec![];

                $(
                    __jsclass_propertyspec!{pspecs $props}
                )*
                pspecs.push(null_property());

                let pboxptr = Box::into_raw(pspecs.into_boxed_slice());
                &(*pboxptr)[0]
            }
        }
    }
}


    
    }
} // macro_rules! js_class

#[macro_export]
macro_rules! nothing {
    ($($any:tt)*) => {}
}


#[macro_export]
macro_rules! __jsclass_parse {
    ($cname:ident $flags:tt $private:tt $constr:tt $fns:tt $ops:tt $props:tt ) => {
        __jsclass_parsed!{$cname $flags $private $constr $fns $ops $props}
    };
    ($cname:ident $flags:tt [$private:ty] $constr:tt $fns:tt $ops:tt $props:tt
     private: $ty:ty, $($rest:tt)*) => {
        __jsclass_parse!{$cname $flags [$ty] $constr $fns $ops $props
        $($rest)*}
    };
    ($cname:ident $flags:tt $private:tt $constr:tt [$($fns:tt)*] $ops:tt $props:tt
     fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $flags $private $constr [$($fns)*
            [fn $name $args -> JSRet<$ret> { $($body)* }]
        ] $ops $props
        $($rest)*}
    };
    ($cname:ident $flags:tt $private:tt [$($constr:tt)*] $fns:tt $ops:tt $props:tt
     @constructor fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $flags $private [$($constr)*
            [fn $name $args -> JSRet<$ret> { $($body)* }]
        ] $fns $ops $props
        $($rest)*}
    };
    ($cname:ident $flags:tt $private:tt $constr:tt $fns:tt [$($ops:tt)*] $props:tt
     @op($op:ident) fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $flags $private $constr $fns [$($ops)*
            [$op fn $name $args -> $ret { $($body)* }]
        ] $props
        $($rest)*}
    };
    ($cname:ident $flags:tt $private:tt $constr:tt $fns:tt $ops:tt [$($props:tt)*]
     @prop $name:ident $body:tt $($rest:tt)*) => {
        __jsclass_parse!{$cname $flags $private $constr $fns $ops [$($props)*
            [$name $body]
        ]
        $($rest)*}
    };
}

#[macro_export]
macro_rules! __jsclass_ops {
    ([] $($body:tt)*) => {
        __jsclass_ops!{[{None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}] $($body)* }
    };

    ([$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt, $t:tt] ) => {
        &JSClassOps {
            addProperty: $ap,
            call: $ca,
            construct: $co,
            delProperty: $dp,
            enumerate: $e,
            finalize: $f,
            getProperty: $gp,
            hasInstance: $hi,
            mayResolve: $mr,
            resolve: $r,
            setProperty: $sp,
            trace: $t,
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt, $t:tt]
        [_op [addProperty fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [{Some($fname)}, $ca, $co, $dp, $e, $f, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt, $t:tt]
        [finalize fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [$ap, $ca, $co, $dp, $e, {Some($fname)}, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt, $t:tt]
        [call fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [$ap, {Some($fname)}, $co, $dp, $e, $f, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt, $t:tt]
        [$oname:ident fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]
         $($body:tt)*
     ) => {
        compile_error!("Bad op name" + stringify!($oname))
    };
    (
        $ops:tt
        [$cname:ident $cbody:tt]
         $($body:tt)*
     ) => {
        __jsclass_ops!{$ops $($body)* }
    };
}


#[macro_export]
macro_rules! __jsclass_constrspec {
    ([fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*}]) => {
        return Some(Box::new($name{}));
    };
}

#[macro_export]
macro_rules! __jsclass_functionspec {
    ($vec:ident [fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*}]) => {
        $vec.push(
            JSFunctionSpec {
                //name: b"log\0" as *const u8 as *const c_char,
                //name: CString::new(stringify!($name)).unwrap().into_raw(),
                name: concat!(stringify!($name), "\0").as_ptr() as *const ::std::os::raw::c_char,
                selfHostedName: ptr::null(),
                flags: JSPROP_ENUMERATE as u16,
                nargs: $name{}.nargs() as u16,
                call: JSNativeWrapper {
                    op: Some($name{}.func()),
                    info: ptr::null(),
                },
            }
        );
    };
}

#[macro_export]
macro_rules! __jsclass_toplevel {
    (_fn [ fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} ]) => {
        js_fn!{fn $name $args -> JSRet<$ret> { $($body)* } }
    };
    (_constr [ fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} ]) => {
        js_fn!{fn $name $args -> JSRet<$ret> { $($body)* } }
    };
    (_op [$oname:ident fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} ]) => {
        #[allow(non_snake_case)]
        unsafe extern "C" fn $name $args -> $ret { $($body)* }
    };
    (_prop [ $name:ident {} ]) => {};
    (_prop [ $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ] ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_toplevel!{_prop [ $name { $($rest)* } ]}
    };
    (_prop [ $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ] ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_toplevel!{_prop [ $name { $($rest)* } ]}
    };
}

#[macro_export]
macro_rules! __jsclass_propertyspec {
    ($vec:ident [$name:ident {$($rest:tt)*}]) => {
        __jsclass_propertyspec!{{$vec, null_wrapper(), null_wrapper()} @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident {}) => {
        $vec.push(
            JSPropertySpec {
                //name: b"window\0" as *const u8 as *const c_char,
                //name: CString::new(stringify!($name)).unwrap().into_raw(),
                name: concat!(stringify!($name), "\0").as_ptr() as *const ::std::os::raw::c_char,
                flags: (JSPROP_ENUMERATE | JSPROP_SHARED) as u8,
                getter: $getter,
                setter: $setter,
            },
        );
    };

    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, JSNativeWrapper { op: Some($fname{}.func()), info: ptr::null() }, $setter} @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, $getter, JSNativeWrapper { op: Some($fname{}.func()), info: ptr::null() }} @prop $name { $($rest)* }}
    };
}

