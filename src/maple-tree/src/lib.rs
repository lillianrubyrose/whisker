mod node;
mod range;
mod rcu;
mod spin_lock;

use std::{
	ptr,
	sync::atomic::{AtomicPtr, AtomicUsize, Ordering},
};

use node::{InternalNode, LeafNode, Node};
use range::Range;
use rcu::{RCU_READER_COUNT, RcuGuard, call_rcu, rcu_barrier};
use spin_lock::SpinLock;

const MAPLE_NODE_SLOTS: usize = 16;

/// Minimum number of slots that must be filled in a node (excluding root)
/// before a rebalance will happen.
const MIN_SLOTS: usize = MAPLE_NODE_SLOTS / 2;

#[derive(Debug)]
pub struct MapleTree<T> {
	root: AtomicPtr<Node<T>>,
	height: AtomicUsize,
	lock: SpinLock,
}

// SAFETY: Our RCU and locking mechanisms ensure thread-safe access
unsafe impl<T: Send> Send for MapleTree<T> {}
unsafe impl<T: Sync> Sync for MapleTree<T> {}

#[derive(Debug)]
enum StoreType {
	/// The new range exactly matches an existing slot - just replace the value
	ExactFit,

	/// Can modify an existing slot by updating its range
	UpdateExistingSlotRange,

	/// Need to insert into an existing node that has space
	InsertIntoExistingNode,

	/// Node is full and needs to be split
	SplitNode,
}

struct WriteState {
	store_type: StoreType,
	slot_idx: usize,
}

