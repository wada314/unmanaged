//! Allocator-aware construction helpers shared by unmanaged containers.
//!
//! These traits exist so a container can be generic over element types that
//! themselves need an allocator to copy or free (nested unmanaged values).

use ::allocator_api2::alloc::Allocator;

/// Allocate a deep copy of `self` in `alloc`.
///
/// `alloc` owns the **new** value's buffers; it need not be the same instance
/// that owns `self`.
pub trait CloneIn<A: Allocator> {
    fn clone_in(&self, alloc: A) -> Self;
}

/// Release heap storage owned by `self` through `alloc`.
pub trait DeallocateIn<A: Allocator> {
    /// Frees buffers owned by `self`.
    ///
    /// # Safety
    ///
    /// `alloc` must own every buffer that `self` still holds.
    unsafe fn deallocate_in(self, alloc: A);
}

macro_rules! impl_clone_in_via_clone {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl<A: Allocator> CloneIn<A> for $ty {
                #[inline]
                fn clone_in(&self, _alloc: A) -> Self {
                    self.clone()
                }
            }
        )+
    };
}

macro_rules! impl_deallocate_in_noop {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl<A: Allocator> DeallocateIn<A> for $ty {
                #[inline]
                unsafe fn deallocate_in(self, _alloc: A) {}
            }
        )+
    };
}

impl_clone_in_via_clone!(
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    bool,
    f32,
    f64,
    ()
);
impl_deallocate_in_noop!(
    u8,
    u16,
    u32,
    u64,
    usize,
    i8,
    i16,
    i32,
    i64,
    isize,
    bool,
    f32,
    f64,
    ()
);

/// [`alloc::string::String`] carries its own allocator; `alloc` is ignored.
impl<A: Allocator> CloneIn<A> for ::alloc::string::String {
    #[inline]
    fn clone_in(&self, _alloc: A) -> Self {
        self.clone()
    }
}

impl<A: Allocator> DeallocateIn<A> for ::alloc::string::String {
    #[inline]
    unsafe fn deallocate_in(self, _alloc: A) {
        drop(self);
    }
}
