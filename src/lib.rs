//! `unmanaged` provides heap-allocated collection types that do **not** store
//! their allocator inline. The allocator is supplied explicitly on every
//! operation that (de)allocates, so a parent struct can own a single allocator
//! and share it across many collection fields, reducing per-field memory
//! footprint.
//!
//! Because the allocator is absent from the collection itself, these types
//! cannot free themselves through the normal [`Drop`] mechanism. Instead they
//! must be released explicitly via a `deallocate(self, alloc)` method. Dropping
//! such a value implicitly panics at runtime (see [`vec`]).
#![no_std]

extern crate alloc;

pub mod vec;

pub use crate::vec::UnmanagedVec;
