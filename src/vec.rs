//! A growable array type that omits the allocator *instance* from its inline
//! representation, while retaining the allocator *type* as a type parameter.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::vec::Vec;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::marker::PhantomData;
use ::core::mem::{self, ManuallyDrop};
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::ptr::{self, NonNull};
use ::core::slice;

use crate::traits::{CloneIn, DeallocateIn, ToOwnedIn};

/// A contiguous growable array that does not store an allocator instance.
///
/// The inline size is three words (`ptr`, `len`, `cap`) plus [`PhantomData<A>`]
/// (ZST). The allocator type `A` is recorded so growth and free APIs are
/// type-checked; the instance is supplied only for operations that need it:
///
/// * Slice access via [`Deref`] / [`DerefMut`] to `[T]` needs no allocator.
/// * Growth via [`UnmanagedVec::with_alloc`] → [`VecGuard`].
/// * Copy via [`UnmanagedVec::clone_in`] (allocates a new buffer in the given allocator).
/// * Release via [`UnmanagedVec::deallocate`].
///
/// # Dropping
///
/// Dropping implicitly panics. Always release through [`deallocate`](Self::deallocate).
pub struct UnmanagedVec<T, A: Allocator> {
    ptr: NonNull<T>,
    len: usize,
    cap: usize,
    _marker: PhantomData<T>,
    _alloc: PhantomData<A>,
}

impl<T, A: Allocator> UnmanagedVec<T, A> {
    /// Creates a new, empty `UnmanagedVec` using `alloc`.
    ///
    /// `Vec::new_in` does not allocate, so passing `&alloc` keeps a move-only
    /// allocator intact for later use.
    #[must_use = "an UnmanagedVec must be released via `deallocate`; dropping it panics"]
    pub fn new(alloc: A) -> Self {
        Self::from_vec(Vec::new_in(alloc))
    }

    /// Decomposes an existing [`Vec`] into the instance-less representation.
    ///
    /// The allocator carried by `vec` is dropped; the buffer is transferred.
    pub fn from_vec(vec: Vec<T, A>) -> Self {
        let (ptr, len, cap, _alloc) = vec.into_raw_parts_with_alloc();
        Self {
            // SAFETY: `Vec` never yields a null data pointer.
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            len,
            cap,
            _marker: PhantomData,
            _alloc: PhantomData,
        }
    }

    /// Returns the number of elements the vector can hold without reallocating.
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// Allocates a copy of this vector in `alloc`.
    ///
    /// Elements are deep-copied with [`CloneIn`]. The destination buffer is
    /// owned by `alloc`; it need not be the same allocator instance that owns
    /// `self`.
    #[must_use = "an UnmanagedVec must be released via `deallocate`; dropping it panics"]
    pub fn clone_in(&self, alloc: A) -> Self
    where
        T: CloneIn<A>,
        A: Clone,
    {
        <[T] as ToOwnedIn<A>>::to_owned_in(self, alloc)
    }

    /// Returns the raw parts (data pointer, length, capacity) of the vector.
    pub(crate) fn as_raw_parts(&self) -> (*mut T, usize, usize) {
        (self.ptr.as_ptr(), self.len, self.cap)
    }

    /// Overwrites the raw parts of the vector.
    ///
    /// # Safety
    ///
    /// The parts must describe a valid buffer owned by this vector.
    pub(crate) unsafe fn set_raw_parts(&mut self, ptr: *mut T, len: usize, cap: usize) {
        // SAFETY: `Vec` never yields a null data pointer.
        self.ptr = unsafe { NonNull::new_unchecked(ptr) };
        self.len = len;
        self.cap = cap;
    }

