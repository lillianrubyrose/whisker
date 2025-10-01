#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

use std::{
	mem::{ManuallyDrop, MaybeUninit},
	sync::atomic::Ordering,
};

use crossbeam_epoch::{Atomic, Guard, Owned, Shared};
use spin::Mutex;

use crate::{
	arena::NodeArena,
	node::{Node, NodeKind, Slot},
	tagged::TaggedPtr,
};

pub mod arena;
pub mod node;
pub mod tagged;

// Branching factor for standard nodes.
pub const RANGE64_SLOTS: usize = 16;
pub const RANGE64_PIVOTS: usize = RANGE64_SLOTS - 1;

pub trait SupportedCapacity {}

macro_rules! supported_capacity {
    ($($capacity:literal),+) => {
        $(
            impl<K, V> SupportedCapacity for MapleTree<K, V, $capacity> {}
        )+
    };
}

supported_capacity!(
	1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
);

pub enum NodePtr<K, V, const CAPACITY: usize> {
	Empty,
	Direct(K, TaggedPtr<V>),
	Node(TaggedPtr<Node<K, V, CAPACITY>>),
}

pub struct MapleTree<K, V, const CAPACITY: usize> {
	root: Atomic<NodePtr<K, V, CAPACITY>>,
	arena: NodeArena<K, V, CAPACITY>,
	lock: Mutex<()>,
}

unsafe impl<K, V, const CAPACITY: usize> Send for MapleTree<K, V, CAPACITY> where Self: SupportedCapacity {}
unsafe impl<K, V, const CAPACITY: usize> Sync for MapleTree<K, V, CAPACITY> where Self: SupportedCapacity {}

