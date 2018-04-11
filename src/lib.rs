#![recursion_limit="128"]

extern crate discard;
extern crate serde;

extern crate futures_channel;
extern crate futures_core;
extern crate futures_util;

#[cfg(test)]
extern crate futures_executor;

pub mod signal;
pub mod signal_vec;

// TODO should this be hidden from the docs ?
#[doc(hidden)]
pub mod internal;

mod macros;
