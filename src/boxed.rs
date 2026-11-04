//! A heap-allocated box that omits the allocator *instance* while retaining
//! the allocator *type* as a type parameter.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::boxed::Box;
use ::core::borrow::Borrow;
use ::core::borrow::BorrowMut;
use ::core::cmp::Ordering;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::marker::PhantomData;
use ::core::mem;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::NonNull;

use crate::traits::{CloneIn, DeallocateIn, DefaultIn};

/// An owned heap allocation that does not store an allocator instance.
///
/// Inline size is one pointer plus [`PhantomData<A>`] (ZST).
///
/// `Send` and `Sync` follow `T` and `A`, matching [`Box`].
///
/// # Dropping
///
/// Dropping implicitly panics. Release the allocation with
/// [`deallocate`](Self::deallocate), or take the value out with
/// [`into_inner`](Self::into_inner).
#[must_use = "an UnmanagedBox must be released via `deallocate`/`into_inner`; dropping it panics"]
pub struct UnmanagedBox<T, A: Allocator> {
    ptr: NonNull<T>,
    _alloc: PhantomData<A>,
}

impl<T, A: Allocator> UnmanagedBox<T, A> {
    /// Allocates through `alloc` and moves `value` into it.
    #[must_use = "an UnmanagedBox must be released via `deallocate`/`into_inner`; dropping it panics"]
    pub fn new_in(value: T, alloc: A) -> Self {
        Self::from_box(Box::new_in(value, alloc))
    }

    /// Decomposes an existing [`Box<T, A>`] into the instance-less form.
    pub fn from_box(b: Box<T, A>) -> Self {
        let (ptr, _alloc) = Box::into_raw_with_allocator(b);
        Self {
            // SAFETY: `Box` never yields a null pointer.
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            _alloc: PhantomData,
        }
    }

    /// Allocates a copy of the boxed value in `alloc`.
    ///
    /// The destination allocation is owned by `alloc`; it need not be the same
    /// allocator instance that owns `self`.
    #[must_use = "an UnmanagedBox must be released via `deallocate`/`into_inner`; dropping it panics"]
    pub fn clone_in(&self, alloc: A) -> Self
    where
        T: CloneIn<A>,
        A: Clone,
    {
        Self::new_in((**self).clone_in(alloc.clone()), alloc)
    }

    /// Rebuilds an allocator-carrying [`Box`] from this value.
    ///
    /// # Safety
    ///
    /// `alloc` must own this box's allocation.
    pub unsafe fn into_box(self, alloc: A) -> Box<T, A> {
        let ptr = self.ptr.as_ptr();
        mem::forget(self);
        // SAFETY: caller guarantees `alloc` owns this allocation; `ptr` came from
        // `Box`/`new_in` and is non-null. `forget` prevents the panicking `Drop`.
        unsafe { Box::from_raw_in(ptr, alloc) }
    }

    /// Releases the inner value via [`DeallocateIn`], then frees the box slot.
    ///
    /// # Safety
    ///
    /// `alloc` must own this box's allocation and the inner value's buffers.
    pub unsafe fn deallocate(self, alloc: &A)
    where
        T: DeallocateIn<A>,
    {
        let ptr = self.ptr.as_ptr();
        mem::forget(self);
        // `alloc` is `&A`, so this is `Box<T, &A>`, not `Box<T, A>` — a
        // different allocator type than the one used to allocate the slot.
        // SAFETY: caller guarantees `alloc` owns the slot; `ptr` is the live
        // allocation. `forget` prevents the panicking `Drop`.
        let b = unsafe { Box::<T, &A>::from_raw_in(ptr, alloc) };
        let value = Box::into_inner(b);
        // SAFETY: same `alloc` obligation as this method.
        unsafe { value.deallocate_in(alloc) };
    }

    /// Moves the inner value out and frees the box slot.
    ///
    /// The returned `T` is not dropped here. When `T` itself owns unmanaged
    /// buffers, release those separately.
    ///
    /// # Safety
    ///
    /// `alloc` must own this box's allocation.
    pub unsafe fn into_inner(self, alloc: A) -> T {
        // SAFETY: same `alloc` ownership obligation as this method.
        let b = unsafe { self.into_box(alloc) };
        Box::into_inner(b)
    }
}

impl<T, A> DefaultIn<A> for UnmanagedBox<T, A>
where
    T: DefaultIn<A>,
    A: Allocator + Clone,
{
    #[inline]
    fn default_in(alloc: A) -> Self {
        UnmanagedBox::new_in(T::default_in(alloc.clone()), alloc)
    }
}

impl<T, A> CloneIn<A> for UnmanagedBox<T, A>
where
    T: CloneIn<A>,
    A: Allocator + Clone,
{
    #[inline]
    fn clone_in(&self, alloc: A) -> Self {
        UnmanagedBox::clone_in(self, alloc)
    }
}

impl<T, A> DeallocateIn<A> for UnmanagedBox<T, A>
where
    T: DeallocateIn<A>,
    A: Allocator,
{
    #[inline]
    unsafe fn deallocate_in(self, alloc: &A) {
        // SAFETY: forwarded to the caller's obligation on `alloc`.
        unsafe { self.deallocate(alloc) };
    }
}

