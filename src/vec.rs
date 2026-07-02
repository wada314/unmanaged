//! A growable array type that omits the allocator from its inline representation.

use ::allocator_api2::alloc::Allocator;
use ::core::alloc::Layout;
use ::core::marker::PhantomData;
use ::core::mem;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::NonNull;
use ::core::ptr;
use ::core::slice;

/// A contiguous growable array type that does not store its allocator.
///
/// The inline size is three words (`ptr`, `len`, `cap`), identical to the
/// standard [`Vec`](alloc::vec::Vec) *without* an allocator type parameter.
/// Every operation that allocates, grows, or frees memory takes an explicit
/// `&A` allocator argument. The caller is responsible for always passing the
/// same allocator instance that was used to create the buffer.
///
/// Immutable and mutable slice methods are available through [`Deref`] and
/// [`DerefMut`] to `[T]`, so they are not reimplemented here.
///
/// # Dropping
///
/// An `UnmanagedVec` cannot free itself, so it deliberately has no useful
/// [`Drop`] behavior. Instead, release it with [`UnmanagedVec::deallocate`].
/// Letting an `UnmanagedVec` be dropped implicitly panics at runtime.
pub struct UnmanagedVec<T> {
    ptr: NonNull<T>,
    len: usize,
    cap: usize,
    _marker: PhantomData<T>,
}

impl<T> UnmanagedVec<T> {
    /// Creates a new, empty `UnmanagedVec`. This does not allocate.
    #[must_use = "an UnmanagedVec must be released via `deallocate`; dropping it panics"]
    pub const fn new() -> Self {
        Self {
            ptr: NonNull::dangling(),
            len: 0,
            cap: 0,
            _marker: PhantomData,
        }
    }

    /// Returns the number of elements the vector can hold without reallocating.
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// Appends an element, growing the backing buffer through `alloc` if needed.
    ///
    /// `alloc` must be the same allocator used for any previous growth of this
    /// vector.
    pub fn push_in<A: Allocator>(&mut self, value: T, alloc: &A) {
        if self.len == self.cap {
            self.grow_in(alloc);
        }
        // SAFETY: `len < cap` after the grow above, so the slot is in bounds and uninitialized.
        unsafe {
            ptr::write(self.ptr.as_ptr().add(self.len), value);
        }
        self.len += 1;
    }

    /// Removes and returns the last element, or `None` if empty.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: the element at `len` was initialized and is now logically removed.
        Some(unsafe { ptr::read(self.ptr.as_ptr().add(self.len)) })
    }

    /// Doubles the capacity (or allocates the first slot) using `alloc`.
    fn grow_in<A: Allocator>(&mut self, alloc: &A) {
        let new_cap = if self.cap == 0 { 1 } else { self.cap * 2 };
        let new_layout = Layout::array::<T>(new_cap).expect("capacity overflow");

        let new_ptr = if self.cap == 0 {
            alloc.allocate(new_layout)
        } else {
            let old_layout = Layout::array::<T>(self.cap).expect("capacity overflow");
            // SAFETY: `ptr` was allocated by `alloc` with `old_layout` and `new_layout` is larger.
            unsafe { alloc.grow(self.ptr.cast(), old_layout, new_layout) }
        }
        .expect("allocation failed");

        self.ptr = new_ptr.cast();
        self.cap = new_cap;
    }

    /// Drops all elements and frees the backing buffer through `alloc`.
    ///
    /// This consumes `self`, which is the only supported way to release an
    /// `UnmanagedVec`. `alloc` must be the same allocator used to grow it.
    pub fn deallocate<A: Allocator>(self, alloc: &A) {
        // Copy out the raw parts (all `Copy`) and forget `self` *before* any
        // fallible work, so no drop obligation for `self` remains on any path.
        let ptr = self.ptr;
        let len = self.len;
        let cap = self.cap;
        mem::forget(self);

        // SAFETY: the first `len` elements are initialized; drop them in place.
        unsafe {
            ptr::drop_in_place(slice::from_raw_parts_mut(ptr.as_ptr(), len));
        }
        if cap != 0 {
            let layout = Layout::array::<T>(cap).expect("capacity overflow");
            // SAFETY: `ptr` was allocated by `alloc` with this layout.
            unsafe {
                alloc.deallocate(ptr.cast(), layout);
            }
        }
    }
}

impl<T> Deref for UnmanagedVec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: `ptr` is valid for `len` initialized elements.
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> DerefMut for UnmanagedVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: `ptr` is valid for `len` initialized elements and we hold `&mut self`.
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T> Drop for UnmanagedVec<T> {
    fn drop(&mut self) {
        // An `UnmanagedVec` has no allocator, so it cannot free itself here.
        // Reaching this point means the value was dropped implicitly instead of
        // being released via `deallocate`, which would leak the backing buffer.
        // Fail loudly at runtime rather than silently leaking.
        panic!("UnmanagedVec must be released via `deallocate`, not dropped implicitly");
    }
}

#[cfg(test)]
mod tests {
    use super::UnmanagedVec;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn allocate_use_and_deallocate() {
        let alloc = Global;
        let mut v = UnmanagedVec::<u32>::new();
        for i in 0..8 {
            v.push_in(i, &alloc);
        }
        // `len`, `is_empty`, indexing, iteration all come from the `[T]` deref.
        assert_eq!(v.len(), 8);
        assert!(!v.is_empty());
        assert_eq!(&v[..], &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(v.iter().sum::<u32>(), 28);
        assert_eq!(v.pop(), Some(7));
        v.deallocate(&alloc);
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let alloc = Global;
        let mut v = UnmanagedVec::<u32>::new();
        v.push_in(1, &alloc);
        // Dropped implicitly at end of scope -> panics.
    }
}
