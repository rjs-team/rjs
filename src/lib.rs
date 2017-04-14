
#![feature(const_fn)]
#![feature(libc)]
// #![cfg(feature = "debugmozjs")]

#[macro_use]
extern crate js;
extern crate libc;
extern crate tokio_core;
// extern crate tokio_timer;
extern crate futures;


#[cfg(test)]
mod tests;

pub mod jslib;