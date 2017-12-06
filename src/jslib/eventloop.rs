
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;

use tokio_core::reactor as tokio;
use futures::Future;
use futures::future;
// use futures::future::{FutureResult};
// use tokio_timer::{Timer, TimerError};
use futures::sync::oneshot;
use futures::future::IntoFuture;

use std::ops::Deref;
use std::sync::{Arc, Weak};


pub fn run<T, F>(t: T, first: F)
    where T: Sized,
          F: FnOnce(Handle<T>) -> ()
{
    let mut core = tokio::Core::new().unwrap();

    // this is used to keep track of all pending daemons and callbacks, when there are no more handles to tx, rx will close and the main thread will exit
    let (tx, rx) = oneshot::channel::<()>();
    let tx = Arc::new(tx);

    let remote = core.remote();

    let _ : Result<(), ()> = core.run(future::lazy(|| {

        first(Handle(Arc::new(Inner {
            remote: remote,
            tx: tx,
            t: t,
        }) ));


        rx.then(|_| Ok(()))
    }));

}

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



