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
use ::core::cmp::Ordering;
use ::core::cmp::PartialEq;
use ::core::fmt;
use ::core::hash::{Hash, Hasher};
use ::core::iter::Extend;
use ::core::mem::ManuallyDrop;
use ::core::ops::Deref;
use ::core::ops::DerefMut;
use ::core::str;

use crate::traits::{CloneIn, DeallocateIn, DefaultIn, ToOwnedIn};
use crate::vec::UnmanagedVec;

/// A growable, allocator-generic UTF-8 string, backed by a `Vec<u8, A>`.
///
/// This type stores `A` inline. [`Drop`] frees the buffer through that
/// allocator, and methods such as [`push`](Self::push) and [`reserve`](Self::reserve)
/// use the stored allocator instead of taking one as an argument.
///
/// [`UnmanagedString`] is the instance-less counterpart: it does not store `A`,
/// grows through [`StringGuard`], and panics if dropped.
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

    /// Converts `vec` into a string if it is valid UTF-8.
    ///
    /// On failure, returns the original vector.
    pub fn from_utf8(vec: Vec<u8, A>) -> Result<Self, Vec<u8, A>> {
        match str::from_utf8(&vec) {
            Ok(_) => Ok(Self { vec }),
            Err(_) => Err(vec),
        }
    }

    /// Wraps `vec` as a string without checking UTF-8.
    ///
    /// # Safety
    ///
    /// `vec` must contain valid UTF-8.
    pub unsafe fn from_utf8_unchecked(vec: Vec<u8, A>) -> Self {
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

impl<A: Allocator> Borrow<str> for String<A> {
    #[inline]
    fn borrow(&self) -> &str {
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

impl<A: Allocator> PartialEq<String<A>> for str {
    #[inline]
    fn eq(&self, other: &String<A>) -> bool {
        self == other.as_str()
    }
}

impl<A: Allocator> PartialEq<String<A>> for &str {
    #[inline]
    fn eq(&self, other: &String<A>) -> bool {
        *self == other.as_str()
    }
}

impl<A: Allocator> PartialOrd for String<A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Allocator> Ord for String<A> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl<A: Allocator> Hash for String<A> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl<A> Clone for String<A>
where
    A: Allocator + Clone,
{
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
        }
    }
}

impl<A: Allocator> fmt::Write for String<A> {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }
}

impl<A: Allocator> Extend<char> for String<A> {
    fn extend<I: IntoIterator<Item = char>>(&mut self, iter: I) {
        for ch in iter {
            self.push(ch);
        }
    }
}

impl<'a, A: Allocator> Extend<&'a char> for String<A> {
    fn extend<I: IntoIterator<Item = &'a char>>(&mut self, iter: I) {
        for ch in iter {
            self.push(*ch);
        }
    }
}

impl<'a, A: Allocator> Extend<&'a str> for String<A> {
    fn extend<I: IntoIterator<Item = &'a str>>(&mut self, iter: I) {
        for s in iter {
            self.push_str(s);
        }
    }
}

