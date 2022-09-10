use std::task::Poll;
use futures_signals::signal::{SignalExt, always};
use crate::util;


#[test]
fn test_always() {
    let mut signal = always(1);

    util::with_noop_context(|cx| {
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(Some(1)));
        assert_eq!(signal.poll_change_unpin(cx), Poll::Ready(None));
    });
}
