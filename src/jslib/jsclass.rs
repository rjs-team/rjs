use mozjs::jsapi::{CallArgs, Handle, HandleObject, JSClass, JSContext, JSFunctionSpec,
                   JSNativeWrapper, JSObject, JSPropertySpec, JS_GetConstructor,
                   JS_GetInstancePrivate, JS_InitClass, JS_SetPrivate,
                   JSCLASS_RESERVED_SLOTS_SHIFT};
use mozjs::JSCLASS_RESERVED_SLOTS_MASK;
use jslib::jsfn::RJSFn;
use jslib::context;
use jslib::context::{ClassInfo, RJSContext};
use libc::c_uint;
use std::ptr;

pub const JSCLASS_HAS_PRIVATE: c_uint = 1;
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

pub trait GetJSClassInfo
where
    Self: Sized + 'static,
{
    fn class_info(rcx: &RJSContext) -> Option<context::ClassInfo>;
}

impl GetJSClassInfo for () {
    fn class_info(_rcx: &RJSContext) -> Option<context::ClassInfo> {
        Some(ClassInfo {
            constr: ptr::null_mut(),
            prototype: ptr::null_mut(),
        })
    }
}

pub trait JSClassInitializer {
    type Private;

    unsafe fn init_class(rcx: &RJSContext, obj: HandleObject) -> context::ClassInfo
    where
        Self: Sized + 'static,
    {
        let cls = Self::class();
        let parent_info = Self::parent_info(rcx).unwrap();
        let constr = Self::constr();
        let (constrfn, constrnargs) = constr
            .map(|c| (Some(c.func()), c.nargs()))
            .unwrap_or((None, 0));
        let props = Self::properties();
        let fns = Self::functions();
        let static_props = Self::static_properties();
        let static_fns = Self::static_functions();

        rooted!(in(rcx.cx) let parent_proto = parent_info.prototype);

        rooted!(in(rcx.cx) let proto = JS_InitClass(
            rcx.cx,
            obj,
            parent_proto.handle(),
            cls,
            constrfn,
            constrnargs,
            props,
            fns,
            static_props,
            static_fns,
        ));

        let constr = JS_GetConstructor(rcx.cx, proto.handle());

        let classinfo = context::ClassInfo {
            constr: constr,
            prototype: proto.get(),
        };

        rcx.set_classinfo_for::<Self>(classinfo);

        classinfo
    }
    fn class() -> *const JSClass;

    // Get the ClassInfo for this class, if it has been defined already.
    fn class_info(rcx: &RJSContext) -> Option<ClassInfo>;

    // This unfortunately has to be provided by the macro in order for
    // init_class to be able to obtain the parent prototype.
    fn parent_info(rcx: &RJSContext) -> Option<ClassInfo>;
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

    fn get_private<'a>(
        cx: *mut JSContext,
        obj: HandleObject,
        args: Option<CallArgs>,
    ) -> Option<&'a Self::Private> {
        unsafe {
            let ptr = JS_GetInstancePrivate(
                cx,
                obj,
                Self::class(),
                args.map_or(ptr::null_mut(), |mut args| &mut args),
            ) as *const Self::Private;
            if ptr.is_null() {
                None
            } else {
                Some(&*ptr)
            }
        }
    }

    fn jsnew_with_private(rcx: &RJSContext, private: *mut Self::Private) -> *mut JSObject
    where
        Self: Sized + 'static,
    {
        let info = rcx.get_classinfo_for::<Self>()
            .expect(&format!("{} must be defined in this compartment!", "?"));

        let jsobj = unsafe {
            ::mozjs::jsapi::JS_NewObjectWithGivenProto(
                rcx.cx,
                Self::class(),
                Handle::from_marked_location(&info.prototype),
            )
        };

        unsafe {
            JS_SetPrivate(jsobj, private as *mut ::std::os::raw::c_void);
        }

        jsobj
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
    ($name:ident extends $parent:ty [$flags:expr] $($body:tt)*) => {
        //trace_macros!{true}
        __jsclass_parse!{$name [$parent] [$flags] [()] [] [] [] [] $($body)*}
    };
}

