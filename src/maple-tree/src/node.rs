use std::{
	marker::PhantomData,
	mem::{ManuallyDrop, MaybeUninit},
};

use crate::tagged::TaggedPtr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeKind {
	Leaf64,
	Range64,
}

pub union Slot<K, V, const CAPACITY: usize> {
	pub value: ManuallyDrop<V>,
	pub child: ManuallyDrop<TaggedPtr<Node<K, V, CAPACITY>>>,
}

#[repr(C)]
pub struct NodeHeader<K, V, const CAPACITY: usize> {
	pub parent: *mut Node<K, V, CAPACITY>,
	pub parent_slot: u8,
	node_type_and_meta: u8,
	_marker: PhantomData<V>,
}

impl<K, V, const CAPACITY: usize> NodeHeader<K, V, CAPACITY> {
	const NODE_KIND_BITS: u8 = 3;
	const NODE_KIND_MASK: u8 = (1 << Self::NODE_KIND_BITS) - 1;
	const SLOT_COUNT_MASK: u8 = !Self::NODE_KIND_MASK;

	#[inline(always)]
	pub fn node_kind(&self) -> NodeKind {
		// SAFETY: You can only set the node type to a valid value
		unsafe { std::mem::transmute(self.node_type_and_meta & Self::NODE_KIND_MASK) }
	}

	#[inline(always)]
	pub fn set_node_kind(&mut self, kind: NodeKind) {
		debug_assert!(
			(kind as u8) <= Self::NODE_KIND_MASK,
			"NodeKind value must fit in {} bits",
			Self::NODE_KIND_BITS
		);
		self.node_type_and_meta = (self.node_type_and_meta & Self::SLOT_COUNT_MASK) | (kind as u8);
	}

	#[inline(always)]
	pub fn slot_count(&self) -> u8 {
		self.node_type_and_meta >> Self::NODE_KIND_BITS
	}

	#[inline(always)]
	pub fn set_slot_count(&mut self, count: u8) {
		debug_assert!(
			count <= (Self::SLOT_COUNT_MASK >> Self::NODE_KIND_BITS),
			"Slot count must fit in {} bits. Max value: {}",
			8 - Self::NODE_KIND_BITS,
			Self::SLOT_COUNT_MASK >> Self::NODE_KIND_BITS
		);
		self.node_type_and_meta = (self.node_type_and_meta & Self::NODE_KIND_MASK) | (count << Self::NODE_KIND_BITS);
	}
}

/// Represents a node in the tree.
///
/// # Safety
///
/// INVARIANT: The first `header.slot_count()` elements of `pivots` and `slots` must be initialized.
#[repr(C)]
pub struct Node<K, V, const CAPACITY: usize> {
	pub header: NodeHeader<K, V, CAPACITY>,
	pivots: [MaybeUninit<K>; CAPACITY],
	slots: [MaybeUninit<Slot<K, V, CAPACITY>>; CAPACITY],
}

impl<K, V, const CAPACITY: usize> Node<K, V, CAPACITY> {
	/// Returns a slice of the initialized pivot keys.
	///
	/// # Safety
	///
	/// INVARIANT: The first `header.slot_count()` elements of `pivots` must be initialized.
	pub fn pivots(&self) -> &[K] {
		let mut count = self.header.slot_count() as usize;

		// Internal nodes have one less pivot than slot
		if self.header.node_kind() != NodeKind::Leaf64 {
			count = count.saturating_sub(1);
		}

		// SAFETY: `self.pivots` contains only initialized values as guaranteed by the invariant.
		// SAFETY: `MaybeUninit<K>` and `K` have the same layout. so the cast is safe.
		unsafe { std::slice::from_raw_parts(self.pivots.as_ptr().cast::<K>(), count) }
	}

	/// Returns a mutable slice of the initialized pivot keys.
	///
	/// # Safety
	///
	/// INVARIANT: The first `header.slot_count()` elements of `pivots` must be initialized.
	pub fn pivots_mut(&mut self) -> &mut [K] {
		let mut count = self.header.slot_count() as usize;

		// Internal nodes have one less pivot than slot
		if self.header.node_kind() != NodeKind::Leaf64 {
			count = count.saturating_sub(1);
		}

		// SAFETY: `self.pivots` contains only initialized values as guaranteed by the invariant.
		// SAFETY: `MaybeUninit<u64>` and `u64` have the same layout. so the cast is safe.
		unsafe { std::slice::from_raw_parts_mut(self.pivots.as_mut_ptr().cast::<K>(), count) }
	}

	/// Returns a slice of the initialized slots.
	///
	/// # Safety
	///
	/// INVARIANT: The first `header.slot_count()` elements of `slots` must be initialized.
	pub fn slots(&self) -> &[Slot<K, V, CAPACITY>] {
		let count = self.header.slot_count() as usize;
		// SAFETY: `self.slots` contains only initialized values as guaranteed by the invariant.
		// SAFETY: `MaybeUninit<Slot<K, V, CAPACITY>>` and `Slot<K, V, CAPACITY>` have the same layout. so the cast is safe.
		unsafe { std::slice::from_raw_parts(self.slots.as_ptr().cast::<Slot<K, V, CAPACITY>>(), count) }
	}

	/// Returns a mutable slice of the initialized slots.
	///
	/// # Safety
	///
	/// INVARIANT: The first `header.slot_count()` elements of `slots` must be initialized.
	pub fn slots_mut(&mut self) -> &mut [Slot<K, V, CAPACITY>] {
		let count = self.header.slot_count() as usize;
		// SAFETY: `self.slots` contains only initialized values as guaranteed by the invariant.
		// SAFETY: `MaybeUninit<Slot<K, V, CAPACITY>>` and `Slot<K, V, CAPACITY>` have the same layout. so the cast is safe.
		unsafe { std::slice::from_raw_parts_mut(self.slots.as_mut_ptr().cast::<Slot<K, V, CAPACITY>>(), count) }
	}

	pub fn pivots_and_slots(&self) -> (&[K], &[Slot<K, V, CAPACITY>]) {
		(self.pivots(), self.slots())
	}

	pub fn pivots_and_slots_raw_mut(
		&mut self,
	) -> (
		&mut [MaybeUninit<K>; CAPACITY],
		&mut [MaybeUninit<Slot<K, V, CAPACITY>>; CAPACITY],
	) {
		(&mut self.pivots, &mut self.slots)
	}

	pub fn pivots_raw_mut(&mut self) -> &mut [MaybeUninit<K>; CAPACITY] {
		&mut self.pivots
	}

	pub fn slots_raw_mut(&mut self) -> &mut [MaybeUninit<Slot<K, V, CAPACITY>>; CAPACITY] {
		&mut self.slots
	}
}
