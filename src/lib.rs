#![recursion_limit="128"]
#![warn(unreachable_pub)]

///! It is *very highly* recommended to read the tutorial.
///! It explains all of the concepts you will need to use Signals effectively.

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

mod future;
pub use future::{cancelable_future, CancelableFutureHandle, CancelableFuture};

pub mod tutorial;
