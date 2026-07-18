//! A heap-allocated box that omits the allocator *instance* while retaining
//! the allocator *type* as a type parameter.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::boxed::Box;
use ::core::marker::PhantomData;
use ::core::mem;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::NonNull;

use crate::traits::{CloneIn, DeallocateIn};

/// An owned heap allocation that does not store an allocator instance.
///
/// Inline size is one pointer plus [`PhantomData<A>`] (ZST).
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

    /// # Safety
    ///
    /// `alloc` must own this box's allocation.
    unsafe fn into_box(self, alloc: A) -> Box<T, A> {
        let ptr = self.ptr.as_ptr();
        mem::forget(self);
        unsafe { Box::from_raw_in(ptr, alloc) }
    }

    /// Releases the inner value via [`DeallocateIn`], then frees the box slot.
    ///
    /// # Safety
    ///
    /// `alloc` must own this box's allocation and the inner value's buffers.
    pub unsafe fn deallocate(self, alloc: A)
    where
        T: DeallocateIn<A>,
        A: Clone,
    {
        let value = unsafe { self.into_inner(alloc.clone()) };
        // SAFETY: same `alloc` obligation as this method.
        unsafe { value.deallocate_in(alloc) };
    }

    /// # Safety
    ///
    /// `alloc` must own this box's allocation.
    pub unsafe fn into_inner(self, alloc: A) -> T {
        let b = unsafe { self.into_box(alloc) };
        Box::into_inner(b)
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
    A: Allocator + Clone,
{
    #[inline]
    unsafe fn deallocate_in(self, alloc: A) {
        // SAFETY: forwarded to the caller's obligation on `alloc`.
        unsafe { self.deallocate(alloc) };
    }
}

impl<T, A: Allocator> Deref for UnmanagedBox<T, A> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T, A: Allocator> DerefMut for UnmanagedBox<T, A> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.ptr.as_mut() }
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
    use ::alloc::string::ToString;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn new_deref_and_deallocate() {
        let mut b = UnmanagedBox::new_in(41u32, Global);
        assert_eq!(*b, 41);
        *b += 1;
        assert_eq!(*b, 42);
        unsafe { b.deallocate(Global) };
    }

    #[test]
    fn into_inner_returns_value() {
        let b = UnmanagedBox::new_in(String::from("owned"), Global);
        let s = unsafe { b.into_inner(Global) };
        assert_eq!(s, "owned");
    }

    #[test]
    fn deallocate_drops_inner() {
        let b = UnmanagedBox::new_in("dropme".to_string(), Global);
        unsafe { b.deallocate(Global) };
    }

    #[test]
    fn clone_in_copies_value() {
        let b = UnmanagedBox::new_in(String::from("boxed"), Global);
        let c = b.clone_in(Global);
        assert_eq!(&*c, "boxed");
        assert_eq!(&*b, "boxed");
        unsafe { c.deallocate(Global) };
        unsafe { b.deallocate(Global) };
    }

    #[test]
    fn deallocate_nested_unmanaged_string() {
        use crate::UnmanagedString;
        let inner = UnmanagedString::from_string(crate::String::from_str_in("inner", Global));
        let b = UnmanagedBox::new_in(inner, Global);
        unsafe { b.deallocate(Global) };
    }

    #[test]
    #[should_panic(expected = "must be released")]
    fn implicit_drop_panics() {
        let _b = UnmanagedBox::new_in(7u32, Global);
    }
}
