
const METHODS: &'static [JSFunctionSpec] = &[
    JSFunctionSpec {
        name: b"addEventListener\0" as *const u8 as *const libc::c_char,
        call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
        nargs: 2,
        flags: JSPROP_ENUMERATE as u16,
        selfHostedName: 0 as *const libc::c_char
    },
    JSFunctionSpec {
        name: b"removeEventListener\0" as *const u8 as *const libc::c_char,
        call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
        nargs: 2,
        flags: JSPROP_ENUMERATE as u16,
        selfHostedName: 0 as *const libc::c_char
    },
    JSFunctionSpec {
        name: b"dispatchEvent\0" as *const u8 as *const libc::c_char,
        call: JSNativeWrapper { op: Some(generic_method), info: ptr::null() },
        nargs: 1,
        flags: JSPROP_ENUMERATE as u16,
        selfHostedName: 0 as *const libc::c_char
    },
    JSFunctionSpec {
        name: ptr::null(),
        call: JSNativeWrapper { op: None, info: ptr::null() },
        nargs: 0,
        flags: 0,
        selfHostedName: ptr::null()
    }
];

static CLASS: JSClass = JSClass {
    name: b"EventTargetPrototype\0" as *const u8 as *const libc::c_char,
    flags: 0,
    cOps: 0 as *const _,
    reserved: [0 as *mut _; 3]
};


unsafe extern "C" fn generic_method(_: *mut JSContext, _: u32, _: *mut Value) -> bool {
    true
}


fn defineAllGlobals() {

}