#[macro_export]
macro_rules! __jsclass_parsed {
    ($name:ident [$parent:ty] [$flags:expr] [$private:ty] [$($constr:tt)*] [$($fns:tt)*]
     [$($ops:tt)*] [$($props:tt)*]) => {

$( __jsclass_toplevel!{_constr $constr} )*
//$( __jsclass_toplevel!{_fn $fns} )*
$( __jsclass_toplevel!{_op $ops} )*
$( __jsclass_toplevel!{_prop $props} )*



impl JSClassInitializer for $name {
    type Private = $private;

    fn class() -> *const JSClass {
        compute_once!{
            *const JSClass = ptr::null();
            {
                Box::into_raw(Box::new( JSClass {
                    name: c_str!(stringify!($name)),
                    flags: $flags,
                    cOps: __jsclass_ops!([] $($ops)*),
                    reserved: [ptr::null_mut() as *mut _; 3],
                }))
            }
        }
    }

    fn class_info(rcx: &RJSContext) -> Option<$crate::jslib::context::ClassInfo> {
        rcx.get_classinfo_for::<Self>()
    }

    fn parent_info(rcx: &RJSContext) -> Option<$crate::jslib::context::ClassInfo> {
        use $crate::jslib::jsclass::GetJSClassInfo;
        //rcx.get_classinfo_for::<$parent>()
        <$parent>::class_info(rcx)
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
    ($cname:ident $parent:tt $flags:tt $private:tt $constr:tt $fns:tt $ops:tt $props:tt ) => {
        __jsclass_parsed!{$cname $parent $flags $private $constr $fns $ops $props}
    };
    ($cname:ident $parent:tt $flags:tt [$private:ty] $constr:tt $fns:tt $ops:tt $props:tt
     private: $ty:ty, $($rest:tt)*) => {
        __jsclass_parse!{$cname $parent $flags [$ty] $constr $fns $ops $props
        $($rest)*}
    };
    ($cname:ident $parent:tt $flags:tt $private:tt $constr:tt [$($fns:tt)*] $ops:tt $props:tt
     fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $parent $flags $private $constr [$($fns)*
            [fn $name $args -> JSRet<$ret> { $($body)* }]
        ] $ops $props
        $($rest)*}
    };
    ($cname:ident $parent:tt $flags:tt $private:tt [$($constr:tt)*] $fns:tt $ops:tt $props:tt
     @constructor fn $name:ident $args:tt -> JSRet<$ret:ty> {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $parent $flags $private [$($constr)*
            [fn $name $args -> JSRet<$ret> { $($body)* }]
        ] $fns $ops $props
        $($rest)*}
    };
    ($cname:ident $parent:tt $flags:tt $private:tt $constr:tt $fns:tt [$($ops:tt)*] $props:tt
     @op($op:ident) fn $name:ident $args:tt -> $ret:ty {$($body:tt)*} $($rest:tt)*) => {
        __jsclass_parse!{$cname $parent $flags $private $constr $fns [$($ops)*
            [$op fn $name $args -> $ret { $($body)* }]
        ] $props
        $($rest)*}
    };
    ($cname:ident $parent:tt $flags:tt $private:tt $constr:tt $fns:tt $ops:tt [$($props:tt)*]
     @prop $name:ident $body:tt $($rest:tt)*) => {
        __jsclass_parse!{$cname $parent $flags $private $constr $fns $ops [$($props)*
            [$name $body]
        ]
        $($rest)*}
    };
}

#[macro_export]
macro_rules! __jsclass_ops {
    ([] $($body:tt)*) => {
        __jsclass_ops!{[{None}, {None}, {None}, {None}, {None}, {None}, {None}, {None}, {None},
            {None}, {None}, {None}] $($body)* }
    };

    ([$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt,
     $t:tt] ) => {
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
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt,
        $t:tt]
        [_op [addProperty fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [{Some($fname)}, $ca, $co, $dp, $e, $f, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt,
        $t:tt]
        [finalize fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [$ap, $ca, $co, $dp, $e, {Some($fname)}, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt,
        $t:tt]
        [call fn $fname:ident $args:tt -> $ret:ty { $($fbody:tt)* }]
         $($body:tt)*
     ) => {
        __jsclass_ops!{
            [$ap, {Some($fname)}, $co, $dp, $e, $f, $gp, $hi, $mr, $r, $sp, $t]
            $($body)*
        }
    };
    (
        [$ap:tt, $ca:tt, $co:tt, $dp:tt, $e:tt, $f:tt, $gp:tt, $hi:tt, $mr:tt, $r:tt, $sp:tt,
        $t:tt]
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
        {

        __jsclass_toplevel!{_fn [fn $name $args -> JSRet<$ret> { $($body)*}]}

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
        }
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
    (_prop [ $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty>
        {$($body:tt)*} $($rest:tt)* } ] ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_toplevel!{_prop [ $name { $($rest)* } ]}
    };
    (_prop [ $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty>
        {$($body:tt)*} $($rest:tt)* } ] ) => {
        js_fn!{fn $fname $args -> JSRet<$ret> { $($body)* } }
        __jsclass_toplevel!{_prop [ $name { $($rest)* } ]}
    };
}

#[macro_export]
macro_rules! __jsclass_propertyspec {
    ($vec:ident [$name:ident {$($rest:tt)*}]) => {
        __jsclass_propertyspec!{{$vec, null_wrapper(), null_wrapper()} @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr}
     @prop $name:ident {}) => {
        $vec.push(
            JSPropertySpec {
                //name: b"window\0" as *const u8 as *const c_char,
                //name: CString::new(stringify!($name)).unwrap().into_raw(),
                name: concat!(stringify!($name), "\0").as_ptr()
                    as *const ::std::os::raw::c_char,
                flags: (JSPROP_ENUMERATE | JSPROP_SHARED) as u8,
                getter: $getter,
                setter: $setter,
            },
        );
    };

    ({$vec:ident, $getter:expr, $setter:expr}
     @prop $name:ident { get fn $fname:ident $args:tt -> JSRet<$ret:ty>
         {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, JSNativeWrapper {
                op: Some($fname{}.func()), info: ptr::null()
            }, $setter}
            @prop $name { $($rest)* }}
    };
    ({$vec:ident, $getter:expr, $setter:expr}
     @prop $name:ident { set fn $fname:ident $args:tt -> JSRet<$ret:ty>
         {$($body:tt)*} $($rest:tt)* } ) => {
        __jsclass_propertyspec!{{$vec, $getter, JSNativeWrapper {
                op: Some($fname{}.func()), info: ptr::null()
            }}
            @prop $name { $($rest)* }}
    };
}
