//! An allocator-generic UTF-8 string type ([`String`]) and its allocator-less
//! counterpart ([`UnmanagedString`]).
//!
//! Neither `allocator_api2` nor `std` provide an allocator-generic `String`, so
//! this module defines [`String<A>`] as a thin wrapper over
//! [`allocator_api2::vec::Vec<u8, A>`] that maintains the UTF-8 invariant. The
//! allocator-less [`UnmanagedString`] then mirrors [`crate::UnmanagedVec`]:
//! growth happens through a [`StringGuard`] that derefs to a full `String<A>`.

use ::allocator_api2::alloc::Allocator;
use ::allocator_api2::alloc::Global;
use ::allocator_api2::vec::Vec;
use ::core::cmp::PartialEq;
use ::core::fmt;
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::str;

use crate::vec::UnmanagedVec;

/// A growable, allocator-generic UTF-8 string, backed by a `Vec<u8, A>`.
///
/// This is a small, `String`-like type that carries its allocator `A` like
/// [`allocator_api2::vec::Vec`] does. Only a commonly-used subset of the
/// standard [`String`](alloc::string::String) API is provided.
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
    /// The parts must originate from a `Vec<u8, A>` (via
    /// [`into_raw_parts_with_alloc`]) whose bytes are valid UTF-8, and `alloc`
    /// must be the allocator that owns the buffer.
    ///
    /// [`into_raw_parts_with_alloc`]: String::into_raw_parts_with_alloc
    pub unsafe fn from_raw_parts_in(ptr: *mut u8, len: usize, cap: usize, alloc: A) -> Self {
        // SAFETY: forwarded to the caller's obligation.
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

/// A contiguous, growable UTF-8 string that does not store its allocator.
///
/// This mirrors [`crate::UnmanagedVec`]: read access is available through
/// [`Deref<Target = str>`], growth happens through a [`StringGuard`] returned by
/// [`UnmanagedString::with_alloc`], and the buffer is released with
/// [`UnmanagedString::deallocate`]. Dropping the value implicitly panics
/// (inherited from the inner `UnmanagedVec`).
#[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
pub struct UnmanagedString {
    // Invariant: `bytes` always holds valid UTF-8.
    bytes: UnmanagedVec<u8>,
}

impl UnmanagedString {
    /// Creates a new, empty `UnmanagedString` using `alloc`.
    ///
    /// `Vec::new_in` does not allocate, so passing `&alloc` keeps a move-only
    /// allocator intact for later use.
    #[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
    pub fn new<A: Allocator>(alloc: A) -> Self {
        Self {
            bytes: UnmanagedVec::new(alloc),
        }
    }

    /// Decomposes an owned [`String<A>`] into the allocator-less representation.
    pub fn from_string<A: Allocator>(s: String<A>) -> Self {
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

    /// Borrows the string together with `alloc`, yielding a guard that derefs to
    /// a full [`String<A>`].
    ///
    /// When the returned [`StringGuard`] is dropped, the raw parts of the
    /// underlying buffer are written back into `self`.
    ///
    /// # Safety
    ///
    /// `alloc` must be the same allocator instance (or an equivalent one that
    /// shares the same allocated blocks) that was used for any previous growth
    /// of this string. Passing a different allocator is undefined behavior.
    pub unsafe fn with_alloc<A: Allocator>(&mut self, alloc: A) -> StringGuard<'_, A> {
        let (ptr, len, cap) = self.bytes.as_raw_parts();
        // SAFETY: the parts describe this string's UTF-8 buffer, and `alloc`
        // satisfies the caller's obligation above.
        let string = unsafe { String::from_raw_parts_in(ptr, len, cap, alloc) };
        StringGuard {
            string: ManuallyDrop::new(string),
            origin: &mut self.bytes,
        }
    }

    /// Drops the contents and frees the backing buffer through `alloc`.
    ///
    /// # Safety
    ///
    /// `alloc` must be the same allocator instance (or an equivalent one that
    /// shares the same allocated blocks) that was used to grow this string.
    pub unsafe fn deallocate<A: Allocator>(self, alloc: A) {
        // SAFETY: forwarded to the caller's obligation on `deallocate`.
        unsafe { self.bytes.deallocate(alloc) }
    }
}

impl Deref for UnmanagedString {
    type Target = str;

    fn deref(&self) -> &str {
        // SAFETY: `bytes` is maintained as valid UTF-8 at all times.
        unsafe { str::from_utf8_unchecked(&self.bytes) }
    }
}

/// A temporary handle that grants the full [`String<A>`] API over an
/// [`UnmanagedString`] by pairing it with an allocator for the duration of a
/// borrow.
///
/// Obtain one via [`UnmanagedString::with_alloc`]. On drop, the current raw
/// parts of the underlying `String` are written back into the originating
/// `UnmanagedString`; the buffer itself is **not** freed.
pub struct StringGuard<'a, A: Allocator> {
    string: ManuallyDrop<String<A>>,
    origin: &'a mut UnmanagedVec<u8>,
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
        // SAFETY: `self.string` is never used again after this take.
        let string = unsafe { ManuallyDrop::take(&mut self.string) };
        let (ptr, len, cap, _alloc) = string.into_raw_parts_with_alloc();
        // SAFETY: parts came from a `String` built from the origin's own parts.
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
            // SAFETY: `Global` is the same (stateless) allocator throughout.
            let mut g = unsafe { s.with_alloc(Global) };
            // Full `String<A>` API is available through the guard.
            g.push('h');
            g.push_str("ello, ");
            g.push_str("world");
            g.push('!');
            assert_eq!(g.as_str(), "hello, world!");
            assert_eq!(g.pop(), Some('!'));
            g.truncate(5);
            assert_eq!(&**g, "hello");
        }
        // Read back without an allocator via `Deref<Target = str>`.
        assert_eq!(&*s, "hello");
        assert_eq!(s.len(), 5);
        // SAFETY: same allocator as used for growth.
        unsafe { s.deallocate(Global) };
    }

    #[test]
    fn multibyte_pop() {
        let mut s = UnmanagedString::new(Global);
        // SAFETY: consistent allocator.
        unsafe { s.with_alloc(Global) }.push_str("aé中");
        {
            // SAFETY: consistent allocator.
            let mut g = unsafe { s.with_alloc(Global) };
            assert_eq!(g.pop(), Some('中'));
            assert_eq!(g.pop(), Some('é'));
            assert_eq!(g.pop(), Some('a'));
            assert_eq!(g.pop(), None);
        }
        // SAFETY: consistent allocator.
        unsafe { s.deallocate(Global) };
    }

    #[test]
    fn from_string_roundtrip() {
        let owned = String::from_str_in("roundtrip", Global);
        let s = UnmanagedString::from_string(owned);
        assert_eq!(&*s, "roundtrip");
        // SAFETY: `owned` was built with `Global`.
        unsafe { s.deallocate(Global) };
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let mut s = UnmanagedString::new(Global);
        // SAFETY: consistent allocator.
        unsafe { s.with_alloc(Global) }.push('x');
        // `s` dropped implicitly at end of scope -> inner UnmanagedVec panics.
    }
}
