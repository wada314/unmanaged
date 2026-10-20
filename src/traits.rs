//! Allocator-aware construction helpers shared by unmanaged containers.
//!
//! These traits exist so a container can be generic over element types that
//! themselves need an allocator to copy or free (nested unmanaged values).

use ::allocator_api2::alloc::Allocator;

/// Allocate a deep copy of `self` in `alloc`.
///
/// `alloc` owns the **new** value's buffers; it need not be the same instance
/// that owns `self`.
///
/// [`Copy`] types ignore `alloc` (bitwise copy). Heap-backed types use it.
pub trait CloneIn<A: Allocator> {
    /// Allocates the copy in `alloc`.
    fn clone_in(&self, alloc: A) -> Self;
}

/// Allocator-aware empty / type-zero construction (like [`Default`], with `alloc`).
///
/// [`Copy`] + [`Default`] types ignore `alloc`. Heap-backed types
/// ([`UnmanagedString`](crate::UnmanagedString), [`UnmanagedVec`](crate::UnmanagedVec),
/// [`UnmanagedBox`](crate::UnmanagedBox)) use it to allocate.
///
/// Do **not** implement [`Default`] in a way that picks an allocator and then
/// rely on the [`Copy`] blanket here — non-`Copy` messages must impl `DefaultIn`
/// explicitly so the caller's `alloc` is honored.
pub trait DefaultIn<A: Allocator> {
    /// Builds the empty / type-zero value, using `alloc` when heap-backed.
    fn default_in(alloc: A) -> Self;
}

/// Allocator-aware borrowed → owned conversion (like [`alloc::borrow::ToOwned`], with `alloc`).
///
/// Mirrors std's shape:
/// - blanket [`CloneIn`] → `Owned = Self` (cf. `T: Clone` → `Owned = T`)
/// - [`str`] → [`UnmanagedString`](crate::UnmanagedString) (cf. `String`)
/// - `[T]` → [`UnmanagedVec`](crate::UnmanagedVec) (cf. `Vec<T>`)
///
/// `alloc` owns the returned value's buffers when the owned form is heap-backed.
pub trait ToOwnedIn<A: Allocator> {
    /// Owned form produced in `alloc`.
    type Owned;

    /// Copies `self` into a value owned by `alloc`.
    fn to_owned_in(&self, alloc: A) -> Self::Owned;
}

/// Like [`alloc::borrow::ToOwned`] for `T: Clone`: owned form is `Self`, built via [`CloneIn`].
impl<T, A> ToOwnedIn<A> for T
where
    T: CloneIn<A>,
    A: Allocator,
{
    type Owned = T;

    #[inline]
    fn to_owned_in(&self, alloc: A) -> T {
        self.clone_in(alloc)
    }
}

/// Release heap storage owned by `self` through `alloc`.
///
/// [`Copy`] types are a no-op (no heap owned by the value itself).
pub trait DeallocateIn<A: Allocator> {
    /// Frees buffers owned by `self`.
    ///
    /// # Safety
    ///
    /// `alloc` must own every buffer that `self` still holds.
    unsafe fn deallocate_in(self, alloc: &A);
}

/// Bitwise-copy payloads: `alloc` is irrelevant.
impl<T: Copy, A: Allocator> CloneIn<A> for T {
    #[inline]
    fn clone_in(&self, _alloc: A) -> Self {
        *self
    }
}

/// Bitwise-copy + [`Default`] payloads: `alloc` is irrelevant.
impl<T, A> DefaultIn<A> for T
where
    T: Copy + Default,
    A: Allocator,
{
    #[inline]
    fn default_in(_alloc: A) -> Self {
        T::default()
    }
}

/// Bitwise-copy payloads own no heap buffers.
impl<T: Copy, A: Allocator> DeallocateIn<A> for T {
    #[inline]
    unsafe fn deallocate_in(self, _alloc: &A) {}
}