impl<K, V, const CAPACITY: usize> MapleTree<K, V, CAPACITY>
where
	Self: SupportedCapacity,
	K: PartialEq + PartialOrd + Ord + Copy,
	[(); CAPACITY + 1]:,
{
	pub fn new() -> Self {
		Self {
			root: Atomic::new(NodePtr::Empty),
			arena: NodeArena::new(),
			lock: Mutex::new(()),
		}
	}

	pub fn load<'guard>(&self, index: K, guard: &'guard Guard) -> Option<&'guard V>
	where
		K: 'guard,
	{
		let root = self.root.load(Ordering::Relaxed, guard);

		// SAFETY: The `root` pointer is loaded from an `Atomic` pointer that's only ever updated by `store`.
		// `store` always allocates memory for new nodes and values through `NodeArena`, ensuring the pointer is valid.
		// The `guard` ensures that the memory will not be freed for the lifetime of the guard.
		match unsafe { root.as_ref() } {
			Some(NodePtr::Direct(stored_index, ptr)) => {
				if index == *stored_index {
					// SAFETY: The `ptr` points to a value allocated by `NodeArena`.
					// The `guard` ensures that the value is pinned and will not be freed during the lifetime of this function.
					Some(unsafe { &*ptr.ptr() })
				} else {
					None
				}
			}

			Some(NodePtr::Node(ptr)) => {
				let leaf = self.find_leaf(*ptr, index, guard);

				// SAFETY: `find_leaf` returns a `TaggedPtr` to a leaf node that's allocated by `NodeArena`.
				// The `guard` ensures that the leaf node is pinned and will not be freed during the lifetime of this function.
				let leaf = unsafe { &*leaf.ptr() };
				let pivots = leaf.pivots();
				let slots = leaf.slots();

				match pivots.binary_search(&index) {
					// SAFETY: `find_leaf` guarantees that it returns a node of kind `NodeKind::Leaf64`.
					// In a `Leaf64` node, the `Slot` union is defined to hold a `value` and not a `child` pointer.
					// Because we have found an exact match for the index in the pivots array, we can be sure that
					// the corresponding slot at `slots[idx]` contains a value due to the slot_count invariant on `Node`.
					Ok(idx) => Some(unsafe { &*slots[idx].value }),
					Err(_) => None,
				}
			}

			_ => None,
		}
	}

	pub fn store(&self, index: K, value: V) {
		let _lock = self.lock.lock(); // Synchronize writes
		let guard = &crossbeam_epoch::pin();

		let root = self.root.load(Ordering::Relaxed, guard);

		// SAFETY: The `root` pointer is loaded from an `Atomic` pointer that's only ever updated by this function,
		// which is synchronized with a `Mutex` ensuring that the root always points to valid memory.
		// The `guard` ensures that the memory will not be freed for the lifetime of the guard.
		match unsafe { root.as_ref() } {
			Some(NodePtr::Empty) => {
				let new_value_ptr = self.arena.alloc_value(value);
				let new_root = NodePtr::Direct(index, TaggedPtr::new(new_value_ptr.as_ptr(), 0));
				self.root.store(Owned::new(new_root), Ordering::Release);
			}

			Some(NodePtr::Direct(existing_index, existing_ptr)) => {
				if index == *existing_index {
					// SAFETY: The `existing_ptr` points to a value allocated by `NodeArena`.
					// The `guard` ensures that the value is pinned and will not be freed during the lifetime of this function.
					// The mutex on this function prevents data races.
					unsafe { existing_ptr.ptr().write(value) };
					return;
				}

				let mut node = self.arena.alloc_node();
				// SAFETY: `NodeArena` guarantees that `alloc_node` returns a `NonNull<u8>`
				// pointer to valid, uninitialized memory for a `Node`.
				// The mutex on this function ensures that we have exclusive access to the memory.
				let node_ref = unsafe { node.as_mut() };
				node_ref.header.parent = std::ptr::null_mut();
				node_ref.header.parent_slot = 0;
				node_ref.header.set_slot_count(2);
				node_ref.header.set_node_kind(NodeKind::Leaf64);

				// SAFETY: The `existing_ptr` is valid for the duration of the guard.
				// `ptr::read` is used to move the value, which is safe because we immediately
				// re-insert it into the new node which avoids a double-free, as the value is now
				// owned by the new node and the old `Direct` pointer will be replaced.
				let existing_value = unsafe { existing_ptr.ptr().read() };
				if index < *existing_index {
					let pivots = node_ref.pivots_mut();
					pivots[0] = index;
					pivots[1] = *existing_index;

					let slots = node_ref.slots_mut();
					slots[0].value = ManuallyDrop::new(value);
					slots[1].value = ManuallyDrop::new(existing_value);
				} else {
					let pivots = node_ref.pivots_mut();
					pivots[0] = *existing_index;
					pivots[1] = index;

					let slots = node_ref.slots_mut();
					slots[0].value = ManuallyDrop::new(existing_value);
					slots[1].value = ManuallyDrop::new(value);
				}

				let new_root = NodePtr::Node(TaggedPtr::new(node.as_ptr(), 0));
				self.root.store(Owned::new(new_root), Ordering::Release);
			}

			Some(NodePtr::Node(node)) => {
				let leaf_ptr = self.find_leaf(*node, index, guard);

				// CoW
				let mut new_leaf = self.arena.clone_node(leaf_ptr);

				// SAFETY: `NodeArena` guarantees that `clone_node` returns a `NonNull<u8>`
				// pointer to a valid copy of the original `Node`.
				// The mutex on this function ensures that we have exclusive access to the memory.
				let new_leaf_ref = unsafe { new_leaf.as_mut() };

				// `leaf_insert` will either insert the value or split the node if full.
				if let Some((pivot, new_sibling_ptr)) = self.leaf_insert(new_leaf_ref, index, value) {
					// The leaf was split. We need to insert the new pivot and sibling into the parent.
					let new_root_ptr =
						self.insert_into_parent(leaf_ptr.ptr(), new_leaf.as_ptr(), pivot, new_sibling_ptr.ptr(), guard);
					self.root
						.store(Owned::new(NodePtr::Node(new_root_ptr)), Ordering::Release);
				} else {
					let new_root_ptr = self.walk_up_and_update(leaf_ptr.ptr(), new_leaf.as_ptr(), guard);
					self.root
						.store(Owned::new(NodePtr::Node(new_root_ptr)), Ordering::Release);
				}
			}
			_ => {}
		}
	}

	pub fn clear(&self) {
		let _lock = self.lock.lock();
		let guard = &crossbeam_epoch::pin();

		let old_root = self.root.swap(Owned::new(NodePtr::Empty), Ordering::Release, guard);
		if let Some(old_root) = unsafe { old_root.as_ref() } {
			match old_root {
				NodePtr::Empty => {}
				NodePtr::Direct(_, tagged_ptr) => unsafe {
					guard.defer_destroy(Shared::from(tagged_ptr.ptr().cast_const()));
				},
				NodePtr::Node(tagged_ptr) => {
					self.defer_destroy_node(tagged_ptr.ptr(), guard);
				}
			}
		}
	}

	fn defer_destroy_node<'guard>(&self, node_ptr: *mut Node<K, V, CAPACITY>, guard: &'guard Guard) {
		let node = unsafe { &*node_ptr };

		match node.header.node_kind() {
			NodeKind::Leaf64 => {
				let slots = unsafe { (*node_ptr).slots_mut() };
				for i in 0..node.header.slot_count() as usize {
					unsafe {
						ManuallyDrop::drop(&mut slots[i].value);
					}
				}
			}
			NodeKind::Range64 => {
				let slots = node.slots();
				for i in 0..node.header.slot_count() as usize {
					let child_ptr = unsafe { *slots[i].child };
					self.defer_destroy_node(child_ptr.ptr(), guard);
				}
			}
		}

		unsafe {
			guard.defer_destroy(Shared::from(node_ptr.cast_const()));
		}
	}

	fn find_leaf<'guard>(
		&self,
		mut node: TaggedPtr<Node<K, V, CAPACITY>>,
		index: K,
		_guard: &'guard Guard,
	) -> TaggedPtr<Node<K, V, CAPACITY>> {
		const MAPLE_HEIGHT_MAX: usize = 31;

		// A static depth loop is bound to the theoretical maximum height of the tree to prevent an infinite loop.
		// The current Linux C implementation defines this here: https://github.com/torvalds/linux/blob/50c19e20ed2ef359cf155a39c8462b0a6351b9fa/include/linux/maple_tree.h#L188
		for _ in 0..MAPLE_HEIGHT_MAX {
			// SAFETY: The initial `node` pointer is guaranteed to be valid by the caller.
			// In subsequent iterations `node` is updated with a child pointer from a valid parent node, so `node.ptr()` is
			// always guaranteed to be a valid pointer of `Node` that will not be freed during the lifetime of this function.
			let node_ref = unsafe { &*node.ptr() };
			if node_ref.header.node_kind() == NodeKind::Leaf64 {
				return node;
			}

			let pivots = node_ref.pivots();
			let slots = node_ref.slots();

			let slot_index = pivots.partition_point(|&p| p < index);

			// SAFETY: This node isn't a Leaf node so it is an internal node.
			// The Tree's invariant that all slots of an internal node always contain `child` valid pointers
			// guarantees that this is a safe access.
			node = unsafe { *slots[slot_index].child };
		}

		panic!("maple tree max depth exceeded");
	}

	/// Updates the parent pointers of all children of `node_ptr`.
	/// This is called after a node is cloned.
	fn reparent_children(&self, node_ptr: *mut Node<K, V, CAPACITY>) {
		// SAFETY: It is up to the caller to guarantee that `node_ptr` is a valid pointer to a node
		// and that we have exclusive mutable access.
		let node = unsafe { &mut *node_ptr };

		// Leaf nodes don't have children.
		if node.header.node_kind() == NodeKind::Leaf64 {
			return;
		}

		for (i, slot) in node.slots_mut().iter_mut().enumerate() {
			// SAFETY: The check above ensures this is an internal node.
			// By the tree's invariant the slots of an internal node always contain
			// `child` pointers.
			let child_tagged_ptr = unsafe { &*slot.child };
			let child_ptr = child_tagged_ptr.ptr();

			if !child_ptr.is_null() {
				// SAFETY: `child_ptr` is a pointer to a node that was valid in the original tree.
				// `store` locking a mutex ensures exclusive access.
				unsafe {
					(*child_ptr).header.parent = node_ptr;
					(*child_ptr).header.parent_slot = i as u8;
				}
			}
		}
	}

	fn walk_up_and_update<'guard>(
		&self,
		mut original_node_ptr: *mut Node<K, V, CAPACITY>,
		mut new_node_ptr: *mut Node<K, V, CAPACITY>,
		_guard: &'guard Guard,
	) -> TaggedPtr<Node<K, V, CAPACITY>> {
		loop {
			// SAFETY: `original_node_ptr` always points to a node on the original path.
			// On the first iteration it points to the original leaf node which is guaranteed to be valid by the caller.
			// On subsequent iterations it points to the `parent_ptr` from the previous node.
			// The guard ensures that these pointers are valid for the duration of this function.
			let (parent_ptr, slot_in_parent) = unsafe {
				let child_ref = &*original_node_ptr;
				(child_ref.header.parent, child_ref.header.parent_slot as usize)
			};

			// If the parent is null, we've reached the root of the tree.
			// `new_node_ptr` points to the head of the new path which is the new root of the tree.
			if parent_ptr.is_null() {
				return TaggedPtr::new(new_node_ptr, 0);
			}

			let mut new_parent = self.arena.clone_node_raw(parent_ptr);

			// SAFETY: `clone_node_raw` returns a `NonNull<Node<V, CAPACITY>>` which is guaranteed to be valid.
			// The caller (`store`) holds a mutex which guarantees exclusive access to this memory.
			let new_parent_ref = unsafe { new_parent.as_mut() };

			// Update the slot in the new parent to point to the new child from the level below.
			new_parent_ref.slots_mut()[slot_in_parent].child = ManuallyDrop::new(TaggedPtr::new(new_node_ptr, 0));
			self.reparent_children(new_parent.as_ptr());

			original_node_ptr = parent_ptr;
			new_node_ptr = new_parent.as_ptr();
		}
	}

	fn leaf_insert(
		&self,
		leaf: &mut Node<K, V, CAPACITY>,
		index: K,
		value: V,
	) -> Option<(K, TaggedPtr<Node<K, V, CAPACITY>>)> {
		let count = leaf.header.slot_count() as usize;
		let pivots = leaf.pivots();
		let slot_index = pivots.binary_search(&index).unwrap_or_else(|i| i);

		// Overwrite the value in-place if the key already exists.
		if slot_index < count && pivots[slot_index] == index {
			unsafe { ManuallyDrop::drop(&mut leaf.slots_mut()[slot_index].value) };
			leaf.slots_mut()[slot_index].value = ManuallyDrop::new(value);
			return None;
		}

		// Node has space, insert the new value
		if count < CAPACITY {
			let (pivots, slots) = leaf.pivots_and_slots_raw_mut();

			// SAFETY: This function is only called by `store` which has a mutex to ensure exclusive access to the memory.
			// The pointers derived from `slots` and `pivots` are valid for R/W because they come from a mutable reference to
			// an arena-allocated `Node`.
			// This operation is **only** safe if the node is not full, if it is then it will write past the array bounds.
			unsafe {
				// Shift pivots and slots to the right to make space for the new pivot and value.

				let pivots_ptr = pivots.as_mut_ptr();
				std::ptr::copy(
					pivots_ptr.add(slot_index),
					pivots_ptr.add(slot_index + 1),
					count - slot_index,
				);

				let slots_ptr = slots.as_mut_ptr();
				for i in (slot_index..count).rev() {
					let val = slots_ptr.add(i).read();
					slots_ptr.add(i + 1).write(val);
				}
			}

			pivots[slot_index].write(index);
			slots[slot_index].write(Slot {
				value: ManuallyDrop::new(value),
			});

			leaf.header.set_slot_count(count as u8 + 1);
			return None;
		}

		// Node is full, split it
		let mut all_pivots: [MaybeUninit<K>; CAPACITY + 1] = unsafe { MaybeUninit::uninit().assume_init() };
		let mut all_slots: [MaybeUninit<Slot<K, V, CAPACITY>>; CAPACITY + 1] =
			unsafe { MaybeUninit::uninit().assume_init() };

		unsafe {
			let pivots_ptr = leaf.pivots().as_ptr();
			let slots_mut_ptr = leaf.slots_mut().as_mut_ptr();

			std::ptr::copy_nonoverlapping(pivots_ptr, all_pivots.as_mut_ptr().cast(), slot_index);
			for i in 0..slot_index {
				all_slots[i].write(slots_mut_ptr.add(i).read());
			}

			all_pivots[slot_index].write(index);
			all_slots[slot_index].write(Slot {
				value: ManuallyDrop::new(value),
			});

			std::ptr::copy_nonoverlapping(
				pivots_ptr.add(slot_index),
				all_pivots.as_mut_ptr().add(slot_index + 1).cast(),
				count - slot_index,
			);
			for i in slot_index..count {
				all_slots[i + 1].write(slots_mut_ptr.add(i).read());
			}
		}

		// Find the split point and the pivot to promote to the parent
		let split_point = (CAPACITY + 1) / 2;
		// SAFETY: `all_pivots` is a slice of initialized pivots.
		let pivot_to_promote = unsafe { all_pivots[split_point - 1].assume_init() };

		// Turn `leaf` into the new left sibling in the split.
		leaf.header.set_slot_count(split_point as u8);
		let (left_pivots, left_slots) = leaf.pivots_and_slots_raw_mut();
		unsafe {
			let all_pivots_ptr = all_pivots.as_ptr();
			let all_slots_ptr = all_slots.as_mut_ptr();

			std::ptr::copy_nonoverlapping(all_pivots_ptr, left_pivots.as_mut_ptr(), split_point);
			for i in 0..split_point {
				left_slots.as_mut_ptr().add(i).write(all_slots_ptr.add(i).read());
			}
		}

		// Create the new right sibling node.
		let mut new_right_sibling = self.arena.alloc_node();
		// SAFETY: `NodeArena` guarantees that `alloc_node` returns a `NonNull<u8>`
		// pointer to valid, uninitialized memory for a `Node`.
		// The mutex from the caller (`store`) ensures that we have exclusive access to the memory.
		let new_sibling_ref = unsafe { new_right_sibling.as_mut() };
		new_sibling_ref.header.set_node_kind(NodeKind::Leaf64);
		new_sibling_ref.header.parent = leaf.header.parent;
		new_sibling_ref.header.parent_slot = leaf.header.parent_slot;

		let right_count = (CAPACITY + 1) - split_point;
		new_sibling_ref.header.set_slot_count(right_count as u8);
		let (right_pivots, right_slots) = new_sibling_ref.pivots_and_slots_raw_mut();

		// Move the second half of the pivots and slots into the new node.
		unsafe {
			let all_pivots_ptr = all_pivots.as_ptr();
			let all_slots_ptr = all_slots.as_mut_ptr();

			std::ptr::copy_nonoverlapping(all_pivots_ptr.add(split_point), right_pivots.as_mut_ptr(), right_count);
			for i in 0..right_count {
				right_slots
					.as_mut_ptr()
					.add(i)
					.write(all_slots_ptr.add(split_point + i).read());
			}
		}

		Some((pivot_to_promote, TaggedPtr::new(new_right_sibling.as_ptr(), 0)))
	}

	fn internal_insert(
		&self,
		node: &mut Node<K, V, CAPACITY>,
		pivot: K,
		child: TaggedPtr<Node<K, V, CAPACITY>>,
	) -> Option<(K, TaggedPtr<Node<K, V, CAPACITY>>)> {
		let count = node.header.slot_count() as usize;
		let pivots = node.pivots();
		let pivot_insertion_point = pivots.binary_search(&pivot).unwrap_or_else(|i| i);
		// In an internal node, the new child slot is after the new pivot's position.
		let slot_insertion_point = pivot_insertion_point + 1;

		// Node has space, insert the new value
		if count < CAPACITY {
			let (pivots_raw, slots_raw) = node.pivots_and_slots_raw_mut();
			// SAFETY: This function is only called by `store` which has a mutex to ensure exclusive access to the memory.
			// The pointers derived from `slots` and `pivots` are valid for R/W because they come from a mutable reference to
			// an arena-allocated `Node`.
			// This operation is **only** safe if the node is not full, if it is then it will write past the array bounds.
			unsafe {
				// Shift pivots and slots to the right to make space for the new pivot and value.

				let pivots_ptr = pivots_raw.as_mut_ptr();
				std::ptr::copy(
					pivots_ptr.add(pivot_insertion_point),
					pivots_ptr.add(pivot_insertion_point + 1),
					// Internal nodes have `count - 1` pivots.
					(count - 1) - pivot_insertion_point,
				);

				let slots_ptr = slots_raw.as_mut_ptr();
				std::ptr::copy(
					slots_ptr.add(slot_insertion_point),
					slots_ptr.add(slot_insertion_point + 1),
					count - slot_insertion_point,
				);
			}
			pivots_raw[pivot_insertion_point].write(pivot);
			slots_raw[slot_insertion_point].write(Slot {
				child: ManuallyDrop::new(child),
			});
			node.header.set_slot_count(count as u8 + 1);
			return None;
		}

		// Node is full, split it
		let pivots_count = count - 1;
		let mut all_pivots: [MaybeUninit<K>; CAPACITY + 1] = unsafe { MaybeUninit::uninit().assume_init() };
		let mut all_slots: [MaybeUninit<Slot<K, V, CAPACITY>>; CAPACITY + 1] =
			unsafe { MaybeUninit::uninit().assume_init() };

		unsafe {
			let pivots_ptr = pivots.as_ptr();
			// SAFETY: `MaybeUninit<u64>` and `u64` have the same layout as eachother.
			let all_pivots_ptr = all_pivots.as_mut_ptr().cast::<K>();
			std::ptr::copy_nonoverlapping(pivots_ptr, all_pivots_ptr, pivot_insertion_point);
			all_pivots_ptr.add(pivot_insertion_point).write(pivot);
			std::ptr::copy_nonoverlapping(
				pivots_ptr.add(pivot_insertion_point),
				all_pivots_ptr.add(pivot_insertion_point + 1),
				pivots_count - pivot_insertion_point,
			);

			let slots_ptr = node.slots().as_ptr();
			// SAFETY: `MaybeUninit<Slot<V, CAPACITY>>` and `Slot<V, CAPACITY>` have the same layout as eachother.
			let all_slots_ptr = all_slots.as_mut_ptr().cast::<Slot<K, V, CAPACITY>>();
			std::ptr::copy_nonoverlapping(slots_ptr, all_slots_ptr, slot_insertion_point);
			all_slots_ptr.add(slot_insertion_point).write(Slot {
				child: ManuallyDrop::new(child),
			});
			std::ptr::copy_nonoverlapping(
				slots_ptr.add(slot_insertion_point),
				all_slots_ptr.add(slot_insertion_point + 1),
				count - slot_insertion_point,
			);
		}

		let left_slots_count = (all_slots.len() + 1) / 2;
		let left_pivots_count = left_slots_count - 1;
		// SAFETY: `all_pivots[left_pivots_count]` is initialized.
		let pivot_to_promote = unsafe { all_pivots[left_pivots_count].assume_init_read() };

		// Turn `node` into the new left sibling in the split.
		node.header.set_slot_count(left_slots_count as u8);
		let (left_pivots, left_slots) = node.pivots_and_slots_raw_mut();
		unsafe {
			std::ptr::copy_nonoverlapping(all_pivots.as_ptr(), left_pivots.as_mut_ptr(), left_pivots_count);
			std::ptr::copy_nonoverlapping(all_slots.as_ptr(), left_slots.as_mut_ptr(), left_slots_count);
		}

		// Create the new right sibling node.
		let mut new_right_sibling = self.arena.alloc_node();
		// SAFETY: `NodeArena` guarantees that `alloc_node` returns a `NonNull<u8>`
		// pointer to valid, uninitialized memory for a `Node`.
		// The mutex from the caller (`store`) ensures that we have exclusive access to the memory.
		let new_sibling_ref = unsafe { new_right_sibling.as_mut() };
		new_sibling_ref.header.set_node_kind(node.header.node_kind());
		new_sibling_ref.header.parent = node.header.parent;
		new_sibling_ref.header.parent_slot = node.header.parent_slot;

		let right_slots_count = (CAPACITY + 1) - left_slots_count;
		let right_pivots_count = right_slots_count - 1;
		new_sibling_ref.header.set_slot_count(right_slots_count as u8);

		let (right_pivots, right_slots) = new_sibling_ref.pivots_and_slots_raw_mut();

		// Move the second half of the pivots and slots into the new node.
		unsafe {
			std::ptr::copy_nonoverlapping(
				all_pivots.as_ptr().add(left_pivots_count + 1),
				right_pivots.as_mut_ptr(),
				right_pivots_count,
			);
			std::ptr::copy_nonoverlapping(
				all_slots.as_ptr().add(left_slots_count),
				right_slots.as_mut_ptr(),
				right_slots_count,
			);
		}

		Some((pivot_to_promote, TaggedPtr::new(new_right_sibling.as_ptr(), 0)))
	}

	fn create_new_root(
		&self,
		pivot: K,
		left_child: *mut Node<K, V, CAPACITY>,
		right_child: *mut Node<K, V, CAPACITY>,
	) -> TaggedPtr<Node<K, V, CAPACITY>> {
		let mut new_root = self.arena.alloc_node();
		// SAFETY: `NodeArena` guarantees that `alloc_node` returns a `NonNull<u8>`
		// pointer to valid, uninitialized memory for a `Node`.
		// The mutex from the caller (`store`) ensures that we have exclusive access to the memory.
		let new_root_ref = unsafe { new_root.as_mut() };

		new_root_ref.header.set_node_kind(NodeKind::Range64);
		new_root_ref.header.set_slot_count(2);
		new_root_ref.header.parent = std::ptr::null_mut();
		new_root_ref.header.parent_slot = 0;

		new_root_ref.pivots_mut()[0] = pivot;

		let slots = new_root_ref.slots_mut();
		slots[0].child = ManuallyDrop::new(TaggedPtr::new(left_child, 0));
		slots[1].child = ManuallyDrop::new(TaggedPtr::new(right_child, 0));

		self.reparent_children(new_root.as_ptr());

		TaggedPtr::new(new_root.as_ptr(), 0)
	}

	fn insert_into_parent<'guard>(
		&self,
		mut old_child_ptr: *mut Node<K, V, CAPACITY>,
		mut new_child_ptr: *mut Node<K, V, CAPACITY>,
		mut pivot: K,
		mut new_sibling_ptr: *mut Node<K, V, CAPACITY>,
		guard: &'guard Guard,
	) -> TaggedPtr<Node<K, V, CAPACITY>> {
		// SAFETY: `old_child_ptr` is the original leaf that was split.
		// The caller (`store`) guarantees that it's valid.
		let parent_ptr = unsafe { (*old_child_ptr).header.parent };

		// Check if the split was on the root
		if parent_ptr.is_null() {
			return self.create_new_root(pivot, new_child_ptr, new_sibling_ptr);
		}

		loop {
			// SAFETY: `old_child_ptr` always points to a valid node on the original path.
			// On subsequent iterations it's the parent of the previous node.
			let (current_parent_ptr, slot_in_parent) = unsafe {
				let child_ref = &*old_child_ptr;
				(child_ref.header.parent, child_ref.header.parent_slot as usize)
			};

			let mut new_parent = self.arena.clone_node_raw(current_parent_ptr);
			// SAFETY: `clone_node_raw` returns a `NonNull<Node<V, CAPACITY>>` which is guaranteed to be valid.
			// The caller (`store`) holds a mutex which guarantees exclusive access to this memory.
			let new_parent_ref = unsafe { new_parent.as_mut() };

			// Update the pointer to the child that was modified.
			new_parent_ref.slots_mut()[slot_in_parent].child = ManuallyDrop::new(TaggedPtr::new(new_child_ptr, 0));

			// Try to insert the new sibling from the split below into the new parent.
			if let Some((promoted_pivot, new_parent_sibling)) =
				self.internal_insert(new_parent_ref, pivot, TaggedPtr::new(new_sibling_ptr, 0))
			{
				// The parent was also split so reparent the children of both siblings.
				self.reparent_children(new_parent.as_ptr());
				self.reparent_children(new_parent_sibling.ptr());

				let next_parent_ptr = new_parent_ref.header.parent;
				if next_parent_ptr.is_null() {
					return self.create_new_root(promoted_pivot, new_parent.as_ptr(), new_parent_sibling.ptr());
				}

				// The split parent becomes the new child for the next level.
				pivot = promoted_pivot;
				new_sibling_ptr = new_parent_sibling.ptr();
				old_child_ptr = current_parent_ptr;
				new_child_ptr = new_parent.as_ptr();
			} else {
				// Parent didn't split, reparent it's children and complete the update.
				self.reparent_children(new_parent.as_ptr());
				return self.walk_up_and_update(current_parent_ptr, new_parent.as_ptr(), guard);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn store_single() {
		let tree = MapleTree::<u64, u64, RANGE64_SLOTS>::new();
		tree.store(10, 100u64);
		let guard = &crossbeam_epoch::pin();
		assert_eq!(*tree.load(10, guard).unwrap(), 100);
	}

	#[test]
	fn store_multiple() {
		let tree = MapleTree::<u64, u64, RANGE64_SLOTS>::new();
		tree.store(10, 100);
		tree.store(20, 200);
		tree.store(5, 50);

		let guard = &crossbeam_epoch::pin();
		assert_eq!(*tree.load(5, guard).unwrap(), 50);
		assert_eq!(*tree.load(10, guard).unwrap(), 100);
		assert_eq!(*tree.load(20, guard).unwrap(), 200);
	}

	#[test]
	fn overwrite() {
		let tree = MapleTree::<u64, u64, RANGE64_SLOTS>::new();
		tree.store(10, 100);
		tree.store(10, 101);
		let guard = &crossbeam_epoch::pin();
		assert_eq!(*tree.load(10, guard).unwrap(), 101);

		tree.store(20, 200);
		tree.store(20, 201);
		assert_eq!(*tree.load(20, guard).unwrap(), 201);
		assert_eq!(*tree.load(10, guard).unwrap(), 101);
	}

	#[test]
	fn fill_leaf() {
		let tree = MapleTree::<u64, u64, RANGE64_SLOTS>::new();
		for i in 0..RANGE64_SLOTS as u64 {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		for i in 0..RANGE64_SLOTS as u64 {
			assert_eq!(*tree.load(i, guard).unwrap(), i * 10);
		}
	}

	#[test]
	fn fill_leaf_rev() {
		let tree = MapleTree::<u64, u64, RANGE64_SLOTS>::new();
		for i in (0..RANGE64_SLOTS as u64).rev() {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		for i in 0..RANGE64_SLOTS as u64 {
			assert_eq!(*tree.load(i, guard).unwrap(), i * 10);
		}
	}

	#[test]
	fn split_leaf() {
		let tree = MapleTree::<u64, u64, 31>::new();
		for i in 0..(RANGE64_SLOTS * 50) as u64 {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		for i in 0..(RANGE64_SLOTS * 50) as u64 {
			assert_eq!(*tree.load(i, guard).unwrap(), i * 10);
		}
	}

	#[test]
	fn split_leaf_rev() {
		let tree = MapleTree::<u64, u64, 31>::new();
		for i in (0..(RANGE64_SLOTS * 50) as u64).rev() {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		for i in 0..(RANGE64_SLOTS * 50) as u64 {
			assert_eq!(*tree.load(i, guard).unwrap(), i * 10);
		}
	}

	#[test]
	fn clear_tree() {
		let tree = MapleTree::<u64, u64, 31>::new();
		for i in 0..(RANGE64_SLOTS * 50) as u64 {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		assert_eq!(*tree.load(100, guard).unwrap(), 1000);

		tree.clear();

		for i in 0..(RANGE64_SLOTS * 50) as u64 {
			assert!(tree.load(i, guard).is_none());
		}

		tree.store(200, 2000);
		assert_eq!(*tree.load(200, guard).unwrap(), 2000);

		tree.clear();
		assert!(tree.load(200, guard).is_none());
	}
}