    /// Borrows the vector together with `alloc`, yielding a [`VecGuard`].
    ///
    /// # Safety
    ///
    /// `alloc` must be interchangeable with the allocator that owns this buffer.
    pub unsafe fn with_alloc(&mut self, alloc: A) -> VecGuard<'_, T, A> {
        let vec = unsafe { Vec::from_raw_parts_in(self.ptr.as_ptr(), self.len, self.cap, alloc) };
        VecGuard {
            vec: ManuallyDrop::new(vec),
            origin: self,
        }
    }

    /// Releases every element via [`DeallocateIn`], then frees the buffer.
    ///
    /// Nested unmanaged elements are freed by the element trait (not by
    /// panicking [`Drop`]), so `UnmanagedVec<UnmanagedString<_>>` can be
    /// released with a single call.
    ///
    /// # Safety
    ///
    /// `alloc` must own this vector's buffer and every live element's buffers.
    pub unsafe fn deallocate(self, alloc: A)
    where
        T: DeallocateIn<A>,
        A: Clone,
    {
        let ptr = self.ptr.as_ptr();
        let len = self.len;
        let cap = self.cap;
        mem::forget(self);
        for i in 0..len {
            // SAFETY: `ptr` holds `len` initialized elements; each is read once.
            let elem = unsafe { ptr::read(ptr.add(i)) };
            // SAFETY: same `alloc` obligation as this method.
            unsafe { elem.deallocate_in(alloc.clone()) };
        }
        // Length 0 so `Vec` does not drop elements again.
        let _ = unsafe { Vec::from_raw_parts_in(ptr, 0, cap, alloc) };
    }

    /// Frees the backing buffer of a vector whose elements were already removed.
    ///
    /// Use this when elements were released separately (e.g. message `Drop`
    /// paths that `pop` + element teardown) and `T` may not implement
    /// [`DeallocateIn`].
    ///
    /// # Safety
    ///
    /// `self.len` must be `0`. `alloc` must own this vector's buffer.
    pub unsafe fn deallocate_buffer(self, alloc: A) {
        debug_assert_eq!(self.len, 0);
        let ptr = self.ptr.as_ptr();
        let cap = self.cap;
        mem::forget(self);
        let _ = unsafe { Vec::from_raw_parts_in(ptr, 0, cap, alloc) };
    }
}

impl<T, A> ToOwnedIn<A> for [T]
where
    T: CloneIn<A>,
    A: Allocator + Clone,
{
    type Owned = UnmanagedVec<T, A>;

    #[inline]
    fn to_owned_in(&self, alloc: A) -> UnmanagedVec<T, A> {
        let mut vec = Vec::with_capacity_in(self.len(), alloc.clone());
        for e in self.iter() {
            vec.push(e.clone_in(alloc.clone()));
        }
        UnmanagedVec::from_vec(vec)
    }
}

impl<T, A> CloneIn<A> for UnmanagedVec<T, A>
where
    T: CloneIn<A>,
    A: Allocator + Clone,
{
    #[inline]
    fn clone_in(&self, alloc: A) -> Self {
        UnmanagedVec::clone_in(self, alloc)
    }
}

impl<T, A> DeallocateIn<A> for UnmanagedVec<T, A>
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

impl<T, A: Allocator> Deref for UnmanagedVec<T, A> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: `ptr` is valid for `len` initialized elements.
        unsafe { slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl<T, A: Allocator> DerefMut for UnmanagedVec<T, A> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: `ptr` is valid for `len` initialized elements and we hold `&mut self`.
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl<T, A: Allocator> Drop for UnmanagedVec<T, A> {
    fn drop(&mut self) {
        panic!("UnmanagedVec must be released via `deallocate`, not dropped implicitly");
    }
}

impl<T: fmt::Debug, A: Allocator> fmt::Debug for UnmanagedVec<T, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

impl<T: PartialEq, A: Allocator> PartialEq for UnmanagedVec<T, A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: Eq, A: Allocator> Eq for UnmanagedVec<T, A> {}

impl<T: Hash, A: Allocator> Hash for UnmanagedVec<T, A> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

/// Temporary full [`Vec<T, A>`] API over an [`UnmanagedVec`].
pub struct VecGuard<'a, T, A: Allocator> {
    vec: ManuallyDrop<Vec<T, A>>,
    origin: &'a mut UnmanagedVec<T, A>,
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
        let vec = unsafe { ManuallyDrop::take(&mut self.vec) };
        let (ptr, len, cap, _alloc) = vec.into_raw_parts_with_alloc();
        unsafe { self.origin.set_raw_parts(ptr, len, cap) };
    }
}

