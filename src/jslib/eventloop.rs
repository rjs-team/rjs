extern crate futures;
extern crate tokio_core;

use tokio_core::reactor as tokio;
use futures::Future;
use futures::future;
use futures::Stream;
use futures::sync::oneshot;
use futures::sync::mpsc;

use std::sync::{Arc, Weak};
use std::rc;
use std::rc::Rc;
use std::clone::Clone;
use std::marker::PhantomData;
use slab::Slab;
use std::cell::RefCell;
use std::fmt::Debug;
use downcast::Any;
use mozjs::rust::{GCMethods, Runtime, Trace};
use mozjs::jsapi::{GCForReason, Heap, JSGCInvocationKind, JSTracer, JS_AddExtraGCRootsTracer,
                   JS_RemoveExtraGCRootsTracer, Reason};

use std::os::raw::c_void;

//type EventLoopFn<T> = for<'t> Fn(&'t T, Handle<T>);
type Message<T> = (Remote<T>, Box<FnBox<T>>);

type RefSlab = RefCell<Slab<RefSlabEl>>;
type RefSlabEl = RefCell<Option<Box<Traceable>>>;

trait Traceable: Any {
    fn get_trace(&self) -> &Trace;
}

unsafe impl Trace for Traceable {
    unsafe fn trace(&self, trc: *mut JSTracer) {
        self.get_trace().trace(trc)
    }
}

impl<T: Trace + 'static> Traceable for T {
    fn get_trace(&self) -> &Trace {
        self
    }
}
downcast!(Traceable);

unsafe extern "C" fn ref_slab_tracer(trc: *mut JSTracer, data: *mut c_void) {
    if data.is_null() {
        return;
    }

    let slab = data as *mut rc::Weak<RefSlab>;
    let slab = (*slab).upgrade();

    if let Some(ref slab) = slab {
        slab.borrow().iter().for_each(|item| {
            let (_, item) = item;
            let optitem = item.borrow();
            if let Some(ref i) = *optitem {
                i.trace(trc);
            }
        })
    }
}

pub fn run<T, F>(rt: &Runtime, t: T, first: F)
where
    T: Sized,
    F: FnOnce(Handle<T>) -> (),
{
    let mut core = tokio::Core::new().unwrap();

    let (tx, rx) = mpsc::unbounded::<Message<T>>();
    let tx = Arc::new(tx);

    let slab: Rc<RefSlab> = Rc::new(RefCell::new(Slab::new()));

    let core_handle = core.handle();

    let data = rc::Rc::new(t);

    let remote = Remote(tx);
    let handle = Handle {
        remote: remote,
        thandle: core_handle.clone(),
        data: Rc::clone(&data),
        slab: Rc::downgrade(&slab),
    };

    let extradata: *mut rc::Weak<RefSlab> = Box::into_raw(Box::new(rc::Weak::clone(&handle.slab)));
    unsafe { JS_AddExtraGCRootsTracer(rt.rt(), Some(ref_slab_tracer), extradata as *mut _) };

    let _: Result<(), ()> = core.run(future::lazy(|| {
        first(handle);

        rx.for_each(|tuple| -> Result<(), ()> {
            let (remote, f) = tuple;
            let handle = Handle {
                remote: remote,
                thandle: core_handle.clone(),
                data: Rc::clone(&data),
                slab: Rc::downgrade(&slab),
            };
            unsafe { GCForReason(rt.rt(), JSGCInvocationKind::GC_SHRINK, Reason::NO_REASON) };
            f.call_box(handle);
            unsafe { GCForReason(rt.rt(), JSGCInvocationKind::GC_SHRINK, Reason::NO_REASON) };
            Ok(())
        })
    }));

    unsafe {
        JS_RemoveExtraGCRootsTracer(rt.rt(), Some(ref_slab_tracer), extradata as *mut _);
        Box::from_raw(extradata);
    }
}

pub struct Handle<T> {
    remote: Remote<T>,
    thandle: tokio::Handle,
    data: rc::Rc<T>,
    slab: rc::Weak<RefCell<Slab<RefSlabEl>>>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle {
            remote: self.remote.clone(),
            thandle: self.thandle.clone(),
            data: Rc::clone(&self.data),
            slab: rc::Weak::clone(&self.slab),
        }
    }
}

