use std::{fmt::Display, marker::PhantomData};

const TAG_BITS: usize = 3;
const TAG_MASK: usize = (1 << TAG_BITS) - 1;
const PTR_MASK: usize = !TAG_MASK;

/// A pointer with a tag stored in unused lower bits.
///
/// Pointers are almost always aligned to a multiple of 4/8 meaning the lower 2-3 bits are zeroed,
/// so we can store a tag in those bits without losing information.
#[repr(transparent)]
pub struct TaggedPtr<T> {
	value: usize,
	_marker: PhantomData<*mut T>,
}

impl<T> TaggedPtr<T> {
	/// Creates a new tagged pointer from a pointer and tag.
	///
	/// # Safety
	///
	/// INVARIANT: `ptr` must be aligned to at least `2 ^ TAG_BITS`.
	/// INVARIANT: `tag` must be less than `1 << TAG_BITS`.
	pub fn new(ptr: *mut T, tag: usize) -> Self {
		debug_assert!(tag <= TAG_MASK, "tag({tag}) doesn't fit within {TAG_BITS} bits");
		debug_assert_eq!(
			ptr as usize & TAG_MASK,
			0,
			"ptr({ptr:p}) not aligned to at least 2^{TAG_BITS}"
		);

		Self {
			value: (ptr as usize) | tag,
			_marker: PhantomData,
		}
	}

	pub fn ptr(&self) -> *mut T {
		(self.value & PTR_MASK) as *mut T
	}

	pub fn tag(&self) -> usize {
		self.value & TAG_MASK
	}

	pub fn is_null(&self) -> bool {
		self.ptr().is_null()
	}
}

impl<T> PartialEq for TaggedPtr<T> {
	fn eq(&self, other: &Self) -> bool {
		self.value == other.value
	}
}

impl<T> Display for TaggedPtr<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Tagged")
			.field("ptr", &self.ptr())
			.field("tag", &self.tag())
			.finish()
	}
}

impl<T> Clone for TaggedPtr<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> Copy for TaggedPtr<T> {}
