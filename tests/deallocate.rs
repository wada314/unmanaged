//! Ownership paths that do not show up in ordinary `Deref` use:
//! paired allocate/deallocate through a move-only allocator, freeing a buffer
//! after its elements were removed, and UTF-8 conversions that must not drop
//! the caller's bytes.

use ::allocator_api2::alloc::AllocError;
use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::alloc::Global;
use ::core::alloc::Layout;
use ::core::cell::Cell;
use ::core::ptr::NonNull;
use ::unmanaged::DeallocateIn;
use ::unmanaged::DefaultIn;
use ::unmanaged::String;
use ::unmanaged::UnmanagedBox;
use ::unmanaged::UnmanagedString;
use ::unmanaged::UnmanagedVec;

/// Forwards to `Global` and counts allocate/deallocate calls.
/// Deliberately neither `Copy` nor `Clone`. Callers pass `&Self`.
struct CountingAllocator {
    allocations: Cell<usize>,
    deallocations: Cell<usize>,
}

impl CountingAllocator {
    fn new() -> Self {
        Self {
            allocations: Cell::new(0),
            deallocations: Cell::new(0),
        }
    }

    fn assert_balanced(&self) {
        assert_eq!(
            self.allocations.get(),
            self.deallocations.get(),
            "allocations {}, deallocations {}",
            self.allocations.get(),
            self.deallocations.get()
        );
    }
}

// SAFETY: every allocation is obtained from `Global` and freed with the same
// layout through `Global`; this type only wraps that allocator.
unsafe impl Allocator for CountingAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocations.set(self.allocations.get() + 1);
        Global.allocate(layout)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocations.set(self.deallocations.get() + 1);
        // SAFETY: `ptr` was handed out by `Global` via this allocator.
        unsafe { Global.deallocate(ptr, layout) }
    }
}