#[cfg(test)]
mod tests {
    use super::UnmanagedVec;
    use crate::traits::ToOwnedIn;
    use ::alloc::string::String;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn grow_via_guard_then_deallocate() {
        let mut v = UnmanagedVec::<u32, Global>::new(Global);
        {
            let mut g = unsafe { v.with_alloc(Global) };
            g.push(1);
            g.push(2);
            g.extend([3, 4, 5]);
            assert_eq!(g.len(), 5);
            assert_eq!(g.pop(), Some(5));
        }
        assert_eq!(v.len(), 4);
        assert_eq!(&v[..], &[1, 2, 3, 4]);
        assert_eq!(v.iter().sum::<u32>(), 10);
        unsafe { v.deallocate(Global) };
    }

    #[test]
    fn empty_vec_deallocates() {
        let v = UnmanagedVec::<String, Global>::new(Global);
        unsafe { v.deallocate(Global) };
    }

    #[test]
    fn from_vec_roundtrip() {
        let mut vec = ::allocator_api2::vec::Vec::<u32, Global>::new_in(Global);
        vec.extend([10, 20, 30]);
        let v = UnmanagedVec::from_vec(vec);
        assert_eq!(&v[..], &[10, 20, 30]);
        unsafe { v.deallocate(Global) };
    }

    #[test]
    fn debug_and_eq_compare_slice_contents() {
        use ::alloc::format;

        let mut a = UnmanagedVec::<u8, Global>::new(Global);
        let mut b = UnmanagedVec::<u8, Global>::new(Global);
        {
            let mut ga = unsafe { a.with_alloc(Global) };
            ga.extend_from_slice(b"hi");
            let mut gb = unsafe { b.with_alloc(Global) };
            gb.extend_from_slice(b"hi");
        }
        assert_eq!(a, b);
        assert_eq!(format!("{:?}", a), "[104, 105]");
        unsafe { a.deallocate(Global) };
        unsafe { b.deallocate(Global) };
    }

    #[test]
    fn clone_in_copies_elements() {
        let mut v = UnmanagedVec::<u32, Global>::new(Global);
        {
            let mut g = unsafe { v.with_alloc(Global) };
            g.extend([1, 2, 3]);
        }
        let c = v.clone_in(Global);
        assert_eq!(&c[..], &[1, 2, 3]);
        assert_eq!(&v[..], &[1, 2, 3]);
        unsafe { c.deallocate(Global) };
        unsafe { v.deallocate(Global) };
    }

    #[test]
    fn slice_to_owned_in_copies_elements() {
        let owned: UnmanagedVec<u32, Global> = [1u32, 2, 3].as_slice().to_owned_in(Global);
        assert_eq!(&owned[..], &[1, 2, 3]);
        unsafe { owned.deallocate(Global) };
    }

    #[test]
    fn clone_in_and_deallocate_nested_strings() {
        use crate::string::String as AString;
        use crate::UnmanagedString;

        let mut v = UnmanagedVec::<UnmanagedString<Global>, Global>::new(Global);
        {
            let mut g = unsafe { v.with_alloc(Global) };
            g.push(UnmanagedString::from_string(AString::from_str_in(
                "a", Global,
            )));
            g.push(UnmanagedString::from_string(AString::from_str_in(
                "b", Global,
            )));
        }
        let c = v.clone_in(Global);
        assert_eq!(&*c[0], "a");
        assert_eq!(&*c[1], "b");
        unsafe { c.deallocate(Global) };
        unsafe { v.deallocate(Global) };
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let mut v = UnmanagedVec::<u32, Global>::new(Global);
        unsafe { v.with_alloc(Global) }.push(1);
    }
}
