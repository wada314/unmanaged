//! An allocator-generic UTF-8 string type ([`String`]) and its instance-less
//! counterpart ([`UnmanagedString`]).
//!
//! Neither `allocator_api2` nor `std` provide an allocator-generic `String`, so
//! this module defines [`String<A>`] as a thin wrapper over
//! [`allocator_api2::vec::Vec<u8, A>`] that maintains the UTF-8 invariant. The
//! instance-less [`UnmanagedString`] mirrors [`crate::UnmanagedVec`]: growth
//! happens through a [`StringGuard`] that derefs to a full `String<A>`.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::alloc::Global;
use ::allocator_api2::vec::Vec;
use ::core::borrow::Borrow;
use ::core::cmp::PartialEq;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::str;

use crate::traits::{CloneIn, DeallocateIn};
use crate::vec::UnmanagedVec;

/// A growable, allocator-generic UTF-8 string, backed by a `Vec<u8, A>`.
pub struct String<A: Allocator = Global> {
    // Invariant: `vec` always holds valid UTF-8.
    vec: Vec<u8, A>,
}

impl<A: Allocator> String<A> {
    /// Creates a new, empty `String` backed by `alloc`. Does not allocate.
    pub fn new_in(alloc: A) -> Self {
        Self {
            vec: Vec::new_in(alloc),
        }
    }

    /// Creates an empty `String` with at least `capacity` bytes reserved.
    pub fn with_capacity_in(capacity: usize, alloc: A) -> Self {
        Self {
            vec: Vec::with_capacity_in(capacity, alloc),
        }
    }

    /// Creates a `String` containing a copy of `s`, backed by `alloc`.
    pub fn from_str_in(s: &str, alloc: A) -> Self {
        let mut vec = Vec::with_capacity_in(s.len(), alloc);
        vec.extend_from_slice(s.as_bytes());
        Self { vec }
    }

    /// Returns the contents as a string slice.
    pub fn as_str(&self) -> &str {
        // SAFETY: `vec` is maintained as valid UTF-8 at all times.
        unsafe { str::from_utf8_unchecked(self.vec.as_slice()) }
    }

    /// Returns the contents as a mutable string slice.
    pub fn as_mut_str(&mut self) -> &mut str {
        // SAFETY: `vec` is valid UTF-8; `&mut str` methods preserve that.
        unsafe { str::from_utf8_unchecked_mut(self.vec.as_mut_slice()) }
    }

    /// Returns the underlying bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.vec.as_slice()
    }

    /// Consumes the string and returns the underlying byte vector.
    pub fn into_bytes(self) -> Vec<u8, A> {
        self.vec
    }

    /// Returns the length in bytes.
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    /// Returns `true` if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    /// Returns the byte capacity without reallocating.
    pub fn capacity(&self) -> usize {
        self.vec.capacity()
    }

    /// Returns a reference to the backing allocator.
    pub fn allocator(&self) -> &A {
        self.vec.allocator()
    }

    /// Appends a character to the end of the string.
    pub fn push(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        self.vec.extend_from_slice(encoded.as_bytes());
    }

    /// Appends a string slice to the end of the string.
    pub fn push_str(&mut self, s: &str) {
        self.vec.extend_from_slice(s.as_bytes());
    }

    /// Removes the last character and returns it, or `None` if empty.
    pub fn pop(&mut self) -> Option<char> {
        let ch = self.as_str().chars().next_back()?;
        let new_len = self.vec.len() - ch.len_utf8();
        self.vec.truncate(new_len);
        Some(ch)
    }

    /// Shortens the string to `new_len` bytes.
    ///
    /// # Panics
    ///
    /// Panics if `new_len` does not lie on a `char` boundary.
    pub fn truncate(&mut self, new_len: usize) {
        if new_len < self.vec.len() {
            assert!(
                self.as_str().is_char_boundary(new_len),
                "new_len does not lie on a char boundary"
            );
            self.vec.truncate(new_len);
        }
    }

    /// Empties the string, keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.vec.clear();
    }

    /// Reserves capacity for at least `additional` more bytes.
    pub fn reserve(&mut self, additional: usize) {
        self.vec.reserve(additional);
    }

    /// Creates a `String` directly from raw parts and an allocator.
    ///
    /// # Safety
    ///
    /// The parts must originate from a `Vec<u8, A>` whose bytes are valid UTF-8,
    /// and `alloc` must own the buffer.
    pub unsafe fn from_raw_parts_in(ptr: *mut u8, len: usize, cap: usize, alloc: A) -> Self {
        let vec = unsafe { Vec::from_raw_parts_in(ptr, len, cap, alloc) };
        Self { vec }
    }

    /// Decomposes the string into raw parts and its allocator, without freeing.
    pub fn into_raw_parts_with_alloc(self) -> (*mut u8, usize, usize, A) {
        self.vec.into_raw_parts_with_alloc()
    }
}

