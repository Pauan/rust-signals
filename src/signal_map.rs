use crate::signal::Signal;
use std::pin::Pin;
use std::marker::Unpin;
use std::future::Future;
use std::task::{Poll, Context};
use futures_core::Stream;
use futures_util::stream;
use futures_util::stream::StreamExt;
use pin_project::pin_project;

// TODO make this non-exhaustive
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MapDiff<K, V> {
    Replace {
        entries: Vec<(K, V)>,
    },

    Insert {
        key: K,
        value: V,
    },

    Update {
        key: K,
        value: V,
    },

    Remove {
        key: K,
    },

    Clear {},
}

impl<K, A> MapDiff<K, A> {
    // TODO inline this ?
    fn map<B, F>(self, mut callback: F) -> MapDiff<K, B> where F: FnMut(A) -> B {
        match self {
            // TODO figure out a more efficient way of implementing this
            MapDiff::Replace { entries } => MapDiff::Replace { entries: entries.into_iter().map(|(k, v)| (k, callback(v))).collect() },
            MapDiff::Insert { key, value } => MapDiff::Insert { key, value: callback(value) },
            MapDiff::Update { key, value } => MapDiff::Update { key, value: callback(value) },
            MapDiff::Remove { key } => MapDiff::Remove { key },
            MapDiff::Clear {} => MapDiff::Clear {},
        }
    }
}


// TODO impl for AssertUnwindSafe ?
#[must_use = "SignalMaps do nothing unless polled"]
pub trait SignalMap {
    type Key;
    type Value;

    fn poll_map_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>>;
}


// Copied from Future in the Rust stdlib
impl<'a, A> SignalMap for &'a mut A where A: ?Sized + SignalMap + Unpin {
    type Key = A::Key;
    type Value = A::Value;

