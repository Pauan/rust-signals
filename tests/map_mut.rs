#[macro_use]
extern crate futures_signals;

use futures_signals::signal::{Signal, State, always};


#[test]
fn map_macro_ident_1() {
    let a = always(1);

    let mut s = map_mut!(a => {
        let a: u32 = *a;
        a + 1
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(2));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_ident_2() {
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
fn map_macro_ident_3() {
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
fn map_macro_ident_4() {
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
fn map_macro_ident_5() {
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
fn map_macro_let_1() {
    let a2 = always(1);

    let mut s = map_mut!(let a = a2 => {
        let a: u32 = *a;
        a + 1
    });

    assert_eq!(Signal::poll(&mut s), State::Changed(2));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_let_2() {
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
fn map_macro_let_3() {
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
fn map_macro_let_4() {
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
fn map_macro_let_5() {
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
fn map_macro_let_type_1() {
    let a2 = always(1);

    let mut s = map_mut! {
        let a: &mut u32 = a2 => {
            let a: u32 = *a;
            a + 1
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(2));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_let_type_2() {
    let a2 = always(1);
    let b2 = always(2);

    let mut s = map_mut! {
        let a: &mut u32 = a2,
        let b: &mut u32 = b2 => {
            let a: u32 = *a;
            let b: u32 = *b;
            a + b
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(3));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_let_type_3() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);

    let mut s = map_mut! {
        let a: &mut u32 = a2,
        let b: &mut u32 = b2,
        let c: &mut u32 = c2 => {
            let a: u32 = *a;
            let b: u32 = *b;
            let c: u32 = *c;
            a + b + c
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(6));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_let_type_4() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);
    let d2 = always(4);

    let mut s = map_mut! {
        let a: &mut u32 = a2,
        let b: &mut u32 = b2,
        let c: &mut u32 = c2,
        let d: &mut u32 = d2 => {
            let a: u32 = *a;
            let b: u32 = *b;
            let c: u32 = *c;
            let d: u32 = *d;
            a + b + c + d
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(10));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro_let_type_5() {
    let a2 = always(1);
    let b2 = always(2);
    let c2 = always(3);
    let d2 = always(4);
    let e2 = always(5);

    let mut s = map_mut! {
        let a: &mut u32 = a2,
        let b: &mut u32 = b2,
        let c: &mut u32 = c2,
        let d: &mut u32 = d2,
        let e: &mut u32 = e2 => {
            let a: u32 = *a;
            let b: u32 = *b;
            let c: u32 = *c;
            let d: u32 = *d;
            let e: u32 = *e;
            a + b + c + d + e
        }
    };

    assert_eq!(Signal::poll(&mut s), State::Changed(15));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}

#[test]
fn map_macro() {
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
        let a = a2,
        let b = b2,
        let c = c2,
        let d = d2,
        let e = e2,
        let r = always(0),
        let l = always(0) =>
        (*l, *r, a.clone(), *b, *c, *d, e.clone())
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

    assert_eq!(Signal::poll(&mut s), State::Changed((0, 0, Cloner { count: 1 }, 2, 3, 4, Cloner { count: 1 })));
    assert_eq!(Signal::poll(&mut s), State::NotChanged);
}
