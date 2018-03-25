#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_pairs {
    (let $name1:pat = $value1:expr; let $name2:pat = $value2:expr;) => {
        $crate::internal::MapPairMut::new($value1, $value2)
    };
    (let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::internal::MapPairMut::new($value, __internal_map_pairs!($($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_borrows {
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr;) => {
        {
            $($lets;)*
            let mut $r = ::std::cell::RefCell::borrow_mut(&(*$r).1);
            let $name = $crate::internal::unwrap_mut(&mut $r);
            $f
        }
    };
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr; $($args:tt)+) => {
        __internal_map_borrows!(
            $f,
            $r,
            {
                $($lets)*
                let mut $r = ::std::cell::RefCell::borrow_mut(&(*$r).1)
                let $r = $crate::internal::unwrap_mut(&mut $r)
                let mut l = ::std::cell::RefCell::borrow_mut(&(*$r).0)
                let $name = $crate::internal::unwrap_mut(&mut l)
            },
            $($args)+
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map {
    (($($move:tt)*), $f:expr, let $name:pat = $value:expr;) => {
        $crate::signal::Signal::map($value, $($move)* |mut x| {
            let $name = &mut x;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
    ) => {
        $crate::internal::Map2::new($value1, $value2, $($move)* |$name1, $name2| $f)
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
        $($args:tt)+
    ) => {
        $crate::internal::Map2::new(
            $value1,
            __internal_map_pairs!(let $name2 = $value2; $($args)+),
            $($move)* |$name1, r| __internal_map_borrows!(
                $f,
                r,
                {
                    let mut l = ::std::cell::RefCell::borrow_mut(&r.0)
                    let $name2 = $crate::internal::unwrap_mut(&mut l)
                },
                $($args)+
            )
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_lets {
    (($($move:tt)*), $f:expr, { $($lets:tt)* },) => {
        __internal_map!(($($move)*), $f, $($lets)*)
    };
    (($($move:tt)*), $f:expr, { $($lets:tt)* }, let $name:pat = $value:expr, $($args:tt)*) => {
        __internal_map_lets!(($($move)*), $f, { $($lets)* let $name = $value; }, $($args)*)
    };
    (($($move:tt)*), $f:expr, { $($lets:tt)* }, $name:ident, $($args:tt)*) => {
        __internal_map_lets!(($($move)*), $f, { $($lets)* let $name = $name; }, $($args)*)
    };
}

// TODO this is pretty inefficient, it iterates over the token tree one token at a time
#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_split {
    (($($before:tt)*), => move $f:expr) => {
        __internal_map_lets!((move), $f, {}, $($before)*,)
    };
    (($($before:tt)*), => $f:expr) => {
        __internal_map_lets!((), $f, {}, $($before)*,)
    };
    (($($before:tt)*), $t:tt $($after:tt)*) => {
        __internal_map_split!(($($before)* $t), $($after)*)
    };
}


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

#[macro_export]
macro_rules! map_mut {
    ($($input:tt)*) => { __internal_map_split!((), $($input)*) };
}