#[test]
fn growth_and_nested_deallocate_free_every_buffer() {
    let alloc = CountingAllocator::new();

    let empty = UnmanagedVec::<u32, _>::new(&alloc);
    // SAFETY: same allocator instance, and `new` did not allocate.
    unsafe { empty.deallocate(&&alloc) };
    assert_eq!(alloc.allocations.get(), 0);
    alloc.assert_balanced();

    let mut v = UnmanagedVec::<u32, _>::new(&alloc);
    {
        // SAFETY: `&alloc` is the allocator that owns this buffer.
        let mut g = unsafe { v.with_alloc(&alloc) };
        for i in 0..64 {
            g.push(i);
        }
    }
    assert!(alloc.allocations.get() > 0);
    // SAFETY: same allocator used for every push above.
    unsafe { v.deallocate(&&alloc) };
    alloc.assert_balanced();

    let before = alloc.allocations.get();
    let mut nested = UnmanagedVec::<UnmanagedString<&CountingAllocator>, _>::new(&alloc);
    {
        // SAFETY: `&alloc` owns this vector and will own each pushed string.
        let mut g = unsafe { nested.with_alloc(&alloc) };
        g.push(UnmanagedString::from_string(String::from_str_in(
            "a", &alloc,
        )));
        g.push(UnmanagedString::from_string(String::from_str_in(
            "bb", &alloc,
        )));
    }
    assert!(alloc.allocations.get() > before);
    // One call must free both string buffers and the vector buffer.
    // SAFETY: same allocator used to build every buffer above.
    unsafe { nested.deallocate(&&alloc) };
    alloc.assert_balanced();

    let inner = UnmanagedString::<&CountingAllocator>::default_in(&alloc);
    let boxed = UnmanagedBox::new_in(inner, &alloc);
    let cloned = boxed.clone_in(&alloc);
    assert_eq!(&*cloned, "");
    // SAFETY: `cloned` was allocated by `&alloc`.
    unsafe { cloned.deallocate(&&alloc) };
    // SAFETY: `boxed` was allocated by `&alloc`.
    unsafe { boxed.deallocate(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn deallocate_buffer_after_elements_are_removed() {
    let alloc = CountingAllocator::new();
    let mut v = UnmanagedVec::<UnmanagedString<&CountingAllocator>, _>::new(&alloc);
    let popped = {
        // SAFETY: `&alloc` owns this vector.
        let mut g = unsafe { v.with_alloc(&alloc) };
        g.push(UnmanagedString::from_string(String::from_str_in(
            "gone", &alloc,
        )));
        g.pop().unwrap()
    };
    assert_eq!(&*popped, "gone");
    assert_eq!(v.len(), 0);
    // SAFETY: `popped` was allocated by `&alloc`.
    unsafe { popped.deallocate(&&alloc) };
    // SAFETY: `&alloc` owns `v`, and `pop` left it empty.
    unsafe { v.deallocate_buffer(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn utf8_failure_returns_the_buffer_and_unchecked_keeps_bytes() {
    let alloc = CountingAllocator::new();

    let mut bytes = UnmanagedVec::<u8, _>::new(&alloc);
    // SAFETY: `&alloc` owns this buffer.
    unsafe { bytes.with_alloc(&alloc) }.extend_from_slice(&[0xff, 0xfe]);
    let err = UnmanagedString::from_utf8(bytes).unwrap_err();
    assert_eq!(&err[..], &[0xff, 0xfe]);
    // SAFETY: `&alloc` owns the rejected buffer.
    unsafe { err.deallocate(&&alloc) };

    let mut raw = UnmanagedVec::<u8, _>::new(&alloc);
    // SAFETY: `&alloc` owns this buffer.
    unsafe { raw.with_alloc(&alloc) }.extend_from_slice(&[0xff]);
    // Do not view this as `&str`: the bytes are not UTF-8.
    assert!(::core::str::from_utf8(&raw).is_err());
    // SAFETY: caller opts out of the UTF-8 check; we only read the bytes back.
    let unchecked = unsafe { UnmanagedString::from_utf8_unchecked(raw) };
    let back = unchecked.into_bytes();
    assert_eq!(&back[..], &[0xff]);
    // SAFETY: `&alloc` owns this buffer.
    unsafe { back.deallocate(&&alloc) };

    alloc.assert_balanced();
}

#[test]
fn deallocate_frees_a_reserved_empty_buffer() {
    let alloc = CountingAllocator::new();
    let mut v = UnmanagedVec::<u32, _>::new(&alloc);
    // SAFETY: `&alloc` owns this buffer.
    unsafe { v.with_alloc(&alloc) }.reserve(32);
    assert_eq!(v.len(), 0);
    assert!(v.capacity() >= 32);
    // SAFETY: `&alloc` owns this buffer, and it holds no elements.
    unsafe { v.deallocate(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn deallocate_buffer_after_growth_and_pop() {
    let alloc = CountingAllocator::new();
    let mut v = UnmanagedVec::<u32, _>::new(&alloc);
    {
        // SAFETY: `&alloc` owns this buffer.
        let mut g = unsafe { v.with_alloc(&alloc) };
        for i in 0..64u32 {
            g.push(i);
        }
        while g.pop().is_some() {}
    }
    assert_eq!(v.len(), 0);
    assert!(v.capacity() >= 64);
    // SAFETY: `&alloc` owns `v`, and every element was popped.
    unsafe { v.deallocate_buffer(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn unchecked_valid_utf8_grows_through_with_alloc() {
    let alloc = CountingAllocator::new();
    let mut raw = UnmanagedVec::<u8, _>::new(&alloc);
    // SAFETY: `&alloc` owns this buffer.
    unsafe { raw.with_alloc(&alloc) }.extend_from_slice(b"hello");
    assert!(::core::str::from_utf8(&raw).is_ok());
    // SAFETY: `raw` is valid UTF-8, checked above.
    let mut text = unsafe { UnmanagedString::from_utf8_unchecked(raw) };
    assert_eq!(&*text, "hello");
    {
        // SAFETY: `&alloc` owns this buffer, and the bytes stay valid UTF-8.
        let mut g = unsafe { text.with_alloc(&alloc) };
        g.push_str(", world");
        g.push('!');
    }
    assert_eq!(&*text, "hello, world!");
    // SAFETY: `&alloc` owns this buffer.
    unsafe { text.deallocate(&&alloc) };

    let mut owned_raw = ::allocator_api2::vec::Vec::<u8, _>::new_in(&alloc);
    owned_raw.extend_from_slice(b"ok");
    assert!(::core::str::from_utf8(&owned_raw).is_ok());
    // SAFETY: `owned_raw` is valid UTF-8, checked above.
    let owned = unsafe { String::from_utf8_unchecked(owned_raw) };
    assert_eq!(owned.as_str(), "ok");
    let transferred = UnmanagedString::from_string(owned);
    // SAFETY: `&alloc` owns the buffer transferred out of `owned`.
    unsafe { transferred.deallocate(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn default_in_vec_and_box_balance() {
    let alloc = CountingAllocator::new();

    let empty_vec = UnmanagedVec::<u32, _>::default_in(&alloc);
    assert!(empty_vec.is_empty());
    // SAFETY: `default_in` used `&alloc` and did not allocate for an empty vec.
    unsafe { empty_vec.deallocate(&&alloc) };
    assert_eq!(alloc.allocations.get(), 0);

    let boxed = UnmanagedBox::<u32, _>::default_in(&alloc);
    assert_eq!(*boxed, 0);
    // SAFETY: box was allocated through `&alloc`.
    unsafe { boxed.deallocate(&&alloc) };
    alloc.assert_balanced();
}

#[test]
fn clone_in_to_a_different_allocator_instance() {
    let alloc_a = CountingAllocator::new();
    let alloc_b = CountingAllocator::new();

    let mut src = UnmanagedVec::<u32, _>::new(&alloc_a);
    {
        // SAFETY: `&alloc_a` owns `src`.
        let mut g = unsafe { src.with_alloc(&alloc_a) };
        g.extend([1, 2, 3]);
    }
    let dst = src.clone_in(&alloc_b);
    assert_eq!(&dst[..], &[1, 2, 3]);

    // SAFETY: each vec is freed through the allocator that owns its buffer.
    unsafe { src.deallocate(&&alloc_a) };
    // SAFETY: `dst` was `clone_in`'d into `&alloc_b`.
    unsafe { dst.deallocate(&&alloc_b) };
    alloc_a.assert_balanced();
    alloc_b.assert_balanced();
}

#[test]
fn deallocate_in_trait_frees_box() {
    let alloc = CountingAllocator::new();
    let boxed = UnmanagedBox::new_in(7u32, &alloc);
    // SAFETY: `&alloc` owns the box; trait path must free the same way.
    unsafe { DeallocateIn::deallocate_in(boxed, &&alloc) };
    alloc.assert_balanced();
}

#[test]
// The popped string panics inside `Drop` and never reaches `deallocate()`.
#[cfg_attr(miri, ignore = "implicit Drop panics before the buffer is freed")]
#[should_panic(expected = "must be released via `deallocate`")]
fn popped_unmanaged_value_still_panics_on_drop() {
    let alloc = CountingAllocator::new();
    let mut v = UnmanagedVec::<UnmanagedString<&CountingAllocator>, _>::new(&alloc);
    let popped = {
        // SAFETY: `&alloc` owns this vector.
        let mut g = unsafe { v.with_alloc(&alloc) };
        g.push(UnmanagedString::from_string(String::from_str_in(
            "x", &alloc,
        )));
        g.pop().unwrap()
    };
    // The vector buffer is empty and must be released before `popped` drops,
    // or the second panicking `Drop` aborts the process.
    assert_eq!(v.len(), 0);
    // SAFETY: `&alloc` owns `v`, and `pop` left it empty (checked above).
    unsafe { v.deallocate_buffer(&&alloc) };
    let _ = popped;
}
