#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut_pairs {
    (let $name1:pat = $value1:expr; let $name2:pat = $value2:expr;) => {
        $crate::internal::MapPairMut::new($value1, $value2)
    };
    (let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::internal::MapPairMut::new($value, __internal_map_mut_pairs!($($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut_borrows {
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr;) => {
        {
            $($lets;)*
            // TODO is this correct ?
            let mut $r = $crate::internal::lock_mut(&(*$r).1);
            let $name = $crate::internal::unwrap_mut(&mut $r);
            $f
        }
    };
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr; $($args:tt)+) => {
        __internal_map_mut_borrows!(
            $f,
            $r,
            {
                $($lets)*
                // TODO is this correct ?
                let mut $r = $crate::internal::lock_mut(&(*$r).1)
                let $r = $crate::internal::unwrap_mut(&mut $r)
                // TODO is this correct ?
                let mut l = $crate::internal::lock_mut(&(*$r).0)
                let $name = $crate::internal::unwrap_mut(&mut l)
            },
            $($args)+
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_mut {
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
            __internal_map_mut_pairs!(let $name2 = $value2; $($args)+),
            $($move)* |$name1, r| __internal_map_mut_borrows!(
                $f,
                r,
                {
                    // TODO is this correct ?
                    let mut l = $crate::internal::lock_mut(&r.0)
                    let $name2 = $crate::internal::unwrap_mut(&mut l)
                },
                $($args)+
            )
        )
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
    ($($input:tt)*) => { __internal_map_split!(__internal_map_mut, (), $($input)*) };
}


#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref_pairs {
    (let $name1:pat = $value1:expr; let $name2:pat = $value2:expr;) => {
        $crate::internal::MapPair::new($value1, $value2)
    };
    (let $name:pat = $value:expr; $($args:tt)+) => {
        $crate::internal::MapPair::new($value, __internal_map_ref_pairs!($($args)+))
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref_borrows {
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr;) => {
        {
            $($lets;)*
            let $name = $crate::internal::unwrap_ref(&(*$r).1);
            $f
        }
    };
    ($f:expr, $r:ident, { $($lets:stmt)* }, let $name:pat = $value:expr; $($args:tt)+) => {
        __internal_map_ref_borrows!(
            $f,
            $r,
            {
                $($lets)*
                // TODO is this correct ?
                let $r = $crate::internal::lock_ref(&$crate::internal::unwrap_ref(&(*$r).1))
                let $name = $crate::internal::unwrap_ref(&(*$r).0)
            },
            $($args)+
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_ref {
    (($($move:tt)*), $f:expr, let $name:pat = $value:expr;) => {
        $crate::signal::Signal::map($value, $($move)* |x| {
            let $name = &x;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
    ) => {
        $crate::internal::Map2::new($value1, $value2, $($move)* |x, y| {
            // TODO is there a better way of converting from &mut to & ?
            let $name1 = &*x;
            let $name2 = &*y;
            $f
        })
    };
    (($($move:tt)*), $f:expr,
        let $name1:pat = $value1:expr;
        let $name2:pat = $value2:expr;
        $($args:tt)+
    ) => {
        $crate::internal::Map2::new(
            $value1,
            __internal_map_ref_pairs!(let $name2 = $value2; $($args)+),
            $($move)* |l, r| __internal_map_ref_borrows!(
                $f,
                r,
                {
                    // TODO is there a better way of converting from &mut to & ?
                    let $name1 = &*l
                    // TODO is this correct ?
                    let r = $crate::internal::lock_ref(&r)
                    let $name2 = $crate::internal::unwrap_ref(&r.0)
                },
                $($args)+
            )
        )
    };
}

/*let mut s = a2.map2(b2.map_pair(c2.map_pair(d2.map_pair(e2))), |a, r| {
    let a: &Cloner = a;

    let r = r.borrow();
    let b: &u32 = r.0.as_ref().unwrap();

    let r = r.1.as_ref().unwrap().borrow();
    let c: &u32 = r.0.as_ref().unwrap();

    let r = r.1.as_ref().unwrap().borrow();
    let d: &u32 = r.0.as_ref().unwrap();

    let e: &Cloner = r.1.as_ref().unwrap();

    (a.clone(), *b, *c, *d, e.clone())
});*/

#[macro_export]
macro_rules! map_ref {
    ($($input:tt)*) => { __internal_map_split!(__internal_map_ref, (), $($input)*) };
}


#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_lets {
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* },) => {
        $macro!(($($move)*), $f, $($lets)*)
    };
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* }, let $name:pat = $value:expr, $($args:tt)*) => {
        __internal_map_lets!($macro, ($($move)*), $f, { $($lets)* let $name = $value; }, $($args)*)
    };
    ($macro:ident, ($($move:tt)*), $f:expr, { $($lets:tt)* }, $name:ident, $($args:tt)*) => {
        __internal_map_lets!($macro, ($($move)*), $f, { $($lets)* let $name = $name; }, $($args)*)
    };
}

// TODO this is pretty inefficient, it iterates over the token tree one token at a time
#[doc(hidden)]
#[macro_export]
macro_rules! __internal_map_split {
    ($macro:ident, ($($before:tt)*), => move $f:expr) => {
        __internal_map_lets!($macro, (move), $f, {}, $($before)*,)
    };
    ($macro:ident, ($($before:tt)*), => $f:expr) => {
        __internal_map_lets!($macro, (), $f, {}, $($before)*,)
    };
    ($macro:ident, ($($before:tt)*), $t:tt $($after:tt)*) => {
        __internal_map_split!($macro, ($($before)* $t), $($after)*)
    };
}
