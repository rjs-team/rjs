
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

//type EventLoopFn<T> = for<'t> Fn(&'t T, Handle<T>);
type Message<T> = (Remote<T>, Box<FnBox<T>>);

pub fn run<T, F>(t: &T, first: F)
    where T: Sized,
          F: FnOnce(Handle<T>) -> ()
{
    let mut core = tokio::Core::new().unwrap();

    let (tx, rx) = mpsc::unbounded::<Message<T>>();
    let tx = Arc::new(tx);

    let slab: Rc<Slab<Box<Any>>> = Rc::new(Slab::new());

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
            f.call_box(&t, handle);
            Ok(())
        })
    }));
}

#[derive(Clone)]
pub struct Handle<T> {
    remote: Remote<T>,
    thandle: tokio::Handle,
    slab: rc::Weak<Slab<Box<Any>>>,
}

impl<T> Handle<T> {
    fn core_handle(&self) -> &tokio::Handle {
        &self.thandle
    }

    fn store<V: 'static>(&self, v: V) -> RemoteRef<V> {
        let slab = self.slab.upgrade().unwrap();

        let key = slab.insert(Box::new(v));

        let (tx, rx) = oneshot::channel::<()>();
        self.thandle.spawn(rx.then(|_| {
            let slab = self.slab.upgrade().unwrap();
            drop(slab.remove(key));
            Ok(())
        }));

        RemoteRef {
            tx: Arc::new(tx),
            key: key,
            marker: PhantomData,
        }
    }
}

#[derive(Clone)]
pub struct RemoteRef<V> {
    tx: Arc<oneshot::Sender<()>>,
    key: usize,
    marker: PhantomData<V>,
}

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


