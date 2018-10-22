#![recursion_limit="128"]
#![warn(unreachable_pub)]
// missing_docs
#![deny(missing_debug_implementations, macro_use_extern_crate)]

#![feature(futures_api, pin, arbitrary_self_types)]

///! It is *very highly* recommended to read the tutorial.
///! It explains all of the concepts you will need to use Signals effectively.

extern crate discard;
extern crate serde;

extern crate futures_channel;
extern crate futures_core;
extern crate futures_util;

extern crate pin_utils;

#[cfg(test)]
extern crate futures_executor;

pub mod signal;
pub mod signal_vec;

// TODO should this be hidden from the docs ?
#[doc(hidden)]
pub mod internal;

mod macros;

mod future;
pub use future::{cancelable_future, CancelableFutureHandle, CancelableFuture};

pub mod tutorial;
