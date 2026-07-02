//! A heap-allocated box type that omits the allocator from its inline
//! representation.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::boxed::Box;
use ::core::mem;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::NonNull;

/// An owned heap allocation that does not store its allocator.
///
/// The inline size is a single pointer, unlike [`Box<T, A>`] whose size grows
/// with a non-zero-sized allocator `A`. Because a `Box` never needs to
/// reallocate, the allocator is only required to **create** and **release** the
/// value; all access to the pointee is available allocator-free through
/// [`Deref`] and [`DerefMut`]. This is why `UnmanagedBox` needs no borrow guard,
/// unlike [`crate::UnmanagedVec`] and [`crate::UnmanagedString`].
///
/// # Dropping
///
/// An `UnmanagedBox` cannot free itself, so dropping it implicitly panics at
/// runtime. Always release it through [`UnmanagedBox::deallocate`] or
/// [`UnmanagedBox::into_inner`].
#[must_use = "an UnmanagedBox must be released via `deallocate`/`into_inner`; dropping it panics"]
pub struct UnmanagedBox<T> {
    ptr: NonNull<T>,
}

impl<T> UnmanagedBox<T> {
    /// Allocates memory through `alloc` and moves `value` into it.
    ///
    /// `Box::new_in` performs the allocation; passing `&alloc` keeps a move-only
    /// allocator intact for the later `deallocate`/`into_inner` call.
    #[must_use = "an UnmanagedBox must be released via `deallocate`/`into_inner`; dropping it panics"]
    pub fn new_in<A: Allocator>(value: T, alloc: A) -> Self {
        Self::from_box(Box::new_in(value, alloc))
    }

    /// Decomposes an existing [`Box<T, A>`] into the allocator-less form.
    ///
    /// The allocator carried by `b` is returned by value and dropped; the
    /// heap allocation is transferred into the returned `UnmanagedBox`.
    pub fn from_box<A: Allocator>(b: Box<T, A>) -> Self {
        let (ptr, _alloc) = Box::into_raw_with_allocator(b);
        Self {
            // SAFETY: `Box` never yields a null pointer.
            ptr: unsafe { NonNull::new_unchecked(ptr) },
        }
    }

    /// Reconstitutes an owning [`Box<T, A>`] from this box and `alloc`.
    ///
    /// # Safety
    ///
    /// `alloc` must be the same allocator instance (or an equivalent one that
    /// shares the same allocated blocks) that was used to create this box.
    unsafe fn into_box<A: Allocator>(self, alloc: A) -> Box<T, A> {
        let ptr = self.ptr.as_ptr();
        mem::forget(self);
        // SAFETY: `ptr` was produced by a `Box` using this allocator.
        unsafe { Box::from_raw_in(ptr, alloc) }
    }

    /// Drops the value and frees the backing allocation through `alloc`.
    ///
    /// # Safety
    ///
    /// `alloc` must be the same allocator instance (or an equivalent one that
    /// shares the same allocated blocks) that was used to create this box.
    pub unsafe fn deallocate<A: Allocator>(self, alloc: A) {
        // SAFETY: forwarded to the caller's obligation.
        let _ = unsafe { self.into_box(alloc) };
    }

    /// Moves the value out of the box and frees the backing allocation.
    ///
    /// # Safety
    ///
    /// `alloc` must be the same allocator instance (or an equivalent one that
    /// shares the same allocated blocks) that was used to create this box.
    pub unsafe fn into_inner<A: Allocator>(self, alloc: A) -> T {
        // SAFETY: forwarded to the caller's obligation.
        let b = unsafe { self.into_box(alloc) };
        Box::into_inner(b)
    }
}

impl<T> Deref for UnmanagedBox<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: `ptr` points to a live, initialized `T` owned by this box.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for UnmanagedBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: `ptr` points to a live `T` owned by this box and we hold `&mut self`.
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> Drop for UnmanagedBox<T> {
    fn drop(&mut self) {
        // An `UnmanagedBox` has no allocator, so it cannot free itself here.
        // Reaching this point means the value was dropped implicitly instead of
        // being released via `deallocate`/`into_inner`, which would leak.
        panic!("UnmanagedBox must be released via `deallocate`/`into_inner`, not dropped implicitly");
    }
}

#[cfg(test)]
mod tests {
    use super::UnmanagedBox;
    use ::alloc::string::String;
    use ::alloc::string::ToString;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn new_deref_and_deallocate() {
        let mut b = UnmanagedBox::new_in(41u32, Global);
        // Access needs no allocator.
        assert_eq!(*b, 41);
        *b += 1;
        assert_eq!(*b, 42);
        // SAFETY: same allocator as used for creation.
        unsafe { b.deallocate(Global) };
    }

    #[test]
    fn into_inner_returns_value() {
        let b = UnmanagedBox::new_in(String::from("owned"), Global);
        // SAFETY: same allocator.
        let s = unsafe { b.into_inner(Global) };
        assert_eq!(s, "owned");
    }

    #[test]
    fn deallocate_drops_inner() {
        // A boxed `String` must have its heap freed when the box is released.
        let b = UnmanagedBox::new_in("dropme".to_string(), Global);
        // SAFETY: same allocator.
        unsafe { b.deallocate(Global) };
    }

    #[test]
    #[should_panic(expected = "must be released")]
    fn implicit_drop_panics() {
        let _b = UnmanagedBox::new_in(7u32, Global);
        // `_b` dropped implicitly at end of scope -> panics.
    }
}
