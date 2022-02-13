use std::sync::atomic::{AtomicPtr, Ordering};

/*#[derive(Debug)]
pub(crate) struct Atomic<A> {
    // TODO only box if the value is too big
    ptr: AtomicPtr<A>,
}

impl<A> Atomic<A> {
    pub(crate) fn new(value: A) -> Self {
        Self {
            ptr: AtomicPtr::new(Box::into_raw(Box::new(value))),
        }
    }

    fn ptr_swap(&self, value: A) -> *mut A {
        let new_ptr = Box::into_raw(Box::new(value));
        self.ptr.swap(new_ptr, Ordering::AcqRel)
    }

    pub(crate) fn swap(&self, value: A) -> A {
        let ptr = self.ptr_swap(value);
        unsafe { *Box::from_raw(ptr) }
    }

    #[inline]
    pub(crate) fn store(&self, value: A) {
        // TODO make this more efficient by using drop_in_place ?
        drop(self.swap(value));
    }
}

impl<A> Atomic<A> where A: Default {
    #[inline]
    pub(crate) fn take(&self) -> A {
        self.swap(Default::default())
    }
}

impl<A> Drop for Atomic<A> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::Acquire);

        // TODO verify the safety of this
        unsafe { drop(Box::from_raw(ptr)); }
    }
}*/

/// The same as `Atomic<Option<A>>` except faster and uses less memory.
///
/// This is because it represents `None` as a null pointer, which avoids boxing.
#[derive(Debug)]
pub(crate) struct AtomicOption<A> {
    ptr: AtomicPtr<A>,
}

impl<A> AtomicOption<A> {
    fn to_ptr(value: Option<A>) -> *mut A {
        match value {
            // TODO only box if the value is too big
            Some(value) => Box::into_raw(Box::new(value)),
            None => std::ptr::null_mut(),
        }
    }

    fn from_ptr(ptr: *mut A) -> Option<A> {
        if ptr.is_null() {
            None
        } else {
            // This is safe because we only do this for pointers created with `Box::into_raw`
            unsafe { Some(*Box::from_raw(ptr)) }
        }
    }

    #[inline]
    pub(crate) fn new(value: Option<A>) -> Self {
        Self {
            ptr: AtomicPtr::new(Self::to_ptr(value)),
        }
    }

    pub(crate) fn swap(&self, value: Option<A>) -> Option<A> {
        let new_ptr = Self::to_ptr(value);
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);
        Self::from_ptr(old_ptr)
    }

    #[inline]
    pub(crate) fn store(&self, value: Option<A>) {
        // TODO make this more efficient by using drop_in_place ?
        drop(self.swap(value));
    }

    #[inline]
    pub(crate) fn take(&self) -> Option<A> {
        self.swap(None)
    }
}

impl<A> Drop for AtomicOption<A> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::Acquire);

        if !ptr.is_null() {
            // This is safe because we only do this for pointers created with `Box::into_raw`
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    }
}
