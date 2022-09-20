#![recursion_limit="128"]

mod util;


#[macro_export]
macro_rules! map_tests {
    ($name:ident, ($($ref:tt)+), ($($arg:tt)+)) => {
        #[cfg(test)]
        mod $name {
            use super::util;
            use std::task::Poll;
            use futures_signals::$name;
            use futures_signals::signal::{SignalExt, always};

            #[test]
            fn send_sync() {
                let _: Box<dyn Send + Sync> = Box::new($name! {
                    let _a = always(1) => ()
                });

                let _: Box<dyn Send + Sync> = Box::new($name! {
                    let _a = always(1),
                    let _b = always(2) => ()
                });

                let _: Box<dyn Send + Sync> = Box::new($name! {
                    let _a = always(1),
                    let _b = always(2),
                    let _c = always(3) => ()
                });

                let _: Box<dyn Send + Sync> = Box::new($name! {
                    let _a = always(1),
                    let _b = always(2),
                    let _c = always(3),
                    let _d = always(4) => ()
                });

                let _: Box<dyn Send + Sync> = Box::new($name! {
                    let _a = always(1),
                    let _b = always(2),
                    let _c = always(3),
                    let _d = always(4),
                    let _e = always(5) => ()
                });
            }

            #[test]
            fn ident_1() {
                let a = always(1);

                let mut s = $name!(a => {
                    let a: u32 = *a;
                    a + 1
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(2)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn ident_2() {
                let a = always(1);
                let b = always(2);

                let mut s = $name!(a, b => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    a + b
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(3)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn ident_3() {
                let a = always(1);
                let b = always(2);
                let c = always(3);

                let mut s = $name!(a, b, c => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    a + b + c
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(6)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn ident_4() {
                let a = always(1);
                let b = always(2);
                let c = always(3);
                let d = always(4);

                let mut s = $name!(a, b, c, d => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    let d: u32 = *d;
                    a + b + c + d
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(10)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn ident_5() {
                let a = always(1);
                let b = always(2);
                let c = always(3);
                let d = always(4);
                let e = always(5);

                let mut s = $name!(a, b, c, d, e => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    let d: u32 = *d;
                    let e: u32 = *e;
                    a + b + c + d + e
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(15)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }


            #[test]
            fn let_1() {
                let a2 = always(1);

                let mut s = $name!(let a = a2 => {
                    let a: u32 = *a;
                    a + 1
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(2)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_2() {
                let a2 = always(1);
                let b2 = always(2);

                let mut s = $name!(let a = a2, let b = b2 => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    a + b
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(3)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_3() {
                let a2 = always(1);
                let b2 = always(2);
                let c2 = always(3);

                let mut s = $name!(let a = a2, let b = b2, let c = c2 => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    a + b + c
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(6)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_4() {
                let a2 = always(1);
                let b2 = always(2);
                let c2 = always(3);
                let d2 = always(4);

                let mut s = $name!(let a = a2, let b = b2, let c = c2, let d = d2 => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    let d: u32 = *d;
                    a + b + c + d
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(10)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_5() {
                let a2 = always(1);
                let b2 = always(2);
                let c2 = always(3);
                let d2 = always(4);
                let e2 = always(5);

                let mut s = $name!(let a = a2, let b = b2, let c = c2, let d = d2, let e = e2 => {
                    let a: u32 = *a;
                    let b: u32 = *b;
                    let c: u32 = *c;
                    let d: u32 = *d;
                    let e: u32 = *e;
                    a + b + c + d + e
                });

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(15)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_pat_1() {
                let mut s = $name! {
                    let $($ref)+ ($($arg)+ a1, $($arg)+ a2) = always((1, 2)) => {
                        let a1: u32 = *a1;
                        let a2: u32 = *a2;
                        a1 + a2
                    }
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(3)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_pat_2() {
                let mut s = $name! {
                    let $($ref)+ ($($arg)+ a1, $($arg)+ a2) = always((1, 2)),
                    let $($ref)+ ($($arg)+ b1, $($arg)+ b2) = always((3, 4)) => {
                        let a1: u32 = *a1;
                        let a2: u32 = *a2;
                        let b1: u32 = *b1;
                        let b2: u32 = *b2;
                        a1 + a2 + b1 + b2
                    }
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(10)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_pat_3() {
                let mut s = $name! {
                    let $($ref)+ ($($arg)+ a1, $($arg)+ a2) = always((1, 2)),
                    let $($ref)+ ($($arg)+ b1, $($arg)+ b2) = always((3, 4)),
                    let $($ref)+ ($($arg)+ c1, $($arg)+ c2) = always((5, 6)) => {
                        let a1: u32 = *a1;
                        let a2: u32 = *a2;
                        let b1: u32 = *b1;
                        let b2: u32 = *b2;
                        let c1: u32 = *c1;
                        let c2: u32 = *c2;
                        a1 + a2 + b1 + b2 + c1 + c2
                    }
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(21)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_pat_4() {
                let mut s = $name! {
                    let $($ref)+ ($($arg)+ a1, $($arg)+ a2) = always((1, 2)),
                    let $($ref)+ ($($arg)+ b1, $($arg)+ b2) = always((3, 4)),
                    let $($ref)+ ($($arg)+ c1, $($arg)+ c2) = always((5, 6)),
                    let $($ref)+ ($($arg)+ d1, $($arg)+ d2) = always((7, 8)) => {
                        let a1: u32 = *a1;
                        let a2: u32 = *a2;
                        let b1: u32 = *b1;
                        let b2: u32 = *b2;
                        let c1: u32 = *c1;
                        let c2: u32 = *c2;
                        let d1: u32 = *d1;
                        let d2: u32 = *d2;
                        a1 + a2 + b1 + b2 + c1 + c2 + d1 + d2
                    }
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(36)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn let_pat_5() {
                let mut s = $name! {
                    let $($ref)+ ($($arg)+ a1, $($arg)+ a2) = always((1, 2)),
                    let $($ref)+ ($($arg)+ b1, $($arg)+ b2) = always((3, 4)),
                    let $($ref)+ ($($arg)+ c1, $($arg)+ c2) = always((5, 6)),
                    let $($ref)+ ($($arg)+ d1, $($arg)+ d2) = always((7, 8)),
                    let $($ref)+ ($($arg)+ e1, $($arg)+ e2) = always((9, 10)) => {
                        let a1: u32 = *a1;
                        let a2: u32 = *a2;
                        let b1: u32 = *b1;
                        let b2: u32 = *b2;
                        let c1: u32 = *c1;
                        let c2: u32 = *c2;
                        let d1: u32 = *d1;
                        let d2: u32 = *d2;
                        let e1: u32 = *e1;
                        let e2: u32 = *e2;
                        a1 + a2 + b1 + b2 + c1 + c2 + d1 + d2 + e1 + e2
                    }
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(55)));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn clone() {
                #[derive(PartialEq, Debug)]
                struct Cloner {
                    count: usize
                }

                impl Clone for Cloner {
                    fn clone(&self) -> Self {
                        Cloner { count: self.count + 1 }
                    }
                }

                let a2 = always(Cloner { count: 0 });
                let b2 = always(2);
                let c2 = always(3);
                let d2 = always(4);
                let e2 = always(Cloner { count: 0 });

                let mut s = $name! {
                    let $($ref)+ ($($arg)+ c1, $($arg)+ c2) = always((Cloner { count: 0 }, Cloner { count: 0 })),
                    let $($ref)+ ($($arg)+ c3, $($arg)+ c4) = always((Cloner { count: 0 }, Cloner { count: 0 })),
                    let a = a2,
                    let b = b2,
                    let c = c2,
                    let d = d2,
                    let e = e2,
                    let r = always(0),
                    let l = always(0),
                    let $($ref)+ ($($arg)+ c5, $($arg)+ c6) = always((Cloner { count: 0 }, Cloner { count: 0 })) =>
                    ((c1.clone(), c2.clone()), (c3.clone(), c4.clone()), *l, *r, a.clone(), *b, *c, *d, e.clone(), (c5.clone(), c6.clone()))
                };

                util::with_noop_context(|cx| {
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(Some(((Cloner { count: 1 }, Cloner { count: 1 }), (Cloner { count: 1 }, Cloner { count: 1 }), 0, 0, Cloner { count: 1 }, 2, 3, 4, Cloner { count: 1 }, (Cloner { count: 1 }, Cloner { count: 1 })))));
                    assert_eq!(s.poll_change_unpin(cx), Poll::Ready(None));
                });
            }

            #[test]
            fn foo() {
                use futures_signals::signal::{Signal, Mutable};

                let foo = Mutable::new(0);
                let bar = Mutable::new(1);
                let qux = Mutable::new(2);
                let corge = Mutable::new(3);

                let mut output = $name! {
                    let foo = foo.signal().map_future(|value| async move { value }),
                    let bar = bar.signal(),
                    let qux = qux.signal(),
                    let corge = corge.signal() => {
                        foo.unwrap_or(0) + *bar + *qux + *corge
                    }
                };

                /*
                use futures_signals::internal::{MapRef1, MapRefSignal};

                let mut output = {
                    let mut foo = MapRef1::new(foo.signal().map_future(|value| async move { value }));
                    let mut bar = MapRef1::new(bar.signal());
                    let mut qux = MapRef1::new(qux.signal());
                    let mut corge = MapRef1::new(corge.signal());

                    MapRefSignal::new(move |cx| {
                        let mut foo = foo.unsafe_pin();
                        let mut bar = bar.unsafe_pin();
                        let mut qux = qux.unsafe_pin();
                        let mut corge = corge.unsafe_pin();

                        let result = foo.as_mut().poll(cx)
                            .merge(bar.as_mut().poll(cx))
                            .merge(qux.as_mut().poll(cx))
                            .merge(corge.as_mut().poll(cx));

                        if result.changed {
                            let foo = foo.value_mut();
                            let bar = bar.value_mut();
                            let qux = qux.value_mut();
                            let corge = corge.value_mut();

                            Poll::Ready(Some({
                                foo.unwrap_or(0) + *bar + *qux + *corge
                            }))

                        } else if result.done {
                            Poll::Ready(None)

                        } else {
                            Poll::Pending
                        }
                    })
                };*/

                let mut output = unsafe { ::std::pin::Pin::new_unchecked(&mut output) };

                util::with_noop_context(|cx| {
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(6)));
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);

                    foo.set(11);
                    corge.set(2);

                    assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(16)));
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);

                    qux.set(22);

                    assert_eq!(output.as_mut().poll_change(cx), Poll::Ready(Some(36)));
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
                    assert_eq!(output.as_mut().poll_change(cx), Poll::Pending);
                });
            }
        }
    };
}

map_tests!(map_mut, (&mut), (ref mut));
map_tests!(map_ref, (&), (ref));