impl String<Global> {
    /// Creates a new, empty `String` backed by the global allocator.
    pub fn new() -> Self {
        Self::new_in(Global)
    }
}

impl Default for String<Global> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Allocator> Deref for String<A> {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<A: Allocator> DerefMut for String<A> {
    fn deref_mut(&mut self) -> &mut str {
        self.as_mut_str()
    }
}

impl<A: Allocator> AsRef<str> for String<A> {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<A: Allocator> AsRef<[u8]> for String<A> {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<A: Allocator> fmt::Debug for String<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl<A: Allocator> fmt::Display for String<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl<A: Allocator, B: Allocator> PartialEq<String<B>> for String<A> {
    fn eq(&self, other: &String<B>) -> bool {
        self.as_str() == other.as_str()
    }
}

impl<A: Allocator> Eq for String<A> {}

impl<A: Allocator> PartialEq<str> for String<A> {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<A: Allocator> PartialEq<&str> for String<A> {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// A contiguous, growable UTF-8 string that does not store an allocator instance.
#[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
pub struct UnmanagedString<A: Allocator> {
    // Invariant: `bytes` always holds valid UTF-8.
    bytes: UnmanagedVec<u8, A>,
}

impl<A: Allocator> UnmanagedString<A> {
    /// Creates a new, empty `UnmanagedString` using `alloc`.
    #[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
    pub fn new(alloc: A) -> Self {
        Self {
            bytes: UnmanagedVec::new(alloc),
        }
    }

    /// Decomposes an owned [`String<A>`] into the instance-less representation.
    pub fn from_string(s: String<A>) -> Self {
        Self {
            bytes: UnmanagedVec::from_vec(s.into_bytes()),
        }
    }

    /// Returns the length of the string in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Allocates a copy of this string in `alloc`.
    ///
    /// The destination buffer is owned by `alloc`; it need not be the same
    /// allocator instance that owns `self`.
    #[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
    pub fn clone_in(&self, alloc: A) -> Self {
        Self::from_string(String::from_str_in(self, alloc))
    }

    /// # Safety
    ///
    /// `alloc` must own this string's buffer.
    pub unsafe fn with_alloc(&mut self, alloc: A) -> StringGuard<'_, A> {
        let (ptr, len, cap) = self.bytes.as_raw_parts();
        let string = unsafe { String::from_raw_parts_in(ptr, len, cap, alloc) };
        StringGuard {
            string: ManuallyDrop::new(string),
            origin: &mut self.bytes,
        }
    }

    /// # Safety
    ///
    /// `alloc` must own this string's buffer.
    pub unsafe fn deallocate(self, alloc: A)
    where
        A: Clone,
    {
        // `u8: DeallocateIn` — byte buffer only; no nested unmanaged elements.
        unsafe { self.bytes.deallocate(alloc) }
    }
}

impl<A: Allocator + Clone> CloneIn<A> for UnmanagedString<A> {
    #[inline]
    fn clone_in(&self, alloc: A) -> Self {
        UnmanagedString::clone_in(self, alloc)
    }
}

impl<A: Allocator + Clone> DeallocateIn<A> for UnmanagedString<A> {
    #[inline]
    unsafe fn deallocate_in(self, alloc: A) {
        // SAFETY: forwarded to the caller's obligation on `alloc`.
        unsafe { self.deallocate(alloc) };
    }
}

impl<A: Allocator> Deref for UnmanagedString<A> {
    type Target = str;

