
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;

use tokio_core::reactor as tokio;
use futures::Future;
use futures::future;
use futures::Stream;
use futures::IntoFuture;
// use futures::future::{FutureResult};
// use tokio_timer::{Timer, TimerError};
use futures::sync::oneshot;
use futures::sync::mpsc;
use futures::sync::mpsc::UnboundedSender;
//use futures::future::IntoFuture;

//use std::ops::Deref;
use std::sync::{Arc, Weak};
use std::rc;
use std::rc::Rc;
//use std::boxed::FnBox;
use std::clone::Clone;
use std::marker::PhantomData;
use std::any::Any;
use slab::Slab;
use std::cell::{RefCell, Ref, RefMut};
use std::fmt::Debug;

//type EventLoopFn<T> = for<'t> Fn(&'t T, Handle<T>);
type Message<T> = (Remote<T>, Box<FnBox<T>>);

pub fn run<T, F>(t: &T, first: F)
    where T: Sized,
          F: FnOnce(Handle<T>) -> ()
{
    let mut core = tokio::Core::new().unwrap();

    let (tx, rx) = mpsc::unbounded::<Message<T>>();
    let tx = Arc::new(tx);

    let slab: Rc<RefCell<Slab<RefCell<Option<*mut ()>>>>> = Rc::new(RefCell::new(Slab::new()));

    let core_handle = core.handle();

    let remote = Remote(tx);
    let handle = Handle {
        remote: remote,
        thandle: core_handle.clone(),
        slab: Rc::downgrade(&slab),
    };


    let _ : Result<(), ()> = core.run(future::lazy(|| {

        first(handle);

        rx.for_each(|tuple| -> Result<(), ()> { 
            let (remote, f) = tuple;
            let handle = Handle {
                remote: remote,
                thandle: core_handle.clone(),
                slab: Rc::downgrade(&slab),
            };
            f.call_box(t, handle);
            Ok(())
        })
    }));
}

#[derive(Clone)]
pub struct Handle<T> {
    remote: Remote<T>,
    thandle: tokio::Handle,
    slab: rc::Weak<RefCell<Slab<RefCell<Option<*mut ()>>>>>,
}

impl<T> Handle<T> {
    pub fn core_handle(&self) -> &tokio::Handle {
        &self.thandle
    }
    pub fn remote(&self) -> &Remote<T> {
        &self.remote
    }

    pub fn store_new<V: 'static>(&self, v: V) -> RemoteRef<V> {
        let slab = self.slab.upgrade().unwrap();
        let mut slab = slab.borrow_mut();

        let key = slab.insert(RefCell::new(Some(Box::into_raw(Box::new(v)) as *mut ())));

        let (tx, rx) = oneshot::channel::<()>();
        let weakslab = self.slab.clone();
        self.thandle.spawn(rx.then(move|_| {
            println!("RemoteRef drop");
            let slab = weakslab.upgrade().unwrap();
            let mut slab = slab.borrow_mut();
            let r = slab.remove(key);
            let o = r.into_inner();
            match o {
                Some(p) => {
                    let b: Box<V> = unsafe { Box::from_raw(p as *mut V) };
                    drop(b);
                },
                None => (),
            };

            Ok(())
        }));

        RemoteRef {
            tx: Arc::new(tx),
            key: key,
            marker: PhantomData,
        }
    }

    pub fn retrieve<V: Debug + 'static>(&self, rref: RemoteRef<V>) -> Option<V> {
        let slab = self.slab.upgrade().unwrap();
        let slab = slab.borrow();
        let r = unsafe { slab.get_unchecked(rref.key) };
        let o = r.replace(None);
        let val = o.map(|p: *mut ()| {
            let b: Box<V> = unsafe { Box::from_raw(p as *mut V) };
            *b
        }); 
        println!("retrieved: {:?}", val);
        val
    }

    // This seems impossible to do without a Ref<Ref<V>>
    /*fn borrow<'h: 'r, 'r, V: 'static>(&self, rref: &'r RemoteRef<V>) -> Option<Ref<'r, V>> {
        let slab = self.slab.upgrade().unwrap();
        let slab = slab.borrow();
        let mut out = None;

        let refopt: Ref<Option<Ref<V>>> = Ref::map(slab, |slab| {
            let r: &RefCell<Option<*mut ()>> = unsafe { slab.get_unchecked(rref.key) };
            let ro = r.try_borrow();

            match ro {
                Err(_) => &out,
                Ok(rro) => {
                    out = rro.rewrap_map(|vp| unsafe { &*(*vp as *mut V) });
                    &out
                }
            }
        });
        match *refopt {
            Some(_) => Some(Ref::map(refopt, |o| o.unwrap())),
            None => None,
        }
    }
    fn borrow_mut<V: 'static>(&self, rref: &RemoteRef<V>) -> Option<RefMut<V>> {

        None
    }*/
}

/*trait Rewrap<'b, T> {
    fn rewrap_map<V, F: FnOnce(&T) -> &V>(self, f: F) -> Option<Ref<'b, V>>;
}
impl<'b, T> Rewrap<'b, T> for Ref<'b, Option<T>> {
    fn rewrap_map<V, F: FnOnce(&T) -> &V>(self, f: F) -> Option<Ref<'b, V>> {
        match *self {
            None => None,
            Some(_) => Some(Ref::map(self, |o| f(&o.unwrap()))),
        }
    }
}*/

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
        Remote(self.0.clone())
    }
}

impl<T> Remote<T> {

    pub fn spawn<F>(&self, f: F)
        where F: FnOnce(&T, Handle<T>) + Send + 'static
    {
        let me: Remote<T> = (*self).clone();
        let myfunc: Box<FnBox<T> + 'static> = Box::new( f );
        //let myfunc: Box<FnBox<T>> = Box::new( |a, b| f(a, b) );
        let fb = (me, myfunc);
        (*self.0).unbounded_send(fb).unwrap()
    }
}

trait FnBox<T>: Send {
    fn call_box(self: Box<Self>, t: &T, h: Handle<T>);
}

impl<T, F: FnOnce(&T, Handle<T>) + Send + 'static> FnBox<T> for F {
    fn call_box(self: Box<Self>, t: &T, h: Handle<T>) {
        (*self)(t, h)
    }
}

/*
#[derive(Debug)]
pub struct Handle<T: Sized>(Arc<Inner<T>>);
#[derive(Debug)]
pub struct WeakHandle<T>(Weak<Inner<T>>);

#[derive(Debug)]
pub struct Inner<T: Sized> {
    remote: tokio::Remote,
    tx: Arc<oneshot::Sender<()>>,
    t: T,
}


impl<T> Handle<T> {
    pub fn downgrade(&self) -> WeakHandle<T> {
        WeakHandle( Arc::downgrade(&self.0) )
    }

    pub fn spawn<F, R>(&self, f: F) 
        where
            F: FnOnce(&tokio::Handle) -> R + Send + 'static,
            R: IntoFuture<Item = (), Error = ()>,
            R::Future: 'static,
    {

        self.0.remote.spawn(f)
    }
}

impl<T> Deref for Handle<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0.t
    }
}

impl<T> WeakHandle<T> {
    pub fn upgrade(&self) -> Option<Handle<T>> {
        self.0.upgrade().map(|inner| Handle(inner))
    }
}
*/


