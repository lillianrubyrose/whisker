use std::{mem::ManuallyDrop, sync::atomic::Ordering};

use crossbeam_epoch::{Atomic, Guard, Owned};
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

pub enum NodePtr<V, const CAPACITY: usize> {
	Empty,
	Direct(u64, TaggedPtr<V>),
	Node(TaggedPtr<Node<V, CAPACITY>>),
}

pub struct MapleTree<V, const CAPACITY: usize> {
	root: Atomic<NodePtr<V, CAPACITY>>,
	arena: NodeArena<V, CAPACITY>,
	lock: Mutex<()>,
}

unsafe impl<V, const CAPACITY: usize> Send for MapleTree<V, CAPACITY> {}
unsafe impl<V, const CAPACITY: usize> Sync for MapleTree<V, CAPACITY> {}

impl<V, const CAPACITY: usize> MapleTree<V, CAPACITY> {
	pub fn new() -> Self {
		Self {
			root: Atomic::new(NodePtr::Empty),
			arena: NodeArena::new(),
			lock: Mutex::new(()),
		}
	}

	pub fn load<'guard>(&self, index: u64, guard: &'guard Guard) -> Option<&'guard V> {
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

	pub fn store(&self, index: u64, value: V) {
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
				node_ref.header.parent = TaggedPtr::new(std::ptr::null_mut(), 0);
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
				let mut new_leaf = self.arena.clone_node(leaf_ptr);

				// SAFETY: `NodeArena` guarantees that `clone_node` returns a `NonNull<u8>`
				// pointer to a valid copy of the original `Node`.
				// The mutex on this function ensures that we have exclusive access to the memory.
				let new_leaf_ref = unsafe { new_leaf.as_mut() };
				self.leaf_insert(new_leaf_ref, index, value);
				let new_root_ptr = self.walk_up_and_update(leaf_ptr, new_leaf.as_ptr(), guard);
				self.root
					.store(Owned::new(NodePtr::Node(new_root_ptr)), Ordering::Release);
			}
			_ => {}
		}
	}

	fn find_leaf<'guard>(
		&self,
		mut node: TaggedPtr<Node<V, CAPACITY>>,
		index: u64,
		_guard: &'guard Guard,
	) -> TaggedPtr<Node<V, CAPACITY>> {
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

	fn walk_up_and_update<'guard>(
		&self,
		mut old_child: TaggedPtr<Node<V, CAPACITY>>,
		new_child_ptr: *mut Node<V, CAPACITY>,
		_guard: &'guard Guard,
	) -> TaggedPtr<Node<V, CAPACITY>> {
		// This starts as the new leaf node.
		// In subsequent iterations it will point to the newly cloned parent from the level below.
		let mut new_child = TaggedPtr::new(new_child_ptr, old_child.tag());

		loop {
			// SAFETY: On the first iteration `old_child` is the original leaf node
			// which is guaranteed to be valid for the duration of the guard.
			// In subsequent iterations `old_child` is the parent pointer from the previous valid node
			// which is guaranteed to be valid for the duration of the guard.
			let parent_ptr = unsafe { (*old_child.ptr()).header.parent() };

			// If the parent is null, we've reached the root of the tree.
			if parent_ptr.is_null() {
				return new_child;
			}

			let slot_in_parent = parent_ptr.tag();
			// SAFETY: `NodeArena` guarantees that `clone_node` returns a `NonNull<u8>`
			// pointer to a valid copy of the original `Node`.
			// This function is only ever called by `store` which has a mutex ensuring exclusive access to the memory.
			let mut new_parent = self.arena.clone_node(parent_ptr);

			// SAFETY: The new parent node is guaranteed to be valid from the allocator.
			let new_parent_ref = unsafe { new_parent.as_mut() };

			// Update the slot in the new parent to point to the new child from the level below.
			new_parent_ref.slots_mut()[slot_in_parent].child = ManuallyDrop::new(new_child);

			old_child = parent_ptr;
			new_child = TaggedPtr::new(new_parent.as_ptr(), parent_ptr.tag());
		}
	}

	fn leaf_insert(&self, leaf: &mut Node<V, CAPACITY>, index: u64, value: V) {
		let count = leaf.header.slot_count() as usize;
		let pivots = leaf.pivots();
		let slot_index = pivots.binary_search(&index).unwrap_or_else(|i| i);

		// Overwrite the value in-place if the key already exists.
		if slot_index < count && pivots[slot_index] == index {
			leaf.slots_mut()[slot_index].value = ManuallyDrop::new(value);
			return;
		}

		// TODO: Implement node splitting in `store`
		debug_assert!(count < CAPACITY, "leaf_insert called on a full node");

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
			std::ptr::copy(
				slots_ptr.add(slot_index),
				slots_ptr.add(slot_index + 1),
				count - slot_index,
			);
		}

		pivots[slot_index].write(index);
		slots[slot_index].write(Slot {
			value: ManuallyDrop::new(value),
		});

		leaf.header.set_slot_count(count as u8 + 1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn store_single() {
		let tree = MapleTree::<u64, RANGE64_SLOTS>::new();
		tree.store(10, 100u64);
		let guard = &crossbeam_epoch::pin();
		assert_eq!(*tree.load(10, guard).unwrap(), 100);
	}

	#[test]
	fn store_multiple() {
		let tree = MapleTree::<u64, RANGE64_SLOTS>::new();
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
		let tree = MapleTree::<u64, RANGE64_SLOTS>::new();
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
		let tree = MapleTree::<u64, RANGE64_SLOTS>::new();
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
		let tree = MapleTree::<u64, RANGE64_SLOTS>::new();
		for i in (0..RANGE64_SLOTS as u64).rev() {
			tree.store(i, i * 10);
		}

		let guard = &crossbeam_epoch::pin();
		for i in 0..RANGE64_SLOTS as u64 {
			assert_eq!(*tree.load(i, guard).unwrap(), i * 10);
		}
	}
}
