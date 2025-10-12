use std::{
	ptr,
	sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use super::{MAPLE_NODE_SLOTS, Range};

/// Nodes are either leaves (containing values) or internal nodes (containing child nodes).
pub(crate) enum Node<T> {
	Leaf(LeafNode<T>),
	Internal(InternalNode<T>),
}

impl<T> Node<T> {
	/// Check if this is a leaf node.
	pub fn is_leaf(&self) -> bool {
		matches!(self, Node::Leaf(_))
	}

	/// # Safety
	/// Caller must ensure this is actually a leaf node.
	pub unsafe fn as_leaf(&self) -> &LeafNode<T> {
		match self {
			Node::Leaf(node) => node,
			_ => unsafe { std::hint::unreachable_unchecked() },
		}
	}

	/// # Safety
	/// Caller must ensure this is actually an internal node.
	pub unsafe fn as_internal(&self) -> &InternalNode<T> {
		match self {
			Node::Internal(node) => node,
			_ => unsafe { std::hint::unreachable_unchecked() },
		}
	}
}

/// A leaf node containing values.
#[repr(align(64))]
pub(crate) struct LeafNode<T> {
	/// Number of active slots in this node
	pub count: AtomicUsize,
	/// The ranges covered by each slot
	pub ranges: [Range; MAPLE_NODE_SLOTS],
	/// Values for each range
	pub values: [AtomicPtr<T>; MAPLE_NODE_SLOTS],
}

impl<T> Clone for LeafNode<T> {
	/// The values themselves are not copied, only the pointers.
	/// This is safe because we manage value lifetimes through RCU.
	fn clone(&self) -> Self {
		let count = self.count.load(Ordering::Relaxed);
		let values = [const { AtomicPtr::new(ptr::null_mut()) }; MAPLE_NODE_SLOTS];

		for i in 0..count {
			values[i].store(self.values[i].load(Ordering::Relaxed), Ordering::Relaxed);
		}

		Self {
			ranges: self.ranges,
			values,
			count: AtomicUsize::new(count),
		}
	}
}

impl<T> LeafNode<T> {
	pub fn new() -> Self {
		Self {
			ranges: [Range { start: 0, end: 0 }; MAPLE_NODE_SLOTS],
			values: [const { AtomicPtr::new(ptr::null_mut()) }; MAPLE_NODE_SLOTS],
			count: AtomicUsize::new(0),
		}
	}

	/// Find the position where a new range should be inserted.
	pub fn find_insert_pos(&self, start: usize) -> usize {
		let count = self.count.load(Ordering::Acquire);

		let mut left = 0;
		let mut right = count;

		while left < right {
			let mid = left.midpoint(right);
			if start < self.ranges[mid].start {
				right = mid;
			} else {
				left = mid + 1;
			}
		}
		left
	}

	/// Insert a range and value at a specific index.
	///
	/// Shifts existing entries to make room.
	pub fn insert_at_index(&mut self, idx: usize, range: Range, value: *mut T) {
		let count = self.count.load(Ordering::Acquire);

		// Shift entries to the right
		for i in (idx..count).rev() {
			self.ranges[i + 1] = self.ranges[i];
			self.values[i + 1].store(self.values[i].load(Ordering::Acquire), Ordering::Release);
		}

		// Insert new entry
		self.ranges[idx] = range;
		self.values[idx].store(value, Ordering::Release);
		self.count.fetch_add(1, Ordering::Release);
	}

	/// Update or insert a range and value at a specific index.
	///
	/// If there's an old value, it's freed.
	pub fn insert_or_update(&mut self, range: Range, value: *mut T, idx: usize) {
		self.ranges[idx] = range;

		let old = self.values[idx].swap(value, Ordering::AcqRel);
		if !old.is_null() {
			// SAFETY: Old pointer is valid and we own it
			unsafe {
				let _ = Box::from_raw(old);
			}
		}
	}
}

/// An internal node containing child pointers.
pub(crate) struct InternalNode<T> {
	/// Number of active children
	pub count: AtomicUsize,
	/// The maximum value in each child's subtree
	pub pivots: [AtomicUsize; MAPLE_NODE_SLOTS],
	pub children: [AtomicPtr<Node<T>>; MAPLE_NODE_SLOTS],
}

impl<T> Clone for InternalNode<T> {
	/// The children themselves are not copied, only the pointers.
	/// This is safe because we manage children lifetimes through RCU.
	fn clone(&self) -> Self {
		let count = self.count.load(Ordering::Relaxed);
		let pivots = [const { AtomicUsize::new(0) }; MAPLE_NODE_SLOTS];
		let children = [const { AtomicPtr::new(ptr::null_mut()) }; MAPLE_NODE_SLOTS];

		for i in 0..count {
			if i < count - 1 {
				pivots[i].store(self.pivots[i].load(Ordering::Relaxed), Ordering::Relaxed);
			}
			children[i].store(self.children[i].load(Ordering::Relaxed), Ordering::Relaxed);
		}

		Self {
			pivots,
			children,
			count: AtomicUsize::new(count),
		}
	}
}

impl<T> InternalNode<T> {
	pub const fn new() -> Self {
		Self {
			pivots: [const { AtomicUsize::new(0) }; MAPLE_NODE_SLOTS],
			children: [const { AtomicPtr::new(ptr::null_mut()) }; MAPLE_NODE_SLOTS],
			count: AtomicUsize::new(0),
		}
	}

	/// Find which child should contain the given index.
	pub fn find_child(&self, index: usize) -> usize {
		let count = self.count.load(Ordering::Acquire);

		let mut left = 0;
		let mut right = count;

		while left < right {
			let mid = left.midpoint(right);
			let pivot = self.pivots[mid].load(Ordering::Acquire);

			if index <= pivot {
				right = mid;
			} else {
				left = mid + 1;
			}
		}
		left.min(count.saturating_sub(1))
	}

	/// Insert a child at a specific position.
	pub fn insert_child(&mut self, idx: usize, child: *mut Node<T>, pivot: usize) {
		let count = self.count.load(Ordering::Acquire);

		// Shift entries to the right
		for i in (idx..count).rev() {
			self.children[i + 1].store(self.children[i].load(Ordering::Acquire), Ordering::Release);
			self.pivots[i + 1].store(self.pivots[i].load(Ordering::Acquire), Ordering::Release);
		}

		self.children[idx].store(child, Ordering::Release);
		self.pivots[idx].store(pivot, Ordering::Release);
		self.count.fetch_add(1, Ordering::Release);
	}

	/// Update the pivot value at a specific index if the new value is larger.
	///
	/// This is used when a child's maximum value increases.
	pub fn update_pivot(&mut self, idx: usize, new_max: usize) {
		let count = self.count.load(Ordering::Acquire);
		if idx < count {
			let current = self.pivots[idx].load(Ordering::Acquire);
			if new_max > current {
				self.pivots[idx].store(new_max, Ordering::Release);
			}
		}
	}
}
