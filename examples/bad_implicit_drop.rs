//! Forbidden usage: letting an `UnmanagedVec` be dropped implicitly compiles,
//! but panics at runtime because the value cannot free its own buffer.
//!
//! Like `ok_deallocate`, this uses a move-only allocator borrowed by reference
//! to emphasize that the allocator need not be `Copy` or `Clone`.

use ::allocator_api2::alloc::AllocError;
use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::alloc::Global;
use ::core::alloc::Layout;
use ::core::cell::Cell;
use ::core::ptr::NonNull;
use ::unmanaged::UnmanagedVec;

/// A move-only allocator (neither `Copy` nor `Clone`) forwarding to `Global`.
struct CountingAllocator {
    allocations: Cell<usize>,
}

impl CountingAllocator {
    fn new() -> Self {
        Self {
            allocations: Cell::new(0),
        }
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
        // SAFETY: `ptr` was handed out by `Global` via this allocator.
        unsafe { Global.deallocate(ptr, layout) }
    }
}

fn main() {
    let alloc = CountingAllocator::new();

    let mut v = UnmanagedVec::<u32, _>::new(&alloc);
    // SAFETY: `&alloc` refers to the same allocator throughout this program.
    unsafe { v.with_alloc(&alloc) }.push(1);

    // No `deallocate` call: `v` goes out of scope here and its `Drop` impl
    // panics at runtime.
    println!("about to drop `v` implicitly (this will panic)...");
}
