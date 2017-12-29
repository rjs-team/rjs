
use ::jslib::eventloop;
use mozjs::jsapi::JSContext;
use mozjs::jsapi::HandleObject;

#[derive(Debug)]
pub struct RJSContext {
    pub cx: *mut JSContext,
    pub global: HandleObject,
}

pub type RJSHandle = eventloop::Handle<RJSContext>;
pub type RJSRemote = eventloop::Remote<RJSContext>;
