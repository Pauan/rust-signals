use std::task::Poll;
use futures_signals::signal::SignalExt;
use crate::util;


#[test]
fn test_neq() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(5),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(2),
        Poll::Ready(3),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(0),
        Poll::Ready(1),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
    ]);

    let output = input.neq(1);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Ready(Some(false)),
        Poll::Pending,
        Poll::Ready(Some(true)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}