impl<T, A: Allocator> Deref for UnmanagedBox<T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: `ptr` uniquely owns a live `T` for the lifetime of `self`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T, A: Allocator> DerefMut for UnmanagedBox<T, A> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: `ptr` uniquely owns a live `T` and we hold `&mut self`.
        unsafe { self.ptr.as_mut() }
    }
}

// SAFETY: the box uniquely owns `T` and the allocation belongs to `A`.
// The allocator instance is not stored; `A: Send` matches `Box<T, A>`.
unsafe impl<T, A> Send for UnmanagedBox<T, A>
where
    T: Send,
    A: Allocator + Send,
{
}

// SAFETY: sharing the allocation is sound when `T` and `A` are `Sync`,
// matching `Box<T, A>`.
unsafe impl<T, A> Sync for UnmanagedBox<T, A>
where
    T: Sync,
    A: Allocator + Sync,
{
}

impl<T: fmt::Debug, A: Allocator> fmt::Debug for UnmanagedBox<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: fmt::Display, A: Allocator> fmt::Display for UnmanagedBox<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: PartialEq, A: Allocator> PartialEq for UnmanagedBox<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: Eq, A: Allocator> Eq for UnmanagedBox<T, A> {}

impl<T: PartialOrd, A: Allocator> PartialOrd for UnmanagedBox<T, A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord, A: Allocator> Ord for UnmanagedBox<T, A> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: Hash, A: Allocator> Hash for UnmanagedBox<T, A> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<T, A: Allocator> AsRef<T> for UnmanagedBox<T, A> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T, A: Allocator> AsMut<T> for UnmanagedBox<T, A> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T, A: Allocator> Borrow<T> for UnmanagedBox<T, A> {
    #[inline]
    fn borrow(&self) -> &T {
        self
    }
}

impl<T, A: Allocator> BorrowMut<T> for UnmanagedBox<T, A> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

impl<T, A: Allocator> Drop for UnmanagedBox<T, A> {
    fn drop(&mut self) {
        panic!(
            "UnmanagedBox must be released via `deallocate`/`into_inner`, not dropped implicitly"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::UnmanagedBox;
    use ::alloc::string::String;
    use ::allocator_api2::alloc::Global;
    use ::core::hash::Hasher;

    #[test]
    fn new_deref_and_deallocate() {
        let mut b = UnmanagedBox::new_in(41u32, Global);
        assert_eq!(*b, 41);
        *b += 1;
        assert_eq!(*b, 42);
        // SAFETY: `Global` owns this box.
        unsafe { b.deallocate(&Global) };
    }

    #[test]
    fn into_inner_returns_value() {
        // `alloc::string::String` needs no `CloneIn`/`DeallocateIn` for into_inner.
        let b = UnmanagedBox::new_in(String::from("owned"), Global);
        // SAFETY: `Global` owns this box.
        let s = unsafe { b.into_inner(Global) };
        assert_eq!(s, "owned");
    }

    #[test]
    fn deallocate_copy_inner() {
        let b = UnmanagedBox::new_in(7u32, Global);
        // SAFETY: `Global` owns this box.
        unsafe { b.deallocate(&Global) };
    }

    #[test]
    fn clone_in_copies_value() {
        let b = UnmanagedBox::new_in(42u32, Global);
        let c = b.clone_in(Global);
        assert_eq!(*c, 42);
        assert_eq!(*b, 42);
        // SAFETY: `Global` owns the clone.
        unsafe { c.deallocate(&Global) };
        // SAFETY: `Global` owns the original.
        unsafe { b.deallocate(&Global) };
    }

    #[test]
    fn deallocate_nested_unmanaged_string() {
        use crate::UnmanagedString;
        let inner = UnmanagedString::from_string(crate::String::from_str_in("inner", Global));
        let b = UnmanagedBox::new_in(inner, Global);
        // SAFETY: `Global` owns the box and the nested string buffer.
        unsafe { b.deallocate(&Global) };
    }

    #[test]
    #[should_panic(expected = "must be released")]
    fn implicit_drop_panics() {
        let _b = UnmanagedBox::new_in(7u32, Global);
    }

    #[test]
    fn send_sync_when_inner_is() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<UnmanagedBox<u32, Global>>();
    }

    #[test]
    fn cmp_fmt_and_into_box() {
        use ::alloc::format;
        use ::core::borrow::Borrow;
        use ::core::hash::Hash;

        let b = UnmanagedBox::new_in(7u32, Global);
        let c = UnmanagedBox::new_in(8u32, Global);
        assert_eq!(b, b);
        assert!(b < c);
        assert_eq!(format!("{b}"), "7");
        assert_eq!(format!("{b:?}"), "7");
        assert_eq!(Borrow::<u32>::borrow(&b), &7);
        assert_eq!(AsRef::<u32>::as_ref(&b), &7);

        let mut hb = HasherStub(0);
        let mut hc = HasherStub(0);
        b.hash(&mut hb);
        7u32.hash(&mut hc);
        assert_eq!(hb.finish(), hc.finish());

        // SAFETY: `Global` owns `b`.
        let boxed = unsafe { b.into_box(Global) };
        assert_eq!(*boxed, 7);
        // SAFETY: `Global` owns `c`.
        unsafe { c.deallocate(&Global) };
    }

    struct HasherStub(u64);

    impl Hasher for HasherStub {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(*byte));
            }
        }
    }
}
