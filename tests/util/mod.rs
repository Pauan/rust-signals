use futures_signals::signal::Signal;
use futures_core::{Async, Never};
use futures_core::task::{Context, LocalMap, Waker, Wake};
use futures_util::future::poll_fn;
use futures_executor::{LocalPool, block_on};
use std::sync::Arc;


#[allow(dead_code)]
pub fn with_noop_context<U, F: FnOnce(&mut Context) -> U>(f: F) -> U {
    // borrowed this design from the futures source
    struct Noop;

    impl Wake for Noop {
        fn wake(_: &Arc<Self>) {}
    }

    let waker = Waker::from(Arc::new(Noop));

    let pool = LocalPool::new();
    let mut exec = pool.executor();
    let mut map = LocalMap::new();
    let mut cx = Context::new(&mut map, &waker, &mut exec);

    f(&mut cx)
}


#[allow(dead_code)]
pub fn get_all_polls<A, B, F>(mut signal: A, mut initial: B, mut f: F) -> Vec<Async<Option<A::Item>>> where A: Signal, F: FnMut(&B, &mut Context) -> B {
    let mut output = vec![];

    block_on(poll_fn(|cx| {
        loop {
            initial = f(&initial, cx);

            let x = signal.poll_change(cx);

            let x: Result<Async<()>, Never> = match x {
                Async::Ready(Some(_)) => {
                    output.push(x);
                    continue;
                },
                Async::Ready(None) => {
                    output.push(x);
                    Ok(Async::Ready(()))
                },
                Async::Pending => {
                    output.push(x);
                    Ok(Async::Pending)
                },
            };

            return x;
        }
    })).unwrap();

    output
}
