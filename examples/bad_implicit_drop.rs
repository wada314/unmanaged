//! Forbidden usage: letting an `UnmanagedVec` be dropped implicitly compiles,
//! but panics at runtime because the value cannot free its own buffer.

use ::allocator_api2::alloc::Global;
use ::unmanaged::UnmanagedVec;

fn main() {
    let alloc = Global;

    let mut v = UnmanagedVec::<u32>::new();
    v.with_alloc(&alloc).push(1);

    // No `deallocate` call: `v` goes out of scope here and its `Drop` impl
    // panics at runtime.
    println!("about to drop `v` implicitly (this will panic)...");
}
