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
//!
//! Copying allocates a **new** buffer in a caller-supplied allocator via
//! [`CloneIn`] / `clone_in` (safe). Nested unmanaged values are released through
//! [`DeallocateIn`] so a container can free its elements without relying on
//! their panicking [`Drop`] impls.
#![no_std]

extern crate alloc;

pub mod boxed;
pub mod string;
pub mod traits;
pub mod vec;

pub use crate::boxed::UnmanagedBox;
pub use crate::string::String;
pub use crate::string::UnmanagedString;
pub use crate::traits::{CloneIn, DeallocateIn, ToOwnedIn};
pub use crate::vec::UnmanagedVec;
