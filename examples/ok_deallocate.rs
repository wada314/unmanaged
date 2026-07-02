//! Correct usage: grow through a `with_alloc` guard, then release explicitly.

use ::allocator_api2::alloc::Global;
use ::unmanaged::UnmanagedVec;

fn main() {
    let alloc = Global;

    let mut v = UnmanagedVec::<u32>::new();
    {
        // The guard derefs to a full `Vec<u32, &Global>`.
        let mut g = v.with_alloc(&alloc);
        for i in 0..5 {
            g.push(i);
        }
        g.extend([5, 6, 7]);
    } // guard drops -> raw parts written back into `v`

    assert_eq!(v.len(), 8);
    assert_eq!(&v[..], &[0, 1, 2, 3, 4, 5, 6, 7]);

    // The only sanctioned release path.
    v.deallocate(&alloc);

    println!("ok: grew via guard, then deallocated an UnmanagedVec");
}
