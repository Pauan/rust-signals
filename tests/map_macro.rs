#![recursion_limit="128"]

extern crate futures;

#[macro_use]
extern crate futures_signals;


#[macro_export]
macro_rules! map_tests {
    ($name:ident, ($($ref:tt)+), ($($arg:tt)+)) => {
        #[cfg(test)]
        mod $name {
            use futures::Async;
            use futures_signals::signal::{Signal, always};

            #[test]
            fn ident_1() {
                let a = always(1);

                let mut s = $name!(a => {
                    let a: u32 = *a;
                    a + 1
                });

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(2)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(3)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(6)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(10)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(15)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
            }


            #[test]
            fn let_1() {
                let a2 = always(1);

                let mut s = $name!(let a = a2 => {
                    let a: u32 = *a;
                    a + 1
                });

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(2)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(3)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(6)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(10)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(15)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(3)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(10)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(21)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(36)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(55)));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
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

                assert_eq!(Signal::poll(&mut s), Async::Ready(Some(((Cloner { count: 1 }, Cloner { count: 1 }), (Cloner { count: 1 }, Cloner { count: 1 }), 0, 0, Cloner { count: 1 }, 2, 3, 4, Cloner { count: 1 }, (Cloner { count: 1 }, Cloner { count: 1 })))));
                assert_eq!(Signal::poll(&mut s), Async::Ready(None));
            }
        }
    };
}

map_tests!(map_mut, (&mut), (ref mut));
map_tests!(map_ref, (&), (ref));
