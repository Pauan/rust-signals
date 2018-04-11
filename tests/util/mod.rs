use futures_core::task::{Context, LocalMap, Waker, Wake};
use futures_executor::LocalPool;
use std::sync::Arc;


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
