# unmanaged

`Vec<T, A>` and `Box<T, A>` store their allocator in the value. A struct that holds many of them stores a separate copy of `A` in every field. When `A` is larger than a zero-sized type, that repetition is wasted space: one allocator owned by the parent could serve every field.

`unmanaged` removes that copy. `UnmanagedBox`, `UnmanagedVec`, and `UnmanagedString` keep the allocator *type*, so allocation and free stay type-checked, but they do not store the allocator *instance*. The parent owns one `A` and passes it in only when a field needs to grow or be freed. What remains in the field is the buffer. A vector or string also stores its length and capacity.

A zero-sized allocator such as `Global` occupies no space either way. The saving appears when `A` carries state, for example an arena.

These types are `no_std` and require `alloc`. The value no longer knows how to free itself, so dropping it panics. Release it with `deallocate`, and pass the same allocator that owns the buffer. Nested unmanaged values are freed by that same call through `DeallocateIn`.

## Install

```toml
[dependencies]
unmanaged = "0.1.0"
allocator-api2 = { version = "0.2", default-features = false, features = ["alloc"] }
```

## Example

`with_alloc` and `deallocate` are `unsafe`: the value does not remember which allocator owns the buffer, so you must pass the same one that created it. The guard is essentially a mutable reference. While it lives, the unmanaged value stays borrowed and you mutate the buffer through the guard, as you would through `&mut Vec<T, A>`. Dropping the guard writes the pointer, length, and capacity back. It does not free the buffer.

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

## License

Licensed under the Apache License, Version 2.0.
