//! Compile-time auto-trait coverage for the public unmanaged types.

use ::allocator_api2::alloc::Global;
use ::unmanaged::{String, UnmanagedBox, UnmanagedString, UnmanagedVec};

#[test]
fn unmanaged_types_are_send_sync_when_parts_are() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<UnmanagedBox<u32, Global>>();
    assert_send_sync::<UnmanagedVec<u32, Global>>();
    assert_send_sync::<UnmanagedString<Global>>();
    assert_send_sync::<String<Global>>();
}
