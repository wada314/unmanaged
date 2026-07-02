//! Correct usage: build with an explicit allocator, then release it explicitly.

use ::allocator_api2::alloc::Global;
use ::unmanaged::UnmanagedVec;

fn main() {
    let alloc = Global;

    let mut v = UnmanagedVec::<u32>::new();
    for i in 0..5 {
        v.push_in(i, &alloc);
    }

    assert_eq!(v.len(), 5);
    assert_eq!(&v[..], &[0, 1, 2, 3, 4]);
    assert_eq!(v.pop(), Some(4));

    // The only sanctioned release path.
    v.deallocate(&alloc);

    println!("ok: allocated, used, and deallocated an UnmanagedVec");
}