    fn deref(&self) -> &str {
        // SAFETY: `bytes` is maintained as valid UTF-8 at all times.
        unsafe { str::from_utf8_unchecked(&self.bytes) }
    }
}

impl<A: Allocator> PartialEq for UnmanagedString<A> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<A: Allocator> Eq for UnmanagedString<A> {}

impl<A: Allocator> Hash for UnmanagedString<A> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}

impl<A: Allocator> Borrow<str> for UnmanagedString<A> {
    #[inline]
    fn borrow(&self) -> &str {
        self
    }
}

/// Temporary full [`String<A>`] API over an [`UnmanagedString`].
pub struct StringGuard<'a, A: Allocator> {
    string: ManuallyDrop<String<A>>,
    origin: &'a mut UnmanagedVec<u8, A>,
}

impl<'a, A: Allocator> Deref for StringGuard<'a, A> {
    type Target = String<A>;

    fn deref(&self) -> &String<A> {
        &self.string
    }
}

impl<'a, A: Allocator> DerefMut for StringGuard<'a, A> {
    fn deref_mut(&mut self) -> &mut String<A> {
        &mut self.string
    }
}

impl<'a, A: Allocator> Drop for StringGuard<'a, A> {
    fn drop(&mut self) {
        let string = unsafe { ManuallyDrop::take(&mut self.string) };
        let (ptr, len, cap, _alloc) = string.into_raw_parts_with_alloc();
        unsafe { self.origin.set_raw_parts(ptr, len, cap) };
    }
}

#[cfg(test)]
mod tests {
    use super::String;
    use super::UnmanagedString;
    use ::allocator_api2::alloc::Global;

    #[test]
    fn owned_string_basics() {
        let mut s = String::from_str_in("hello", Global);
        s.push(',');
        s.push(' ');
        s.push_str("world");
        assert_eq!(s, "hello, world");
        assert_eq!(s.pop(), Some('d'));
        s.truncate(5);
        assert_eq!(s.as_str(), "hello");
        assert_eq!(&*s, "hello");
    }

    #[test]
    fn build_and_read() {
        let mut s = UnmanagedString::new(Global);
        {
            let mut g = unsafe { s.with_alloc(Global) };
            g.push('h');
            g.push_str("ello, ");
            g.push_str("world");
            g.push('!');
            assert_eq!(g.as_str(), "hello, world!");
            assert_eq!(g.pop(), Some('!'));
            g.truncate(5);
            assert_eq!(&**g, "hello");
        }
        assert_eq!(&*s, "hello");
        assert_eq!(s.len(), 5);
        unsafe { s.deallocate(Global) };
    }

    #[test]
    fn multibyte_pop() {
        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.push_str("aé中");
        {
            let mut g = unsafe { s.with_alloc(Global) };
            assert_eq!(g.pop(), Some('中'));
            assert_eq!(g.pop(), Some('é'));
            assert_eq!(g.pop(), Some('a'));
            assert_eq!(g.pop(), None);
        }
        unsafe { s.deallocate(Global) };
    }

    #[test]
    fn from_string_roundtrip() {
        let owned = String::from_str_in("roundtrip", Global);
        let s = UnmanagedString::from_string(owned);
        assert_eq!(&*s, "roundtrip");
        unsafe { s.deallocate(Global) };
    }

    #[test]
    fn clone_in_copies_contents() {
        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.push_str("clone me");
        let c = s.clone_in(Global);
        assert_eq!(&*c, "clone me");
        assert_eq!(&*s, "clone me");
        unsafe { c.deallocate(Global) };
        unsafe { s.deallocate(Global) };
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.push('x');
    }
}