    #[inline]
    fn poll_map_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> {
        A::poll_map_change(Pin::new(&mut **self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalMap for Box<A> where A: ?Sized + SignalMap + Unpin {
    type Key = A::Key;
    type Value = A::Value;

    #[inline]
    fn poll_map_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> {
        A::poll_map_change(Pin::new(&mut *self), cx)
    }
}

// Copied from Future in the Rust stdlib
impl<A> SignalMap for Pin<A>
    where A: Unpin + ::std::ops::DerefMut,
          A::Target: SignalMap {
    type Key = <<A as ::std::ops::Deref>::Target as SignalMap>::Key;
    type Value = <<A as ::std::ops::Deref>::Target as SignalMap>::Value;

    #[inline]
    fn poll_map_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> {
        Pin::get_mut(self).as_mut().poll_map_change(cx)
    }
}


// TODO Seal this
pub trait SignalMapExt: SignalMap {
    #[inline]
    fn map_value<A, F>(self, callback: F) -> MapValue<Self, F>
        where F: FnMut(Self::Value) -> A,
              Self: Sized {
        MapValue {
            signal: self,
            callback,
        }
    }

    /// Returns a signal that tracks the value of a particular key in the map.
    #[inline]
    fn key_cloned(self, key: Self::Key) -> MapWatchKeySignal<Self>
        where Self::Key: PartialEq,
              Self::Value: Clone,
              Self: Sized {
        MapWatchKeySignal {
            signal_map: self,
            watch_key: key,
            first: true,
        }
    }

    #[inline]
    fn for_each<U, F>(self, callback: F) -> ForEach<Self, U, F>
        where U: Future<Output = ()>,
              F: FnMut(MapDiff<Self::Key, Self::Value>) -> U,
              Self: Sized {
        // TODO a little hacky
        ForEach {
            inner: SignalMapStream {
                signal_map: self,
            }.for_each(callback)
        }
    }

    /// A convenience for calling `SignalMap::poll_map_change` on `Unpin` types.
    #[inline]
    fn poll_map_change_unpin(&mut self, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> where Self: Unpin + Sized {
        Pin::new(self).poll_map_change(cx)
    }

    #[inline]
    fn boxed<'a>(self) -> Pin<Box<dyn SignalMap<Key = Self::Key, Value = Self::Value> + Send + 'a>>
        where Self: Sized + Send + 'a {
        Box::pin(self)
    }

    #[inline]
    fn boxed_local<'a>(self) -> Pin<Box<dyn SignalMap<Key = Self::Key, Value = Self::Value> + 'a>>
        where Self: Sized + 'a {
        Box::pin(self)
    }
}

// TODO why is this ?Sized
impl<T: ?Sized> SignalMapExt for T where T: SignalMap {}


/// An owned dynamically typed [`SignalMap`].
///
/// This is useful if you don't know the static type, or if you need
/// indirection.
pub type BoxSignalMap<'a, Key, Value> = Pin<Box<dyn SignalMap<Key = Key, Value = Value> + Send + 'a>>;

/// Same as [`BoxSignalMap`], but without the `Send` requirement.
pub type LocalBoxSignalMap<'a, Key, Value> = Pin<Box<dyn SignalMap<Key = Key, Value = Value> + 'a>>;


#[pin_project(project = MapValueProj)]
#[derive(Debug)]
#[must_use = "SignalMaps do nothing unless polled"]
pub struct MapValue<A, B> {
    #[pin]
    signal: A,
    callback: B,
}

impl<A, B, F> SignalMap for MapValue<A, F>
    where A: SignalMap,
          F: FnMut(A::Value) -> B {
    type Key = A::Key;
    type Value = B;

    // TODO should this inline ?
    #[inline]
    fn poll_map_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> {
        let MapValueProj { signal, callback } = self.project();

        signal.poll_map_change(cx).map(|some| some.map(|change| change.map(|value| callback(value))))
    }
}

#[pin_project(project = MapWatchKeySignalProj)]
#[derive(Debug)]
pub struct MapWatchKeySignal<M> where M: SignalMap {
    #[pin]
    signal_map: M,
    watch_key: M::Key,
    first: bool,
}

impl<M> Signal for MapWatchKeySignal<M>
    where M: SignalMap,
          M::Key: PartialEq,
          M::Value: Clone {
    type Item = Option<M::Value>;

    fn poll_change(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let MapWatchKeySignalProj { mut signal_map, watch_key, first } = self.project();

        let mut changed: Option<Option<M::Value>> = None;

        let is_done = loop {
            break match signal_map.as_mut().poll_map_change(cx) {
                Poll::Ready(some) => match some {
                    Some(MapDiff::Replace { entries }) => {
                        changed = Some(
                            entries
                                .into_iter()
                                .find(|entry| entry.0 == *watch_key)
                                .map(|entry| entry.1)
                        );
                        continue;
                    },
                    Some(MapDiff::Insert { key, value }) | Some(MapDiff::Update { key, value }) => {
                        if key == *watch_key {
                            changed = Some(Some(value));
                        }
                        continue;
                    },
                    Some(MapDiff::Remove { key }) => {
                        if key == *watch_key {
                            changed = Some(None);
                        }
                        continue;
                    },
                    Some(MapDiff::Clear {}) => {
                        changed = Some(None);
                        continue;
                    },
                    None => {
                        true
                    },
                },
                Poll::Pending => {
                    false
                },
            }
        };

        if let Some(change) = changed {
            *first = false;
            Poll::Ready(Some(change))

        } else if *first {
            *first = false;
            Poll::Ready(Some(None))

        } else if is_done {
            Poll::Ready(None)

        } else {
            Poll::Pending
        }
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Streams do nothing unless polled"]
struct SignalMapStream<A> {
    #[pin]
    signal_map: A,
}

impl<A: SignalMap> Stream for SignalMapStream<A> {
    type Item = MapDiff<A::Key, A::Value>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.project().signal_map.poll_map_change(cx)
    }
}


#[pin_project]
#[derive(Debug)]
#[must_use = "Futures do nothing unless polled"]
pub struct ForEach<A, B, C> {
    #[pin]
    inner: stream::ForEach<SignalMapStream<A>, B, C>,
}

impl<A, B, C> Future for ForEach<A, B, C>
    where A: SignalMap,
          B: Future<Output = ()>,
          C: FnMut(MapDiff<A::Key, A::Value>) -> B {
    type Output = ();

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}


// TODO verify that this is correct
mod mutable_btree_map {
    use super::{SignalMap, SignalMapExt, MapDiff};
    use std::pin::Pin;
    use std::marker::Unpin;
    use std::fmt;
    use std::ops::{Deref, Index};
    use std::cmp::{Ord, Ordering};
    use std::hash::{Hash, Hasher};
    use std::collections::BTreeMap;
    use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
    use std::task::{Poll, Context};
    use futures_channel::mpsc;
    use futures_util::stream::StreamExt;
    use crate::signal_vec::{SignalVec, VecDiff};


    #[derive(Debug)]
    struct MutableBTreeState<K, V> {
        values: BTreeMap<K, V>,
        senders: Vec<mpsc::UnboundedSender<MapDiff<K, V>>>,
    }

    impl<K: Ord, V> MutableBTreeState<K, V> {
        // TODO should this inline ?
        #[inline]
        fn notify<B: FnMut() -> MapDiff<K, V>>(&mut self, mut change: B) {
            self.senders.retain(|sender| {
                sender.unbounded_send(change()).is_ok()
            });
        }

        // If there is only 1 sender then it won't clone at all.
        // If there is more than 1 sender then it will clone N-1 times.
        // TODO verify that this works correctly
        #[inline]
        fn notify_clone<A, F>(&mut self, value: Option<A>, mut f: F) where A: Clone, F: FnMut(A) -> MapDiff<K, V> {
            if let Some(value) = value {
                let mut len = self.senders.len();

                if len > 0 {
                    let mut copy = Some(value);

                    self.senders.retain(move |sender| {
                        let value = copy.take().unwrap();

                        len -= 1;

                        // This isn't the last element
                        if len != 0 {
                            copy = Some(value.clone());
                        }

                        sender.unbounded_send(f(value)).is_ok()
                    });
                }
            }
        }

        #[inline]
        fn change<A, F>(&self, f: F) -> Option<A> where F: FnOnce() -> A {
            if self.senders.is_empty() {
                None

            } else {
                Some(f())
            }
        }

        fn clear(&mut self) {
            if !self.values.is_empty() {
                self.values.clear();

                self.notify(|| MapDiff::Clear {});
            }
        }
    }

    impl<K: Ord + Clone, V> MutableBTreeState<K, V> {
        fn remove(&mut self, key: &K) -> Option<V> {
            let value = self.values.remove(key)?;

            let key = self.change(|| key.clone());
            self.notify_clone(key, |key| MapDiff::Remove { key });

            Some(value)
        }
    }

    impl<K: Clone + Ord, V: Clone> MutableBTreeState<K, V> {
        fn entries_cloned(values: &BTreeMap<K, V>) -> Vec<(K, V)> {
            values.into_iter().map(|(k, v)| {
                (k.clone(), v.clone())
            }).collect()
        }

        fn replace_cloned(&mut self, values: BTreeMap<K, V>) {
            let entries = self.change(|| Self::entries_cloned(&values));

            self.values = values;

            self.notify_clone(entries, |entries| MapDiff::Replace { entries });
        }

        fn insert_cloned(&mut self, key: K, value: V) -> Option<V> {
            let x = self.change(|| (key.clone(), value.clone()));

            if let Some(value) = self.values.insert(key, value) {
                self.notify_clone(x, |(key, value)| MapDiff::Update { key, value });
                Some(value)

            } else {
                self.notify_clone(x, |(key, value)| MapDiff::Insert { key, value });
                None
            }
        }

        fn signal_map_cloned(&mut self) -> MutableSignalMap<K, V> {
            let (sender, receiver) = mpsc::unbounded();

            if !self.values.is_empty() {
                sender.unbounded_send(MapDiff::Replace {
                    entries: Self::entries_cloned(&self.values),
                }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalMap {
                receiver
            }
        }
    }

    impl<K: Copy + Ord, V: Copy> MutableBTreeState<K, V> {
        fn entries(values: &BTreeMap<K, V>) -> Vec<(K, V)> {
            values.into_iter().map(|(k, v)| {
                (*k, *v)
            }).collect()
        }

        fn replace(&mut self, values: BTreeMap<K, V>) {
            let entries = self.change(|| Self::entries(&values));

            self.values = values;

            self.notify_clone(entries, |entries| MapDiff::Replace { entries });
        }

        fn insert(&mut self, key: K, value: V) -> Option<V> {
            if let Some(old_value) = self.values.insert(key, value) {
                self.notify(|| MapDiff::Update { key, value });
                Some(old_value)

            } else {
                self.notify(|| MapDiff::Insert { key, value });
                None
            }
        }

        fn signal_map(&mut self) -> MutableSignalMap<K, V> {
            let (sender, receiver) = mpsc::unbounded();

            if !self.values.is_empty() {
                sender.unbounded_send(MapDiff::Replace {
                    entries: Self::entries(&self.values),
                }).unwrap();
            }

            self.senders.push(sender);

            MutableSignalMap {
                receiver
            }
        }
    }


    macro_rules! make_shared {
        ($t:ty) => {
            impl<'a, K, V> PartialEq<BTreeMap<K, V>> for $t where K: PartialEq<K>, V: PartialEq<V> {
                #[inline] fn eq(&self, other: &BTreeMap<K, V>) -> bool { **self == *other }
                #[inline] fn ne(&self, other: &BTreeMap<K, V>) -> bool { **self != *other }
            }

            impl<'a, K, V> PartialEq<$t> for $t where K: PartialEq<K>, V: PartialEq<V> {
                #[inline] fn eq(&self, other: &$t) -> bool { *self == **other }
                #[inline] fn ne(&self, other: &$t) -> bool { *self != **other }
            }

            impl<'a, K, V> Eq for $t where K: Eq, V: Eq {}

            impl<'a, K, V> PartialOrd<BTreeMap<K, V>> for $t where K: PartialOrd<K>, V: PartialOrd<V> {
                #[inline]
                fn partial_cmp(&self, other: &BTreeMap<K, V>) -> Option<Ordering> {
                    PartialOrd::partial_cmp(&**self, &*other)
                }
            }

            impl<'a, K, V> PartialOrd<$t> for $t where K: PartialOrd<K>, V: PartialOrd<V> {
                #[inline]
                fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                    PartialOrd::partial_cmp(&**self, &**other)
                }
            }

            impl<'a, K, V> Ord for $t where K: Ord, V: Ord {
                #[inline]
                fn cmp(&self, other: &Self) -> Ordering {
                    Ord::cmp(&**self, &**other)
                }
            }

            impl<'a, 'b, K, V> Index<&'b K> for $t where K: Ord {
                type Output = V;

                #[inline]
                fn index(&self, key: &'b K) -> &Self::Output {
                    Index::index(&**self, key)
                }
            }

            impl<'a, K, V> Deref for $t {
                type Target = BTreeMap<K, V>;

                #[inline]
                fn deref(&self) -> &Self::Target {
                    &self.lock.values
                }
            }