/// A contiguous, growable UTF-8 string that does not store an allocator instance.
///
/// `Send` and `Sync` follow `A`, matching [`UnmanagedVec`] of `u8`.
///
/// Reading via [`Deref`] to [`prim@str`] needs no allocator. Growth goes through
/// [`with_alloc`](Self::with_alloc), which yields a [`StringGuard`]. [`String`]
/// is the variant that stores the allocator and frees itself on [`Drop`].
///
/// # Dropping
///
/// Dropping implicitly panics. Release the buffer with [`deallocate`](Self::deallocate).
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

    /// Converts `bytes` into a string if it is valid UTF-8.
    ///
    /// On failure, returns the original buffer so the caller can still
    /// [`deallocate`](UnmanagedVec::deallocate) it.
    pub fn from_utf8(bytes: UnmanagedVec<u8, A>) -> Result<Self, UnmanagedVec<u8, A>> {
        match str::from_utf8(&bytes) {
            Ok(_) => Ok(Self { bytes }),
            Err(_) => Err(bytes),
        }
    }

    /// Wraps `bytes` as a string without checking UTF-8.
    ///
    /// # Safety
    ///
    /// [`Deref`] to [`prim@str`] uses `from_utf8_unchecked`. If `bytes` is not
    /// valid UTF-8, later `&str` views violate Rust's UTF-8 invariant.
    /// protobuf `utf8_validation=NONE` decode uses this to copy wire bytes
    /// unchanged.
    pub unsafe fn from_utf8_unchecked(bytes: UnmanagedVec<u8, A>) -> Self {
        Self { bytes }
    }

    /// Returns the length of the string in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns `true` if the string is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Returns the byte capacity without reallocating.
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }

    /// Consumes the string and returns the underlying byte buffer.
    ///
    /// The caller must still [`deallocate`](UnmanagedVec::deallocate) the buffer.
    pub fn into_bytes(self) -> UnmanagedVec<u8, A> {
        self.bytes
    }

    /// Rebuilds an allocator-carrying [`String`] from this value.
    ///
    /// # Safety
    ///
    /// `alloc` must own this string's buffer. The bytes must be valid UTF-8,
    /// which holds unless this value was built with [`Self::from_utf8_unchecked`].
    pub unsafe fn into_string(self, alloc: A) -> String<A> {
        let vec = unsafe { self.into_bytes().into_vec(alloc) };
        String { vec }
    }

    /// Allocates a copy of this string in `alloc`.
    ///
    /// The destination buffer is owned by `alloc`; it need not be the same
    /// allocator instance that owns `self`.
    #[must_use = "an UnmanagedString must be released via `deallocate`; dropping it panics"]
    pub fn clone_in(&self, alloc: A) -> Self {
        <str as ToOwnedIn<A>>::to_owned_in(self, alloc)
    }

    /// Borrows the string together with `alloc`, yielding a [`StringGuard`].
    ///
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

    /// Frees this string's buffer.
    ///
    /// # Safety
    ///
    /// `alloc` must own this string's buffer.
    pub unsafe fn deallocate(self, alloc: &A) {
        // `u8: DeallocateIn` — byte buffer only; no nested unmanaged elements.
        unsafe { self.bytes.deallocate(alloc) }
    }
}

impl<A: Allocator> ToOwnedIn<A> for str {
    type Owned = UnmanagedString<A>;

    #[inline]
    fn to_owned_in(&self, alloc: A) -> UnmanagedString<A> {
        UnmanagedString::from_string(String::from_str_in(self, alloc))
    }
}

impl<A: Allocator> DefaultIn<A> for UnmanagedString<A> {
    #[inline]
    fn default_in(alloc: A) -> Self {
        UnmanagedString::new(alloc)
    }
}

impl<A: Allocator + Clone> CloneIn<A> for UnmanagedString<A> {
    #[inline]
    fn clone_in(&self, alloc: A) -> Self {
        UnmanagedString::clone_in(self, alloc)
    }
}

impl<A: Allocator> DeallocateIn<A> for UnmanagedString<A> {
    #[inline]
    unsafe fn deallocate_in(self, alloc: &A) {
        // SAFETY: forwarded to the caller's obligation on `alloc`.
        unsafe { self.deallocate(alloc) };
    }
}

impl<A: Allocator> Deref for UnmanagedString<A> {
    type Target = str;

    fn deref(&self) -> &str {
        // SAFETY: `from_utf8` keeps UTF-8. `from_utf8_unchecked` (protobuf
        // `utf8_validation=NONE`) may store non-UTF-8 bytes presented as `&str`.
        unsafe { str::from_utf8_unchecked(&self.bytes) }
    }
}

impl<A: Allocator> fmt::Debug for UnmanagedString<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
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

impl<A: Allocator> AsRef<str> for UnmanagedString<A> {
    #[inline]
    fn as_ref(&self) -> &str {
        self
    }
}

impl<A: Allocator> AsRef<[u8]> for UnmanagedString<A> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<A: Allocator> fmt::Display for UnmanagedString<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<A: Allocator> PartialEq<str> for UnmanagedString<A> {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        &**self == other
    }
}

impl<A: Allocator> PartialEq<&str> for UnmanagedString<A> {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        &**self == *other
    }
}

impl<A: Allocator> PartialEq<UnmanagedString<A>> for str {
    #[inline]
    fn eq(&self, other: &UnmanagedString<A>) -> bool {
        self == &**other
    }
}

impl<A: Allocator> PartialEq<UnmanagedString<A>> for &str {
    #[inline]
    fn eq(&self, other: &UnmanagedString<A>) -> bool {
        *self == &**other
    }
}