impl<T: 'static> MapleTree<T> {
	pub const fn new() -> Self {
		Self {
			root: AtomicPtr::new(ptr::null_mut()),
			height: AtomicUsize::new(0),
			lock: SpinLock::new(),
		}
	}

	/// Try to load a value at a specific index.
	pub fn load(&self, index: usize) -> Option<&T> {
		let _guard = RcuGuard::new();

		let root = self.root.load(Ordering::Acquire);
		if root.is_null() {
			return None;
		}

		// SAFETY: Root pointer is valid while holding RCU read lock
		unsafe { self.load_from(root, index).map(|ptr| &*ptr) }
	}

	/// Try to load a value at a specific index.
	///
	/// # Safety
	///
	/// This is unsafe in a concurrent context.
	/// The caller must ensure exclusive access.
	pub fn load_mut(&self, index: usize) -> Option<&mut T> {
		let _guard = RcuGuard::new();

		let root = self.root.load(Ordering::Acquire);
		if root.is_null() {
			return None;
		}

		// SAFETY: Caller ensures exclusive access for mutation
		unsafe { self.load_from(root, index).map(|ptr| &mut *ptr) }
	}

	/// Store a value at a single index.
	pub fn store(&self, index: usize, value: T) {
		self.insert(Range::new(index, index), value);
	}

	/// Store a value across a range of indices.
	pub fn store_range(&self, start: usize, end: usize, value: T) {
		self.insert(Range::new(start, end), value);
	}

	/// Remove and return the value at a specific index.
	pub fn erase(&self, index: usize) -> Option<T> {
		let _guard = self.lock.lock();
		let old_root = self.root.load(Ordering::Acquire);
		if old_root.is_null() {
			return None;
		}

		// SAFETY: We hold the write lock
		unsafe {
			let (new_root, erased_val, _) = self.erase_recursive(old_root, index);
			if new_root != old_root {
				self.root.store(new_root, Ordering::Release);
				call_rcu(old_root);
			}

			erased_val.map(|ptr| *Box::from_raw(ptr))
		}
	}

	pub fn clear(&mut self) {
		let root = self.root.swap(ptr::null_mut(), Ordering::AcqRel);
		if !root.is_null() {
			call_rcu(root);
			rcu_barrier();
		}
	}

	unsafe fn load_from(&self, node: *const Node<T>, index: usize) -> Option<*mut T> {
		if unsafe { (*node).is_leaf() } {
			let leaf = unsafe { (*node).as_leaf() };
			let count = leaf.count.load(Ordering::Acquire);
			for i in 0..count {
				let range = leaf.ranges[i];
				if index >= range.start && index <= range.end {
					return Some(leaf.values[i].load(Ordering::Acquire));
				}
			}
			None
		} else {
			let internal = unsafe { (*node).as_internal() };
			let count = internal.count.load(Ordering::Acquire);
			for i in 0..count {
				let pivot = internal.pivots[i].load(Ordering::Acquire);
				if index <= pivot {
					let child = internal.children[i].load(Ordering::Acquire);
					return unsafe { self.load_from(child, index) };
				}
			}
			None
		}
	}

	/// Insert a value with the given range into the tree.
	fn insert(&self, range: Range, value: T) {
		let _guard = self.lock.lock();
		let value = Box::into_raw(Box::new(value));

		let root = self.root.load(Ordering::Acquire);
		if root.is_null() {
			// Tree is empty, create a new leaf node
			let mut leaf = LeafNode::new();
			leaf.ranges[0] = range;
			leaf.values[0].store(value, Ordering::Release);
			leaf.count.store(1, Ordering::Release);

			self.root
				.store(Box::into_raw(Box::new(Node::Leaf(leaf))), Ordering::Release);
			self.height.store(1, Ordering::Release);
			return;
		}

		// SAFETY: We hold the write lock
		unsafe {
			let (new_root, split_opt) = self.insert_recursive(root, range, value);

			if let Some((sibling, pivot)) = split_opt {
				// Root split
				let new_parent = InternalNode::new();
				new_parent.children[0].store(new_root, Ordering::Release);
				new_parent.children[1].store(sibling, Ordering::Release);
				new_parent.pivots[0].store(pivot, Ordering::Release);
				new_parent.count.store(2, Ordering::Release);

				let new_root_node = Box::into_raw(Box::new(Node::Internal(new_parent)));
				self.root.store(new_root_node, Ordering::Release);
				self.height.fetch_add(1, Ordering::Release);
			} else {
				self.root.store(new_root, Ordering::Release);
			}

			if new_root != root {
				call_rcu(root);
			}
		}
	}

	/// Returns the new node pointer and optionally a sibling+pivot if the node split.
	///
	/// # Safety
	///
	/// Caller must hold the write lock and ensure that the node pointer is valid.
	unsafe fn insert_recursive(
		&self,
		node: *mut Node<T>,
		range: Range,
		value: *mut T,
	) -> (*mut Node<T>, Option<(*mut Node<T>, usize)>) {
		// SAFETY: Caller guarantees node is valid
		if unsafe { (*node).is_leaf() } {
			let leaf = unsafe { (*node).as_leaf() };
			let ws = self.analyze_leaf_write(leaf, range);

			match ws.store_type {
				StoreType::ExactFit => {
					// If the RCU_READER_COUNT is zero, we can modify the leaf in-place
					if RCU_READER_COUNT.with(|count| *count.borrow() == 0) {
						let old = leaf.values[ws.slot_idx].swap(value, Ordering::Release);
						if !old.is_null() {
							unsafe {
								let _ = Box::from_raw(old);
							}
						}

						return (node, None);
					}

					// Otherwise do a CoW strategy
					let new_leaf = leaf.clone();
					new_leaf.values[ws.slot_idx].store(value, Ordering::Release);
					(Box::into_raw(Box::new(Node::Leaf(new_leaf))), None)
				}
				StoreType::UpdateExistingSlotRange => {
					let mut new_leaf = leaf.clone();
					new_leaf.insert_or_update(range, value, ws.slot_idx);
					(Box::into_raw(Box::new(Node::Leaf(new_leaf))), None)
				}
				StoreType::InsertIntoExistingNode => {
					// Add to an existing node that has space
					let mut new_leaf = leaf.clone();
					new_leaf.insert_at_index(ws.slot_idx, range, value);
					(Box::into_raw(Box::new(Node::Leaf(new_leaf))), None)
				}
				StoreType::SplitNode => {
					// Node is full, split it
					let mut new_leaf = leaf.clone();
					let (sibling, pivot) = self.split_leaf(&mut new_leaf, range, value);
					(Box::into_raw(Box::new(Node::Leaf(new_leaf))), Some((sibling, pivot)))
				}
			}
		} else {
			// Internal node, find the appropriate child
			let internal = unsafe { (*node).as_internal() };
			let child_idx = internal.find_child(range.start);
			let child = internal.children[child_idx].load(Ordering::Acquire);

			// SAFETY: Child pointer is valid while we hold the write lock
			let (new_child, sibling) = unsafe { self.insert_recursive(child, range, value) };

			// If no modification just return
			if new_child == child {
				return (node, None);
			}

			// Otherwise time to update our pointer
			let mut new_internal = internal.clone();
			new_internal.children[child_idx].store(new_child, Ordering::Release);
			call_rcu(child);

			if let Some((sibling, pivot)) = sibling {
				// Child split, need to insert the sibling
				let count = new_internal.count.load(Ordering::Relaxed);
				if count < MAPLE_NODE_SLOTS {
					// We have space
					new_internal.insert_child(child_idx + 1, sibling, pivot);
					(Box::into_raw(Box::new(Node::Internal(new_internal))), None)
				} else {
					// No space, need to split
					let (new_internal_sibling, new_pivot) =
						self.split_internal(&mut new_internal, child_idx + 1, sibling, pivot);
					(
						Box::into_raw(Box::new(Node::Internal(new_internal))),
						Some((new_internal_sibling, new_pivot)),
					)
				}
			} else {
				new_internal.update_pivot(child_idx, range.end);
				(Box::into_raw(Box::new(Node::Internal(new_internal))), None)
			}
		}
	}

	fn analyze_leaf_write(&self, leaf: &LeafNode<T>, range: Range) -> WriteState {
		let count = leaf.count.load(Ordering::Relaxed);

		for i in 0..count {
			if leaf.ranges[i] == range {
				return WriteState {
					store_type: StoreType::ExactFit,
					slot_idx: i,
				};
			}
		}

		for i in 0..count {
			let existing = leaf.ranges[i];
			if range.start <= existing.end && range.end >= existing.start {
				return WriteState {
					store_type: StoreType::UpdateExistingSlotRange,
					slot_idx: i,
				};
			}
		}

		let idx = leaf.find_insert_pos(range.start);
		if count >= MAPLE_NODE_SLOTS {
			return WriteState {
				store_type: StoreType::SplitNode,
				slot_idx: idx,
			};
		}

		WriteState {
			store_type: StoreType::InsertIntoExistingNode,
			slot_idx: idx,
		}
	}

	/// Split a full leaf node into two nodes.
	///
	/// Returns the new sibling node and the pivot value that separates them.
	fn split_leaf(&self, leaf: &mut LeafNode<T>, range: Range, value: *mut T) -> (*mut Node<T>, usize) {
		let mid = MAPLE_NODE_SLOTS / 2;
		let mut new_leaf = LeafNode::new();

		// Copy the right half of the ranges and values to the new leaf.
		for i in mid..MAPLE_NODE_SLOTS {
			new_leaf.ranges[i - mid] = leaf.ranges[i];
			new_leaf.values[i - mid].store(leaf.values[i].load(Ordering::Acquire), Ordering::Release);
		}
		new_leaf.count.store(MAPLE_NODE_SLOTS - mid, Ordering::Release);
		leaf.count.store(mid, Ordering::Release);

		let pivot = leaf.ranges[mid - 1].end;

		if range.start <= pivot {
			let idx = leaf.find_insert_pos(range.start);
			leaf.insert_at_index(idx, range, value);
		} else {
			let idx = new_leaf.find_insert_pos(range.start);
			new_leaf.insert_at_index(idx, range, value);
		}

		(Box::into_raw(Box::new(Node::Leaf(new_leaf))), pivot)
	}

	/// Split a full internal node into two nodes.
	fn split_internal(
		&self,
		internal: &mut InternalNode<T>,
		insert_idx: usize,
		new_child: *mut Node<T>,
		pivot: usize,
	) -> (*mut Node<T>, usize) {
		let new_internal = InternalNode::new();
		let old_count = internal.count.load(Ordering::Relaxed);

		let mut tmp_children: [_; MAPLE_NODE_SLOTS + 1] = [ptr::null_mut(); MAPLE_NODE_SLOTS + 1];
		let mut tmp_pivots: [_; MAPLE_NODE_SLOTS] = [0; MAPLE_NODE_SLOTS];

		// Merge old data with new child/pivot
		for i in 0..insert_idx {
			tmp_children[i] = internal.children[i].load(Ordering::Relaxed);
		}
		tmp_children[insert_idx] = new_child;
		for i in insert_idx..old_count {
			tmp_children[i + 1] = internal.children[i].load(Ordering::Relaxed);
		}

		if insert_idx > 0 {
			for i in 0..insert_idx - 1 {
				tmp_pivots[i] = internal.pivots[i].load(Ordering::Relaxed);
			}
		}
		tmp_pivots[insert_idx - 1] = pivot;
		for i in (insert_idx - 1)..(old_count - 1) {
			tmp_pivots[i + 1] = internal.pivots[i].load(Ordering::Relaxed);
		}

		// Split the tmp arrays
		let mid = (MAPLE_NODE_SLOTS + 1) / 2;
		let new_pivot = tmp_pivots[mid - 1];

		// Copy the left half (original node)
		for i in 0..mid {
			internal.children[i].store(tmp_children[i], Ordering::Relaxed);
			if i < mid - 1 {
				internal.pivots[i].store(tmp_pivots[i], Ordering::Relaxed);
			}
		}
		internal.count.store(mid, Ordering::Release);

		// Copy the right half (new sibling node)
		let new_count = (MAPLE_NODE_SLOTS + 1) - mid;
		for i in 0..new_count {
			new_internal.children[i].store(tmp_children[mid + i], Ordering::Relaxed);
			if i < new_count - 1 {
				new_internal.pivots[i].store(tmp_pivots[mid + i], Ordering::Relaxed);
			}
		}
		new_internal.count.store(new_count, Ordering::Release);

		(Box::into_raw(Box::new(Node::Internal(new_internal))), new_pivot)
	}

	/// Recursively erase a value from the tree.
	///
	/// Returns the new node pointer, the erased value, and if the node became underfull.
	///
	/// # Safety
	///
	/// Caller must hold write lock and ensure node pointer is valid.
	unsafe fn erase_recursive(&self, node: *mut Node<T>, index: usize) -> (*mut Node<T>, Option<*mut T>, bool) {
		// SAFETY: Caller guarantees node is valid
		if unsafe { (*node).is_leaf() } {
			let leaf = unsafe { (*node).as_leaf() };
			let count = leaf.count.load(Ordering::Relaxed);
			let mut found_idx = None;

			for i in 0..count {
				if index >= leaf.ranges[i].start && index <= leaf.ranges[i].end {
					found_idx = Some(i);
					break;
				}
			}

			if let Some(i) = found_idx {
				let mut new_leaf = leaf.clone();
				let old_val = new_leaf.values[i].load(Ordering::Acquire);

				// Shift remaining entries to the left
				for j in i..count - 1 {
					new_leaf.ranges[j] = new_leaf.ranges[j + 1];
					new_leaf.values[j].store(new_leaf.values[j + 1].load(Ordering::Acquire), Ordering::Release);
				}
				new_leaf.count.fetch_sub(1, Ordering::Release);

				let underfull = new_leaf.count.load(Ordering::Relaxed) < MIN_SLOTS;
				return (Box::into_raw(Box::new(Node::Leaf(new_leaf))), Some(old_val), underfull);
			}

			return (node, None, false);
		} else {
			let internal = unsafe { (*node).as_internal() };
			let child_idx = internal.find_child(index);
			let child = internal.children[child_idx].load(Ordering::Acquire);

			// SAFETY: Child pointer is valid while we hold write lock
			let (new_child, erased_val, _) = unsafe { self.erase_recursive(child, index) };

			if new_child == child {
				return (node, None, false);
			}

			let new_internal = internal.clone();
			new_internal.children[child_idx].store(new_child, Ordering::Release);
			call_rcu(child);

			return (Box::into_raw(Box::new(Node::Internal(new_internal))), erased_val, false);
		}
	}
}

