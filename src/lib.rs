#![feature(const_fn)]
#![feature(libc)]
#![feature(trace_macros)]
#![feature(fnbox)]
#![feature(refcell_replace_swap)]
// #![cfg(feature = "debugmozjs")]

extern crate libc;
extern crate mozjs;
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;
//#[macro_use]
#[macro_use]
extern crate downcast;
extern crate lazy_static;
extern crate slab;

#[cfg(test)]
mod tests;

#[macro_use]
pub mod jslib;
