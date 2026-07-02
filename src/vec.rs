//! A growable array type that omits the allocator from its inline representation.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::vec::Vec;
use ::core::marker::PhantomData;
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::NonNull;
use ::core::slice;

/// A contiguous growable array type that does not store its allocator.
///
/// The inline size is three words (`ptr`, `len`, `cap`), identical to the
/// standard [`Vec`] *without* an allocator type parameter. The allocator is
/// supplied only for the operations that need it:
///
/// * Slice access (immutable and mutable) is available through [`Deref`] and
///   [`DerefMut`] to `[T]`, which need no allocator.
/// * Growing operations are performed through the full [`Vec`] API by
///   borrowing the vector together with an allocator via
///   [`UnmanagedVec::with_alloc`], which returns a [`VecGuard`].
/// * Releasing the buffer is done with [`UnmanagedVec::deallocate`].
///
/// # Dropping
///
/// An `UnmanagedVec` cannot free itself, so dropping it implicitly panics at
/// runtime. Always release it through [`UnmanagedVec::deallocate`].
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

    /// Borrows the vector together with `alloc`, yielding a guard that derefs to
    /// a full [`Vec<T, A>`] so every growing `Vec` method is available.
    ///
    /// When the returned [`VecGuard`] is dropped, the (possibly reallocated)
    /// pointer, length, and capacity are written back into `self`. `alloc` must
    /// be the same allocator used for any previous growth of this vector.
    pub fn with_alloc<A: Allocator>(&mut self, alloc: A) -> VecGuard<'_, T, A> {
        // SAFETY: `ptr`/`len`/`cap` are either the empty (dangling, 0, 0) state
        // produced by `new`, or a set of raw parts previously produced by a
        // `Vec<T, A>` using the same allocator. Either is valid for
        // `from_raw_parts_in`.
        let vec = unsafe { Vec::from_raw_parts_in(self.ptr.as_ptr(), self.len, self.cap, alloc) };
        VecGuard {
            vec: ManuallyDrop::new(vec),
            origin: self,
        }
    }

    /// Drops all elements and frees the backing buffer through `alloc`.
    ///
    /// This consumes `self`, which is the only supported way to release an
    /// `UnmanagedVec`. `alloc` must be the same allocator used to grow it.
    pub fn deallocate<A: Allocator>(self, alloc: A) {
        // Copy out the raw parts (all `Copy`) and forget `self` *before*
        // building the `Vec`, so no drop obligation for `self` remains.
        let ptr = self.ptr.as_ptr();
        let len = self.len;
        let cap = self.cap;
        ::core::mem::forget(self);

        // Reconstitute a real `Vec` and let it drop: this drops the elements and
        // frees the buffer, correctly handling zero capacity and ZSTs.
        // SAFETY: same invariants as `with_alloc`.
        let _ = unsafe { Vec::from_raw_parts_in(ptr, len, cap, alloc) };
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

/// A temporary handle that grants the full [`Vec<T, A>`] API over an
/// [`UnmanagedVec`] by pairing it with an allocator for the duration of a
/// borrow.
///
/// Obtain one via [`UnmanagedVec::with_alloc`]. On drop, the current raw parts
/// of the underlying `Vec` are written back into the originating
/// `UnmanagedVec`; the buffer itself is **not** freed.
pub struct VecGuard<'a, T, A: Allocator> {
    vec: ManuallyDrop<Vec<T, A>>,
    origin: &'a mut UnmanagedVec<T>,
}

impl<'a, T, A: Allocator> Deref for VecGuard<'a, T, A> {
    type Target = Vec<T, A>;

    fn deref(&self) -> &Vec<T, A> {
        &self.vec
    }
}

impl<'a, T, A: Allocator> DerefMut for VecGuard<'a, T, A> {
    fn deref_mut(&mut self) -> &mut Vec<T, A> {
        &mut self.vec
    }
}

impl<'a, T, A: Allocator> Drop for VecGuard<'a, T, A> {
    fn drop(&mut self) {
        // SAFETY: `self.vec` is never used again after this take.
        let vec = unsafe { ManuallyDrop::take(&mut self.vec) };
        // Decompose without freeing, then write the current parts back into the
        // origin. The allocator is returned by value and dropped here; the
        // buffer stays alive and is owned by the origin again.
        let (ptr, len, cap, _alloc) = vec.into_raw_parts_with_alloc();
        self.origin.ptr = unsafe { NonNull::new_unchecked(ptr) };
        self.origin.len = len;
        self.origin.cap = cap;
    }
}

#[cfg(test)]
mod tests {
    use super::UnmanagedVec;
    use ::alloc::string::String;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn grow_via_guard_then_deallocate() {
        let mut v = UnmanagedVec::<u32>::new();
        {
            let mut g = v.with_alloc(Global);
            // Full `Vec` API is available through the guard.
            g.push(1);
            g.push(2);
            g.extend([3, 4, 5]);
            assert_eq!(g.len(), 5);
            assert_eq!(g.pop(), Some(5));
        }
        // Parts were written back to `v`; slice access works without a guard.
        assert_eq!(v.len(), 4);
        assert_eq!(&v[..], &[1, 2, 3, 4]);
        assert_eq!(v.iter().sum::<u32>(), 10);
        v.deallocate(Global);
    }

    #[test]
    fn empty_vec_deallocates() {
        let v = UnmanagedVec::<String>::new();
        v.deallocate(Global);
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let mut v = UnmanagedVec::<u32>::new();
        v.with_alloc(Global).push(1);
        // `v` dropped implicitly at end of scope -> panics.
    }
}