impl<T> Drop for MapleTree<T> {
	fn drop(&mut self) {
		let root = self.root.swap(ptr::null_mut(), Ordering::AcqRel);
		if !root.is_null() {
			// SAFETY: We have exclusive ownership during drop
			unsafe {
				drop_tree_recursive(root);
			}
		}
		rcu_barrier();
	}
}

/// Recursively free all nodes and values in the tree.
///
/// # Safety
///
/// Caller must have exclusive access to the tree and ensure no other threads are accessing these nodes.
unsafe fn drop_tree_recursive<T>(node: *mut Node<T>) {
	if node.is_null() {
		return;
	}

	// SAFETY: Caller guarantees node pointer is valid
	match unsafe { &*node } {
		Node::Internal(internal) => {
			let count = internal.count.load(Ordering::Acquire);
			for i in 0..count {
				let child = internal.children[i].load(Ordering::Acquire);
				// SAFETY: Children are valid while parent exists
				unsafe { drop_tree_recursive(child) };
			}
		}

		Node::Leaf(leaf) => {
			let count = leaf.count.load(Ordering::Acquire);
			for i in 0..count {
				let val = leaf.values[i].load(Ordering::Acquire);
				if !val.is_null() {
					// SAFETY: Values are valid while parent exists
					let _ = unsafe { Box::from_raw(val) };
				}
			}
		}
	}

	// SAFETY: Caller guarantees exclusive access
	let _ = unsafe { Box::from_raw(node) };
}

