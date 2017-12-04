
use mozjs::jsapi::JSClass;
//use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSFunctionSpec;
use mozjs::jsapi::JSNativeWrapper;
use mozjs::jsapi::JSPropertySpec;
use mozjs::jsapi::JSCLASS_RESERVED_SLOTS_SHIFT;

//use mozjs::JSCLASS_GLOBAL_SLOT_COUNT;
//use mozjs::JSCLASS_IS_GLOBAL;
use mozjs::JSCLASS_RESERVED_SLOTS_MASK;


//use libc::c_char;
use libc::c_uint;

use std::ptr;
//use std::sync::Mutex;

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
    fn class() -> *const JSClass;
    fn functions() -> *const JSFunctionSpec;
    fn properties() -> *const JSPropertySpec;
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
macro_rules! js_class {
    ($name:ident $($body:tt)*) => {
        //trace_macros!{true}

//pub struct $name;

__jsclass_foreach!{{nothing, __jsclass_property, __jsclass_function} {} $($body)*}


impl JSClassInitializer for $name {
    fn class() -> *const JSClass {
        compute_once!{
            *const JSClass = ptr::null();
            {
                Box::into_raw(Box::new( JSClass {
                    name: CString::new(stringify!($name)).unwrap().into_raw(),
                    flags: jsclass_has_reserved_slots(2),
                    cOps: &JSClassOps {
                        addProperty: None,
                        call: None,
                        construct: None,
                        delProperty: None,
                        enumerate: None,
                        finalize: None,
                        getProperty: None,
                        hasInstance: None,
                        mayResolve: None,
                        resolve: None,
                        setProperty: None,
                        trace: None,
                    },
                    reserved: [0 as *mut _; 3],
                }))
            }
        }
    }

    fn functions() -> *const JSFunctionSpec {
        compute_once!{
            *const JSFunctionSpec = ptr::null();
            {
                let mut fspecs: Vec<JSFunctionSpec> = vec![];

                __jsclass_foreach!{{nothing, nothing, __jsclass_functionspec} {fspecs} $($body)*};
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

                __jsclass_foreach!{{nothing, __jsclass_propertyspec, nothing} {pspecs} $($body)*};
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
macro_rules! __jsclass_foreach {
    ($ms:tt $margs:tt ) => { };
    ({$mop:ident, $mprop:ident, $mfn:ident} $margs:tt  fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)*) => {
        $mfn!{$margs fn $name $args -> JSRet<$ret> { $($body)* }}
        __jsclass_foreach!{{$mop, $mprop, $mfn} $margs $($rest)*}
    };
    ({$mop:ident, $mprop:ident, $mfn:ident} $margs:tt  @op($op:ident) fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} $($rest:tt)*) => {
        $mop!{$margs $op fn $name $args -> $ret { $($body)* }}
        __jsclass_foreach!{{$mop, $mprop, $mfn} $margs $($rest)*}
    };
    ({$mop:ident, $mprop:ident, $mfn:ident} $margs:tt  @prop $name:ident $body:tt $($rest:tt)*) => {
        $mprop!{$margs @prop $name $body }
        __jsclass_foreach!{{$mop, $mprop, $mfn} $margs $($rest)*}
    };
}

#[macro_export]
macro_rules! __jsclass_functionspec {
    ({$vec:ident} fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*}) => {
        $vec.push(
            JSFunctionSpec {
                //name: b"log\0" as *const u8 as *const c_char,
                name: CString::new(stringify!($name)).unwrap().into_raw(),
                selfHostedName: ptr::null(),
                flags: JSPROP_ENUMERATE as u16,
                nargs: 1,
                call: JSNativeWrapper {
                    op: Some($name),
                    info: ptr::null(),
                },
            }
        );
    };
}

#[macro_export]
macro_rules! __jsclass_function {
    ({} fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*}) => {
        js_fn!{fn $name $args -> JSRet<$ret> { $($body)* } }
    };
}

#[macro_export]
macro_rules! __jsclass_propertyspec {
    ({$vec:ident} @prop $name:ident {$($rest:tt)*}) => {
        __jsclass_propertyspec!{{$vec, null_wrapper(), null_wrapper()} @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident {}) => {
        $vec.push(
            JSPropertySpec {
                //name: b"window\0" as *const u8 as *const c_char,
                name: CString::new(stringify!($name)).unwrap().into_raw(),
                flags: (JSPROP_ENUMERATE | JSPROP_SHARED) as u8,
                getter: $getter,
                setter: $setter,
            },
        );
    };

    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, JSNativeWrapper { op: Some($fname), info: ptr::null() }, $setter} @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr} @prop $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, $getter, JSNativeWrapper { op: Some($fname), info: ptr::null() }} @prop $name { $($rest)* }}
    };
}

#[macro_export]
macro_rules! __jsclass_property {
    ({} @prop $name:ident {}) => {
    };

    ({} @prop $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_property!{{} @prop $name { $($rest)* }}
    };
    ({} @prop $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)* } ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_property!{{} @prop $name { $($rest)* }}
    };
}
