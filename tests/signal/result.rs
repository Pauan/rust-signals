use std::task::Poll;
use futures_signals::signal::{SignalExt, Always, result};
use crate::util;


#[test]
fn test_result() {
    let mut signal = result::<Always<()>, _>(Err(5));

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(Err(5))));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}

#[test]
fn test_result_signal() {
    let test_value = 0;

    let input = match test_value {
        0 => Ok(util::Source::new(vec![
            Poll::Ready(1),
            Poll::Pending,
            Poll::Ready(3),
            Poll::Pending,
        ])),
        _ => Err("hello"),
    };

    let output = result(input);

    util::assert_signal_eq(output, vec![
        Poll::Ready(Some(Ok(1))),
        Poll::Pending,
        Poll::Ready(Some(Ok(3))),
        Poll::Pending,
        Poll::Ready(None),
    ]);
}