#[cfg(test)]
mod tests {
	use std::{sync::Arc, thread};

	use super::*;

	#[test]
	fn basic_insert_load() {
		let tree = MapleTree::new();

		tree.store_range(10, 20, 42);

		let result = tree.load(15);
		assert!(result.is_some());
	}

	#[test]
	fn test_node_split() {
		let tree = MapleTree::new();

		for i in 0..20 {
			tree.store_range(i * 10, i * 10 + 5, i);
		}

		for i in 0..20 {
			let result = tree.load(i * 10 + 2);
			assert!(result.is_some());
		}
	}

	#[test]
	fn test_concurrent_reads() {
		let tree = Arc::new(MapleTree::new());

		for i in 0..10 {
			tree.store_range(i * 10, i * 10 + 5, i);
		}

		let mut handles = vec![];
		for _ in 0..4 {
			let tree = Arc::clone(&tree);
			handles.push(thread::spawn(move || {
				for i in 0..10 {
					let result = tree.load(i * 10 + 2);
					assert!(result.is_some());
				}
			}));
		}

		for handle in handles {
			handle.join().unwrap();
		}
	}

	#[test]
	fn test_erase() {
		let tree = MapleTree::new();

		tree.store_range(10, 20, 42);
		tree.store_range(30, 40, 84);

		let result = tree.erase(15);
		assert!(result.is_some());

		let result = tree.load(15);
		assert!(result.is_none());

		let result = tree.load(35);
		assert!(result.is_some());
	}

