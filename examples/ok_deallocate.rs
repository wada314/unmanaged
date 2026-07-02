//! Correct usage: grow through a `with_alloc` guard, then release explicitly.
//!
//! This example also demonstrates that the allocator type need **not** be
//! `Copy` or `Clone`. We own a single move-only allocator and lend it out by
//! reference (`&A` implements `Allocator` for any `A: Allocator`).

use ::allocator_api2::alloc::AllocError;
use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::alloc::Global;
use ::core::alloc::Layout;
use ::core::cell::Cell;
use ::core::ptr::NonNull;
use ::unmanaged::UnmanagedVec;

/// A minimal allocator that forwards to `Global` while counting allocations.
/// It is deliberately neither `Copy` nor `Clone`, proving that `UnmanagedVec`
/// works with move-only allocators borrowed by reference.
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
    // A single, move-only allocator instance.
    let alloc = CountingAllocator::new();

    let mut v = UnmanagedVec::<u32>::new(&alloc);
    {
        // The guard derefs to a full `Vec<u32, &CountingAllocator>`: the
        // allocator is only *borrowed*, never cloned or copied.
        // SAFETY: `&alloc` refers to the same allocator throughout this program.
        let mut g = unsafe { v.with_alloc(&alloc) };
        for i in 0..5 {
            g.push(i);
        }
        g.extend([5, 6, 7]);
    } // guard drops -> raw parts written back into `v`

    assert_eq!(v.len(), 8);
    assert_eq!(&v[..], &[0, 1, 2, 3, 4, 5, 6, 7]);

    // Release through the same borrowed allocator.
    // SAFETY: same allocator instance used for all growth above.
    unsafe { v.deallocate(&alloc) };

    println!(
        "ok: grew via guard and deallocated; allocator ran {} allocation(s)",
        alloc.allocations.get()
    );
}
