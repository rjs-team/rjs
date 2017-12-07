
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;

use tokio_core::reactor as tokio;
//use futures::Future;
use futures::future;
use futures::Stream;
// use futures::future::{FutureResult};
// use tokio_timer::{Timer, TimerError};
use futures::sync::mpsc;
//use futures::future::IntoFuture;

//use std::ops::Deref;
use std::sync::{Arc, Weak};
use std::boxed::FnBox;
use std::clone::Clone;


pub fn run<T, F>(t: &T, first: F)
    where T: Sized,
          F: FnOnce(Handle<T>) -> ()
{
    let mut core = tokio::Core::new().unwrap();

    let (tx, rx) = mpsc::unbounded::<Box<for<'t> FnBox(&'t T)>>();
    let tx = Arc::new(tx);

    let handle = Handle(tx);

    let _ : Result<(), ()> = core.run(future::lazy(|| {

        first(handle);


        rx.for_each(|f| -> Result<(), ()> { 
            f.call_box((&t,));
            Ok(())
        })
    }));

}

pub struct Handle<T>(Arc<mpsc::UnboundedSender<Box<for<'t> FnBox(&'t T)>>>);
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle(self.0.clone())
    }
}

impl<T> Handle<T> {

    pub fn spawn<F>(&self, f: F)
        where F: for<'aa> FnOnce(&'aa T, Self) + Send
    {
        let me: Handle<T> = (*self).clone();
        let fb = Box::new(|t| { f(t, me) });
        self.0.unbounded_send(fb).unwrap()
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


