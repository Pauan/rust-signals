#![recursion_limit="128"]

#[macro_use]
extern crate futures_signals;

use futures_signals::signal::{Signal, State, always};


#[test]
fn map_mut_ident_1() {
    let a = always(1);

    let mut s = map_mut!(a => {
        let a: u32 = *a;
        a + 1
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(2));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_ident_2() {
    let a = always(1);
    let b = always(2);

    let mut s = map_mut!(a, b => {
        let a: u32 = *a;
        let b: u32 = *b;
        a + b
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(3));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_ident_3() {
    let a = always(1);
    let b = always(2);
    let c = always(3);

    let mut s = map_mut!(a, b, c => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        a + b + c
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(6));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_ident_4() {
    let a = always(1);
    let b = always(2);
    let c = always(3);
    let d = always(4);

    let mut s = map_mut!(a, b, c, d => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        let d: u32 = *d;
        a + b + c + d
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(10));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_ident_5() {
    let a = always(1);
    let b = always(2);
    let c = always(3);
    let d = always(4);
    let e = always(5);

    let mut s = map_mut!(a, b, c, d, e => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        let d: u32 = *d;
        let e: u32 = *e;
        a + b + c + d + e
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(15));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}


#[test]
fn map_mut_let_1() {
    let a2 = always(1);

    let mut s = map_mut!(let a = a2 => {
        let a: u32 = *a;
        a + 1
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(2));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_2() {
    let a2 = always(1);
    let b2 = always(2);

    let mut s = map_mut!(let a = a2, let b = b2 => {
        let a: u32 = *a;
        let b: u32 = *b;
        a + b
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(3));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_3() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);

    let mut s = map_mut!(let a = a2, let b = b2, let c = c2 => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        a + b + c
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(6));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_4() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);
    let d2 = always(4);

    let mut s = map_mut!(let a = a2, let b = b2, let c = c2, let d = d2 => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        let d: u32 = *d;
        a + b + c + d
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(10));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_5() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);
    let d2 = always(4);
    let e2 = always(5);

    let mut s = map_mut!(let a = a2, let b = b2, let c = c2, let d = d2, let e = e2 => {
        let a: u32 = *a;
        let b: u32 = *b;
        let c: u32 = *c;
        let d: u32 = *d;
        let e: u32 = *e;
        a + b + c + d + e
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(15));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_pat_1() {
    let mut s = map_mut! {
        let &mut (ref mut a1, ref mut a2) = always((1, 2)) => {
            let a1: u32 = *a1;
            let a2: u32 = *a2;
            a1 + a2
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(3));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_pat_2() {
    let mut s = map_mut! {
        let &mut (ref mut a1, ref mut a2) = always((1, 2)),
        let &mut (ref mut b1, ref mut b2) = always((3, 4)) => {
            let a1: u32 = *a1;
            let a2: u32 = *a2;
            let b1: u32 = *b1;
            let b2: u32 = *b2;
            a1 + a2 + b1 + b2
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(10));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_pat_3() {
    let mut s = map_mut! {
        let &mut (ref mut a1, ref mut a2) = always((1, 2)),
        let &mut (ref mut b1, ref mut b2) = always((3, 4)),
        let &mut (ref mut c1, ref mut c2) = always((5, 6)) => {
            let a1: u32 = *a1;
            let a2: u32 = *a2;
            let b1: u32 = *b1;
            let b2: u32 = *b2;
            let c1: u32 = *c1;
            let c2: u32 = *c2;
            a1 + a2 + b1 + b2 + c1 + c2
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(21));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_pat_4() {
    let mut s = map_mut! {
        let &mut (ref mut a1, ref mut a2) = always((1, 2)),
        let &mut (ref mut b1, ref mut b2) = always((3, 4)),
        let &mut (ref mut c1, ref mut c2) = always((5, 6)),
        let &mut (ref mut d1, ref mut d2) = always((7, 8)) => {
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

    assert_eq!(Signal::poll(&mut s), State::Changed(36));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_let_pat_5() {
    let mut s = map_mut! {
        let &mut (ref mut a1, ref mut a2) = always((1, 2)),
        let &mut (ref mut b1, ref mut b2) = always((3, 4)),
        let &mut (ref mut c1, ref mut c2) = always((5, 6)),
        let &mut (ref mut d1, ref mut d2) = always((7, 8)),
        let &mut (ref mut e1, ref mut e2) = always((9, 10)) => {
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

    assert_eq!(Signal::poll(&mut s), State::Changed(55));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_mut_clone() {
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

    let mut s = map_mut! {
        let &mut (ref mut c1, ref mut c2) = always((Cloner { count: 0 }, Cloner { count: 0 })),
        let &mut (ref mut c3, ref mut c4) = always((Cloner { count: 0 }, Cloner { count: 0 })),
        let a = a2,
        let b = b2,
        let c = c2,
        let d = d2,
        let e = e2,
        let r = always(0),
        let l = always(0),
        let &mut (ref mut c5, ref mut c6) = always((Cloner { count: 0 }, Cloner { count: 0 })) =>
        ((c1.clone(), c2.clone()), (c3.clone(), c4.clone()), *l, *r, a.clone(), *b, *c, *d, e.clone(), (c5.clone(), c6.clone()))
    };

    /*let mut s = a2.map2(b2.map_pair(c2.map_pair(d2.map_pair(e2))), |a: &mut Cloner, r: &mut Pair<u32, Pair<u32, Pair<u32, Cloner>>>| {
        let a: &mut Cloner = a;

        let mut l = r.0.borrow_mut();
        let b: &mut u32 = l.as_mut().unwrap();

        let mut r = r.1.borrow_mut();
        let r = r.as_mut().unwrap();

        let mut l = r.0.borrow_mut();
        let c: &mut u32 = l.as_mut().unwrap();

        let mut r = r.1.borrow_mut();
        let r = r.as_mut().unwrap();

        let mut l = r.0.borrow_mut();
        let d: &mut u32 = l.as_mut().unwrap();

        let mut r = r.1.borrow_mut();
        let e: &mut Cloner = r.as_mut().unwrap();

        (a.clone(), *b, *c, *d, e.clone())
    });*/

    assert_eq!(Signal::poll(&mut s), State::Changed(((Cloner { count: 1 }, Cloner { count: 1 }), (Cloner { count: 1 }, Cloner { count: 1 }), 0, 0, Cloner { count: 1 }, 2, 3, 4, Cloner { count: 1 }, (Cloner { count: 1 }, Cloner { count: 1 }))));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}