	#[test]
	fn test_store_single_index() {
		let tree = MapleTree::new();

		tree.store(10, 100);
		tree.store(20, 200);
		tree.store(30, 300);

		let result = tree.load(10);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 100);

		let result = tree.load(20);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 200);

		let result = tree.load(30);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 300);

		let result = tree.load(15);
		assert!(result.is_none());
	}

	#[test]
	fn test_store_overwrite() {
		let tree = MapleTree::new();

		tree.store(10, 42);

		let result = tree.load(10);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 42);

		tree.store(10, 84);

		let result = tree.load(10);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 84);
	}

	#[test]
	fn test_concurrent_read_write() {
		let tree = Arc::new(MapleTree::new());

		for i in 0..5 {
			tree.store_range(i * 10, i * 10 + 5, i);
		}

		let tree_clone = Arc::clone(&tree);
		let reader = thread::spawn(move || {
			for _ in 0..100 {
				let _guard = RcuGuard::new();
				for i in 0..5 {
					let _ = tree_clone.load(i * 10 + 2);
				}
			}
		});

		thread::sleep(std::time::Duration::from_millis(10));

		for i in 5..10 {
			tree.store_range(i * 10, i * 10 + 5, i);
		}

		reader.join().unwrap();
	}

	#[test]
	fn test_exact_fit_optimization() {
		let tree = MapleTree::new();

		tree.store_range(10, 20, 42);
		tree.store_range(10, 20, 84);

		let result = tree.load(15);
		assert!(result.is_some());
		assert_eq!(result.copied().unwrap(), 84);
	}
}
