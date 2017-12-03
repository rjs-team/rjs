
use mozjs::jsapi::JSClass;
use mozjs::jsapi::JSClassOps;
use mozjs::jsapi::JSFunctionSpec;
use mozjs::jsapi::JSNativeWrapper;
use mozjs::jsapi::JSPropertySpec;
use mozjs::jsapi::JSCLASS_RESERVED_SLOTS_SHIFT;

use mozjs::JSCLASS_GLOBAL_SLOT_COUNT;
use mozjs::JSCLASS_IS_GLOBAL;
use mozjs::JSCLASS_RESERVED_SLOTS_MASK;


use libc::c_char;
use libc::c_uint;

use std::ptr;

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
    fn class() -> &'static JSClass;
    fn functions() -> &'static [JSFunctionSpec];
    fn properties() -> &'static [JSPropertySpec];
}

#[macro_export]
macro_rules! js_class {
    ($name:ident $($body:tt)*) => {
        trace_macros!{true}

//pub struct $name;

//__jsclass_functions!{{} $($body)*}

lazy_static!{
    
    static ref _CLASS: JSClass = JSClass {
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
    };

    static ref _FUNCTIONS: &'static [JSFunctionSpec] = &[
        //__jsclass_functions!{{} $($body)*}
        null_function(),
    ];

    static ref _PROPERTIES: &'static [JSPropertySpec] = &[
        /*JSPropertySpec {
            name: b"window\0" as *const u8 as *const c_char,
            flags: (JSPROP_ENUMERATE | JSPROP_SHARED) as u8,
            getter: JSNativeWrapper {
                op: Some(window_window_getter_op),
                info: ptr::null(),
            },
            setter: null_wrapper(),
        },*/
        null_property(),
    ];

} // lazy_static

/*impl JSClassInitializer for $name {
    fn class() -> &'static JSClass {
        &_CLASS
    }

    fn functions() -> &'static [JSFunctionSpec] {
        &_FUNCTIONS
    }

    fn properties() -> &'static [JSPropertySpec] {
        &_PROPERTIES
    }
}
*/

    
    }
} // macro_rules! js_class

#[macro_export]
macro_rules! __jsclass_functionspecs {
    ({} fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} $($rest:tt)*) => {
        JSFunctionSpec {
            //name: b"log\0" as *const u8 as *const c_char,
            name: CString::new(stringify!($name)).into_raw(),
            selfHostedName: ptr::null(),
            flags: JSPROP_ENUMERATE as u16,
            nargs: 1,
            call: JSNativeWrapper {
                op: Some($name),
                info: ptr::null(),
            },
        },
        __jsclass_functionspecs!{{} $($rest)*}
    };
}

#[macro_export]
macro_rules! __jsclass_functions{
    ({} ) => {};
    ({} fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} $($rest:tt)*) => {
        js_fn!{fn $name $args -> $ret { $($body)* } }

        __jsclass_functions!{{} $($rest)*}
    };
}
