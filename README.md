# unmanaged

`no_std` heap containers that need `alloc` and do not store the allocator inline: `UnmanagedBox`, `UnmanagedVec`, and `UnmanagedString`. A parent owns one allocator and passes it in on allocate and free.

Implicit `Drop` panics. Call `deallocate` explicitly, or `DeallocateIn` for nested values. Growth goes through `with_alloc`, which borrows the allocator for the duration of the guard.

`with_alloc` and `deallocate` are `unsafe`. Pass the same allocator that owns the buffer. A move-only allocator can be lent as `&alloc`, because `&A` implements `Allocator` when `A` does.

```toml
[dependencies]
unmanaged = "0.1.0"
allocator-api2 = { version = "0.2", default-features = false, features = ["alloc"] }
```

See the crate rustdoc and `examples/ok_deallocate.rs`. `examples/bad_implicit_drop.rs` panics on purpose.

## License

Licensed under the Apache License, Version 2.0.
