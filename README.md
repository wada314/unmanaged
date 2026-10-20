# unmanaged

`no_std` containers that need `alloc` and keep the allocator out of the value: `UnmanagedBox`, `UnmanagedVec`, and `UnmanagedString`. A parent owns one allocator and shares it across fields, so each field stores only the buffer (a vec or string also stores its length and capacity).

The value cannot free itself. Dropping it panics. Pass the allocator back in when you grow the buffer or release it.

## Install

```toml
[dependencies]
unmanaged = "0.1.0"
allocator-api2 = { version = "0.2", default-features = false, features = ["alloc"] }
```

## Example

`with_alloc` and `deallocate` are `unsafe`: the value does not remember which allocator owns the buffer, so you must pass the same one that created it. The guard gives you a normal `Vec` for the duration of the borrow. Dropping the guard writes the pointer, length, and capacity back. It does not free the buffer.

```rust
use allocator_api2::alloc::Global;
use unmanaged::UnmanagedVec;

fn main() {
    let mut values = UnmanagedVec::<u32, Global>::new(Global);

    {
        // SAFETY: `Global` owns this buffer.
        let mut guard = unsafe { values.with_alloc(Global) };
        guard.extend([1, 2, 3]);
    }

    assert_eq!(&values[..], &[1, 2, 3]);

    // SAFETY: same allocator as `new` and `with_alloc`.
    unsafe { values.deallocate(&Global) };
}
```

A move-only allocator works the same way when you lend `&alloc`, because `&A` implements `Allocator`. That pattern is in `examples/ok_deallocate.rs`. `examples/bad_implicit_drop.rs` panics on purpose by dropping a value without `deallocate`.

Nested unmanaged elements are released by the outer `deallocate` through `DeallocateIn`.

## License

Licensed under the Apache License, Version 2.0.
