use std::{
	alloc::{Layout, alloc, dealloc},
	cell::RefCell,
	marker::PhantomData,
	ptr::{self, NonNull},
};

use crate::{node::Node, tagged::TaggedPtr};

const CHUNK_SIZE: usize = 64 * 1024;

struct ArenaChunk {
	memory: NonNull<u8>,
	layout: Layout,
	pos: usize,
}

impl ArenaChunk {
	fn new() -> Self {
		let layout = Layout::from_size_align(CHUNK_SIZE, 1).unwrap();
		let memory = unsafe { alloc(layout) };
		Self {
			memory: NonNull::new(memory).unwrap_or_else(|| std::alloc::handle_alloc_error(layout)),
			layout,
			pos: 0,
		}
	}

	fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
		let align = layout.align();
		let start = (self.memory.as_ptr() as usize + self.pos + align - 1) & !(align - 1);
		let new_pos = (start - self.memory.as_ptr() as usize) + layout.size();

		if new_pos > CHUNK_SIZE {
			return None;
		}

		self.pos = new_pos;

		// SAFETY: start is always valid because it's aligned to the layout and within the chunk bounds
		Some(unsafe { NonNull::new_unchecked(start as *mut u8) })
	}
}

impl Drop for ArenaChunk {
	fn drop(&mut self) {
		unsafe {
			dealloc(self.memory.as_ptr(), self.layout);
		}
	}
}

pub struct NodeArena<V, const CAPACITY: usize> {
	chunks: RefCell<Vec<ArenaChunk>>,
	_marker: PhantomData<V>,
}

impl<V, const CAPACITY: usize> NodeArena<V, CAPACITY> {
	pub fn new() -> Self {
		Self {
			chunks: RefCell::new(Vec::new()),
			_marker: PhantomData,
		}
	}

	fn alloc_raw(&self, layout: Layout) -> *mut u8 {
		debug_assert!(
			layout.size() < CHUNK_SIZE,
			"Layout size greater than maximum chunk size"
		);

		let mut chunks = self.chunks.borrow_mut();

		if let Some(chunk) = chunks.last_mut() {
			if let Some(ptr) = chunk.alloc(layout) {
				return ptr.as_ptr();
			}
		}

		let mut chunk = ArenaChunk::new();
		let ptr = chunk
			.alloc(layout)
			.expect("chunk to be large enough to fit the passed Layout");
		chunks.push(chunk);
		ptr.as_ptr()
	}

	pub fn alloc_node(&self) -> NonNull<Node<V, CAPACITY>> {
		let layout = Layout::new::<Node<V, CAPACITY>>();
		let ptr = self.alloc_raw(layout);

		// SAFETY: `alloc_raw` will always return a valid pointer
		unsafe { NonNull::new_unchecked(ptr as *mut Node<V, CAPACITY>) }
	}

	pub fn alloc_value(&self, value: V) -> NonNull<V> {
		let layout = Layout::new::<V>();
		let ptr = self.alloc_raw(layout) as *mut V;

		// SAFETY: `alloc_raw` will always return a valid pointer
		unsafe {
			ptr.write(value);
			NonNull::new_unchecked(ptr)
		}
	}

	pub fn clone_node(&self, node: TaggedPtr<Node<V, CAPACITY>>) -> NonNull<Node<V, CAPACITY>> {
		let new = self.alloc_node();

		// SAFETY: `alloc_node` will always return a valid pointer
		unsafe {
			ptr::copy_nonoverlapping(node.ptr(), new.as_ptr(), 1);
		}
		new
	}

	pub fn clone_node_raw(&self, node_ptr: *mut Node<V, CAPACITY>) -> NonNull<Node<V, CAPACITY>> {
		let new = self.alloc_node();

		// SAFETY: `alloc_node` will always return a valid pointer
		unsafe {
			ptr::copy_nonoverlapping(node_ptr, new.as_ptr(), 1);
		}
		new
	}
}