impl<A: Allocator> PartialOrd for UnmanagedString<A> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<A: Allocator> Ord for UnmanagedString<A> {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        (**self).cmp(&**other)
    }
}

/// Temporary full [`String<A>`] API over an [`UnmanagedString`].
///
/// On drop, the guard writes the buffer's pointer, length, and capacity back
/// into the [`UnmanagedString`]. It does not free the buffer. [`String::clear`]
/// and [`String::truncate`] shorten that same buffer while the guard is alive.
pub struct StringGuard<'a, A: Allocator> {
    string: ManuallyDrop<String<A>>,
    origin: &'a mut UnmanagedVec<u8, A>,
}

impl<A: Allocator> Deref for StringGuard<'_, A> {
    type Target = String<A>;

    fn deref(&self) -> &String<A> {
        &self.string
    }
}

impl<A: Allocator> DerefMut for StringGuard<'_, A> {
    fn deref_mut(&mut self) -> &mut String<A> {
        &mut self.string
    }
}

impl<A: Allocator> Drop for StringGuard<'_, A> {
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
    use ::allocator_api2::vec::Vec;
    use ::core::hash::Hasher;

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
        unsafe { s.deallocate(&Global) };
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
        unsafe { s.deallocate(&Global) };
    }

    #[test]
    fn from_string_roundtrip() {
        let owned = String::from_str_in("roundtrip", Global);
        let s = UnmanagedString::from_string(owned);
        assert_eq!(&*s, "roundtrip");
        unsafe { s.deallocate(&Global) };
    }

    #[test]
    fn clone_in_copies_contents() {
        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.push_str("clone me");
        let c = s.clone_in(Global);
        assert_eq!(&*c, "clone me");
        assert_eq!(&*s, "clone me");
        unsafe { c.deallocate(&Global) };
        unsafe { s.deallocate(&Global) };
    }

    #[test]
    #[should_panic(expected = "must be released via `deallocate`")]
    fn implicit_drop_panics() {
        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.push('x');
    }

    #[test]
    fn send_sync_when_allocator_is() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<String<Global>>();
        assert_send_sync::<UnmanagedString<Global>>();
    }

    #[test]
    fn owned_string_matches_std_traits() {
        use ::core::fmt::Write;
        use ::core::hash::Hash;

        let mut s = String::from_str_in("hi", Global);
        assert!("hi" == s);
        assert!(s < String::from_str_in("hj", Global));
        let cloned = s.clone();
        assert_eq!(cloned, "hi");

        s.extend(['!']);
        s.extend([&'x']);
        s.extend(["y"]);
        write!(s, "{n}", n = 1).unwrap();
        assert_eq!(s, "hi!xy1");

        let mut hasher_s = HasherStub(0);
        let mut hasher_lit = HasherStub(0);
        s.hash(&mut hasher_s);
        s.as_str().hash(&mut hasher_lit);
        assert_eq!(hasher_s.finish(), hasher_lit.finish());

        let mut raw = Vec::new_in(Global);
        raw.extend_from_slice(b"ok");
        let ok = String::from_utf8(raw).unwrap();
        assert_eq!(ok, "ok");
        let mut bad = Vec::new_in(Global);
        bad.push(0xff);
        assert!(String::from_utf8(bad).is_err());
        let mut raw = Vec::new_in(Global);
        raw.extend_from_slice(b"ok");
        let unchecked = unsafe { String::from_utf8_unchecked(raw) };
        assert_eq!(unchecked, "ok");
    }

    #[test]
    fn unmanaged_string_cmp_capacity_and_into() {
        use ::alloc::format;

        let mut s = UnmanagedString::new(Global);
        unsafe { s.with_alloc(Global) }.reserve(8);
        unsafe { s.with_alloc(Global) }.push_str("hi");
        assert!(s.capacity() >= 8);
        assert!(s == "hi");
        assert!("hi" == s);
        assert_eq!(format!("{s}"), "hi");
        let later = UnmanagedString::from_string(String::from_str_in("hj", Global));
        assert!(s < later);

        let bytes = s.into_bytes();
        assert_eq!(&bytes[..], b"hi");
        unsafe { bytes.deallocate(&Global) };

        let owned = unsafe { later.into_string(Global) };
        assert_eq!(owned, "hj");
    }

    struct HasherStub(u64);

    impl Hasher for HasherStub {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(u64::from(*byte));
            }
        }
    }
}
