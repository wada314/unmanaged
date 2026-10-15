# AGENTS.md

Instructions for coding agents working in this repository. This file is the [AGENTS.md](https://agents.md/) entry point. Behaviour is documented in rustdoc on `src/lib.rs`.

## Overview

`no_std` allocator-aware containers that **do not store the allocator inline**: `UnmanagedBox`, `UnmanagedVec`, `UnmanagedString`. A parent (for example a puroro message) owns one `A` and passes it on allocate / free.

Because the allocator is not in the value, **implicit `Drop` panics**. Callers must `deallocate` explicitly (or go through [`DeallocateIn`](src/traits.rs) for nested values).

## Build, test, lint

```sh
cargo test
cargo run --example ok_deallocate
cargo fmt
cargo clippy --all-targets
```

`examples/bad_implicit_drop.rs` is expected to panic if run; it documents the forbidden path.

## Constraints

- Do not implement a silent `Drop` that frees memory. The panic-on-drop behaviour is the product.
- Construction / clone / free take the allocator as an argument (`DefaultIn`, `CloneIn`, `deallocate` / `DeallocateIn`). Do not add `Default` that picks `Global` and then ignore the caller's `A`.
- `allocator-api2` is the allocator trait (`default-features = false`, `features = ["alloc"]`).
- Keep the crate `no_std` + `extern crate alloc`.

## Code style

- Imports: `crate::` / `self::` for this crate; other paths start with `::`.
- Comments in English.
