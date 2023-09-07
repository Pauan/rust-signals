use std::task::Poll;
use futures_signals::signal::SignalExt;
use crate::util;


#[test]
fn test_stop_if() {
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

    let output = input.stop_if(move |x| *x == 5);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(0)),
        Poll::Pending,
        Poll::Pending,
        Poll::Ready(Some(1)),
        Poll::Ready(Some(5)),
        Poll::Ready(None),
    ]);
}