impl<T> Handle<T> {
    pub fn core_handle(&self) -> &tokio::Handle {
        &self.thandle
    }
    pub fn remote(&self) -> &Remote<T> {
        &self.remote
    }
    pub fn get(&self) -> &T {
        &self.data
    }

    pub fn downgrade(&self) -> WeakHandle<T> {
        WeakHandle {
            remote: Arc::downgrade(&self.remote.0),
            thandle: self.thandle.clone(),
            data: rc::Rc::downgrade(&self.data),
            slab: rc::Weak::clone(&self.slab),
        }
    }

    pub fn store_new<V: GCMethods + Copy + 'static>(&self, v: V) -> RemoteRef<V>
    where
        Heap<V>: Default + Trace,
    {
        let slab = self.slab.upgrade().unwrap();
        let mut slab = slab.borrow_mut();

        let valbox = Box::new(Heap::default());
        valbox.set(v);

        let key = slab.insert(RefCell::new(Some(valbox)));

        let (tx, rx) = oneshot::channel::<()>();
        let weakslab = rc::Weak::clone(&self.slab);
        self.thandle.spawn(rx.then(move |_| {
            let slab = weakslab.upgrade().unwrap();
            let mut slab = slab.borrow_mut();
            let r = slab.remove(key);
            let o = r.into_inner();
            if let Some(p) = o {
                let _: Box<V> = unsafe { p.downcast_unchecked::<V>() };
            }

            Ok(())
        }));

        RemoteRef {
            tx: Arc::new(tx),
            key: key,
            marker: PhantomData,
        }
    }

    pub fn retrieve<V: Debug + 'static>(&self, rref: &RemoteRef<V>) -> Option<V> {
        let slab = self.slab.upgrade().unwrap();
        let slab = slab.borrow();
        let r = unsafe { slab.get_unchecked(rref.key) };
        let o = r.replace(None);
        o.map(|p| {
            let b: Box<V> = unsafe { p.downcast_unchecked::<V>() };
            *b
        })
    }

    pub fn retrieve_copy<V: Copy + 'static>(&self, rref: &RemoteRef<V>) -> Option<V> {
        let slab = self.slab.upgrade().unwrap();
        let slab = slab.borrow();
        let r = unsafe { slab.get_unchecked(rref.key) };
        let o = &*r.borrow();
        match *o {
            None => None,
            Some(ref p) => {
                let v: &V = unsafe { p.downcast_ref_unchecked::<V>() };
                Some(*v)
            }
        }
    }
}

#[derive(Clone)]
pub struct WeakHandle<T> {
    remote: Weak<mpsc::UnboundedSender<Message<T>>>,
    thandle: tokio::Handle,
    data: rc::Weak<T>,
    slab: rc::Weak<RefCell<Slab<RefSlabEl>>>,
}

impl<T> WeakHandle<T> {
    pub fn upgrade(&self) -> Option<Handle<T>> {
        let remote = self.remote.upgrade();
        let data = self.data.upgrade();
        remote.and_then(|remote| {
            data.map(|data| {
                Handle {
                    remote: Remote(remote),
                    thandle: self.thandle.clone(),
                    data: data,
                    slab: rc::Weak::clone(&self.slab),
                }
            })
        })
    }
}

#[derive(Clone)]
pub struct RemoteRef<V> {
    tx: Arc<oneshot::Sender<()>>,
    key: usize,
    marker: PhantomData<*const V>,
}

unsafe impl<V> Send for RemoteRef<V> {}

pub struct Remote<T>(Arc<mpsc::UnboundedSender<Message<T>>>);
impl<T> Clone for Remote<T> {
    fn clone(&self) -> Self {
        Remote(Arc::clone(&self.0))
    }
}

impl<T> Remote<T> {
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Handle<T>) + Send + 'static,
    {
        let me: Remote<T> = (*self).clone();
        let myfunc: Box<FnBox<T> + 'static> = Box::new(f);
        //let myfunc: Box<FnBox<T>> = Box::new( |a, b| f(a, b) );
        let fb = (me, myfunc);
        (*self.0).unbounded_send(fb).unwrap()
    }
}

trait FnBox<T>: Send {
    fn call_box(self: Box<Self>, h: Handle<T>);
}

impl<T, F: FnOnce(Handle<T>) + Send + 'static> FnBox<T> for F {
    fn call_box(self: Box<Self>, h: Handle<T>) {
        (*self)(h)
    }
}