            impl<'a, K, V> Hash for $t where K: Hash, V: Hash {
                #[inline]
                fn hash<H>(&self, state: &mut H) where H: Hasher {
                    Hash::hash(&**self, state)
                }
            }
        };
    }


    #[derive(Debug)]
    pub struct MutableBTreeMapLockRef<'a, K, V> where K: 'a, V: 'a {
        lock: RwLockReadGuard<'a, MutableBTreeState<K, V>>,
    }

    make_shared!(MutableBTreeMapLockRef<'a, K, V>);


    #[derive(Debug)]
    pub struct MutableBTreeMapLockMut<'a, K, V> where K: 'a, V: 'a {
        lock: RwLockWriteGuard<'a, MutableBTreeState<K, V>>,
    }

    make_shared!(MutableBTreeMapLockMut<'a, K, V>);

    impl<'a, K, V> MutableBTreeMapLockMut<'a, K, V> where K: Ord {
        #[inline]
        pub fn clear(&mut self) {
            self.lock.clear()
        }
    }

    impl<'a, K, V> MutableBTreeMapLockMut<'a, K, V> where K: Ord + Clone {
        #[inline]
        pub fn remove(&mut self, key: &K) -> Option<V> {
            self.lock.remove(key)
        }
    }

    impl<'a, K, V> MutableBTreeMapLockMut<'a, K, V> where K: Ord + Clone, V: Clone {
        #[inline]
        pub fn replace_cloned(&mut self, values: BTreeMap<K, V>) {
            self.lock.replace_cloned(values)
        }

        #[inline]
        pub fn insert_cloned(&mut self, key: K, value: V) -> Option<V> {
            self.lock.insert_cloned(key, value)
        }
    }

    impl<'a, K, V> MutableBTreeMapLockMut<'a, K, V> where K: Ord + Copy, V: Copy {
        #[inline]
        pub fn replace(&mut self, values: BTreeMap<K, V>) {
            self.lock.replace(values)
        }

        #[inline]
        pub fn insert(&mut self, key: K, value: V) -> Option<V> {
            self.lock.insert(key, value)
        }
    }


    // TODO get rid of the Arc
    // TODO impl some of the same traits as BTreeMap
    pub struct MutableBTreeMap<K, V>(Arc<RwLock<MutableBTreeState<K, V>>>);

    impl<K, V> MutableBTreeMap<K, V> {
        // TODO deprecate this and replace with From ?
        #[inline]
        pub fn with_values(values: BTreeMap<K, V>) -> Self {
            Self(Arc::new(RwLock::new(MutableBTreeState {
                values,
                senders: vec![],
            })))
        }

        // TODO return Result ?
        #[inline]
        pub fn lock_ref(&self) -> MutableBTreeMapLockRef<K, V> {
            MutableBTreeMapLockRef {
                lock: self.0.read().unwrap(),
            }
        }

        // TODO return Result ?
        #[inline]
        pub fn lock_mut(&self) -> MutableBTreeMapLockMut<K, V> {
            MutableBTreeMapLockMut {
                lock: self.0.write().unwrap(),
            }
        }
    }

    impl<K, V> MutableBTreeMap<K, V> where K: Ord {
        #[inline]
        pub fn new() -> Self {
            Self::with_values(BTreeMap::new())
        }
    }

    impl<K, V> MutableBTreeMap<K, V> where K: Ord + Clone, V: Clone {
        #[inline]
        pub fn signal_map_cloned(&self) -> MutableSignalMap<K, V> {
            self.0.write().unwrap().signal_map_cloned()
        }

        // TODO deprecate and rename to keys_cloned
        // TODO replace with MutableBTreeMapKeysCloned
        #[inline]
        pub fn signal_vec_keys(&self) -> MutableBTreeMapKeys<K, V> {
            MutableBTreeMapKeys {
                signal: self.signal_map_cloned(),
                keys: vec![],
            }
        }

        // TODO replace with MutableBTreeMapEntriesCloned
        #[inline]
        pub fn entries_cloned(&self) -> MutableBTreeMapEntries<K, V> {
            MutableBTreeMapEntries {
                signal: self.signal_map_cloned(),
                keys: vec![],
            }
        }
    }

    impl<K, V> MutableBTreeMap<K, V> where K: Ord + Copy, V: Copy {
        #[inline]
        pub fn signal_map(&self) -> MutableSignalMap<K, V> {
            self.0.write().unwrap().signal_map()
        }

        // TODO deprecate and rename to entries
        #[inline]
        pub fn signal_vec_entries(&self) -> MutableBTreeMapEntries<K, V> {
            MutableBTreeMapEntries {
                signal: self.signal_map(),
                keys: vec![],
            }
        }
    }

    impl<K, V> fmt::Debug for MutableBTreeMap<K, V> where K: fmt::Debug, V: fmt::Debug {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            let state = self.0.read().unwrap();

            fmt.debug_tuple("MutableBTreeMap")
                .field(&state.values)
                .finish()
        }
    }

    #[cfg(feature = "serde")]
    impl<K, V> serde::Serialize for MutableBTreeMap<K, V> where BTreeMap<K, V>: serde::Serialize {
        #[inline]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
            self.0.read().unwrap().values.serialize(serializer)
        }
    }

    #[cfg(feature = "serde")]
    impl<'de, K, V> serde::Deserialize<'de> for MutableBTreeMap<K, V> where BTreeMap<K, V>: serde::Deserialize<'de> {
        #[inline]
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: serde::Deserializer<'de> {
            <BTreeMap<K, V>>::deserialize(deserializer).map(MutableBTreeMap::with_values)
        }
    }

    impl<K, V> Default for MutableBTreeMap<K, V> where K: Ord {
        #[inline]
        fn default() -> Self {
            MutableBTreeMap::new()
        }
    }

    impl<K, V> Clone for MutableBTreeMap<K, V> {
        #[inline]
        fn clone(&self) -> Self {
            MutableBTreeMap(self.0.clone())
        }
    }

    #[derive(Debug)]
    #[must_use = "SignalMaps do nothing unless polled"]
    pub struct MutableSignalMap<K, V> {
        receiver: mpsc::UnboundedReceiver<MapDiff<K, V>>,
    }

    impl<K, V> Unpin for MutableSignalMap<K, V> {}

    impl<K, V> SignalMap for MutableSignalMap<K, V> {
        type Key = K;
        type Value = V;

        #[inline]
        fn poll_map_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<MapDiff<Self::Key, Self::Value>>> {
            self.receiver.poll_next_unpin(cx)
        }
    }


    #[derive(Debug)]
    #[must_use = "SignalVecs do nothing unless polled"]
    pub struct MutableBTreeMapKeys<K, V> {
        signal: MutableSignalMap<K, V>,
        keys: Vec<K>,
    }

    impl<K, V> Unpin for MutableBTreeMapKeys<K, V> {}

    impl<K, V> SignalVec for MutableBTreeMapKeys<K, V> where K: Ord + Clone {
        type Item = K;

        #[inline]
        fn poll_vec_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
            loop {
                return match self.signal.poll_map_change_unpin(cx) {
                    Poll::Ready(Some(diff)) => match diff {
                        MapDiff::Replace { entries } => {
                            // TODO verify that it is in sorted order ?
                            self.keys = entries.into_iter().map(|(k, _)| k).collect();
                            Poll::Ready(Some(VecDiff::Replace { values: self.keys.clone() }))
                        },
                        MapDiff::Insert { key, value: _ } => {
                            let index = self.keys.binary_search(&key).unwrap_err();
                            self.keys.insert(index, key.clone());
                            Poll::Ready(Some(VecDiff::InsertAt { index, value: key }))
                        },
                        MapDiff::Update { .. } => {
                            continue;
                        },
                        MapDiff::Remove { key } => {
                            let index = self.keys.binary_search(&key).unwrap();
                            self.keys.remove(index);
                            Poll::Ready(Some(VecDiff::RemoveAt { index }))
                        },
                        MapDiff::Clear {} => {
                            self.keys.clear();
                            Poll::Ready(Some(VecDiff::Clear {}))
                        },
                    },
                    Poll::Ready(None) => Poll::Ready(None),
                    Poll::Pending => Poll::Pending,
                };
            }
        }
    }


    #[derive(Debug)]
    #[must_use = "SignalVecs do nothing unless polled"]
    pub struct MutableBTreeMapEntries<K, V> {
        signal: MutableSignalMap<K, V>,
        keys: Vec<K>,
    }

    impl<K, V> Unpin for MutableBTreeMapEntries<K, V> {}

    impl<K, V> SignalVec for MutableBTreeMapEntries<K, V> where K: Ord + Clone {
        type Item = (K, V);

        #[inline]
        fn poll_vec_change(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<VecDiff<Self::Item>>> {
            self.signal.poll_map_change_unpin(cx).map(|opt| opt.map(|diff| {
                match diff {
                    MapDiff::Replace { entries } => {
                        // TODO verify that it is in sorted order ?
                        self.keys = entries.iter().map(|(k, _)| k.clone()).collect();
                        VecDiff::Replace { values: entries }
                    },
                    MapDiff::Insert { key, value } => {
                        let index = self.keys.binary_search(&key).unwrap_err();
                        self.keys.insert(index, key.clone());
                        VecDiff::InsertAt { index, value: (key, value) }
                    },
                    MapDiff::Update { key, value } => {
                        let index = self.keys.binary_search(&key).unwrap();
                        VecDiff::UpdateAt { index, value: (key, value) }
                    },
                    MapDiff::Remove { key } => {
                        let index = self.keys.binary_search(&key).unwrap();
                        self.keys.remove(index);
                        VecDiff::RemoveAt { index }
                    },
                    MapDiff::Clear {} => {
                        self.keys.clear();
                        VecDiff::Clear {}
                    },
                }
            }))
        }
    }
}

pub use self::mutable_btree_map::*;
