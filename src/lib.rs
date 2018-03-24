#![recursion_limit="128"]

extern crate futures;
extern crate discard;
extern crate serde;

pub mod signal;
pub mod signal_vec;

// TODO should this be hidden from the docs ?
#[doc(hidden)]
pub mod internal;

mod macros;
