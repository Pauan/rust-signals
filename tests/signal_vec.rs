#![feature(pin, futures_api, arbitrary_self_types)]

extern crate pin_utils;
extern crate futures_core;
extern crate futures_util;
extern crate futures_executor;
extern crate futures_signals;

use futures_signals::signal_vec::{SignalVecExt, VecDiff};
use futures_core::Poll;

mod util;


#[test]
fn test_sum() {
    let input = util::Source::new(vec![
        Poll::Pending,
        Poll::Ready(VecDiff::Replace { values: vec![0, 1, 2, 3, 4, 5] }),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(VecDiff::InsertAt { index: 0, value: 6 }),
        Poll::Ready(VecDiff::InsertAt { index: 2, value: 7 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 0 }),
        Poll::Ready(VecDiff::UpdateAt { index: 4, value: 0 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Move { old_index: 1, new_index: 3 }),
        Poll::Pending,
        Poll::Ready(VecDiff::RemoveAt { index: 1 }),
        Poll::Pending,
        Poll::Ready(VecDiff::Clear {}),
    ]);

    let output = input.sum();

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(0)),
        Poll::Ready(Some(15)),
        Poll::Pending,
        Poll::Ready(Some(28)),
        Poll::Ready(Some(19)),
        Poll::Pending,
        Poll::Ready(Some(18)),
        Poll::Ready(Some(0)),
        Poll::Ready(None),
    ]);
}
