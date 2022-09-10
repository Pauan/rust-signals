use std::task::Poll;
use futures_signals::signal::SignalExt;
use crate::util;


#[test]
fn test_map() {
    let input = util::Source::new(vec![
        Poll::Ready(0),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(1),
        Poll::Ready(5),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(0),
        Poll::Pending,
        Poll::Ready(3),
        Poll::Pending,
    ]);

    let output = input.map(move |x| x * 10);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(0)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(10)),
        Poll::Ready(Some(50)),
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(0)),
        Poll::Pending,
        Poll::Ready(Some(30)),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}
