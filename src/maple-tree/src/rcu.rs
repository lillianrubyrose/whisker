use std::{
	cell::RefCell,
	collections::VecDeque,
	sync::atomic::{AtomicUsize, Ordering, fence},
};

use spin::Mutex;

use super::Node;

thread_local! {
	/// This tracks how many RCU read-side critical sections are active on this thread.
	/// Nodes cannot be freed while this is > 0.
	pub(crate) static RCU_READER_COUNT: RefCell<usize> = RefCell::new(0);
}

/// This is incremented each time an RCU grace period completes.
/// Objects are safe to free once two grace periods have passed since they were marked for deletion.
static GRACE_PERIOD_COUNTER: AtomicUsize = AtomicUsize::new(0);
static DEFERRED_FREE_QUEUE: Mutex<VecDeque<DeferredFree>> = Mutex::new(VecDeque::new());

struct DeferredFree {
	ptr: Box<dyn DeferredFreeObject>,
	/// Grace period when this object was queued
	gp: usize,
}

trait DeferredFreeObject: Send + Sync {
	fn free(self: Box<Self>);
}

#[repr(transparent)]
struct SendSyncMutPtr<T>(*mut T);

// SAFETY: We only use this in RCU contexts where synchronization is handled
unsafe impl<T: Send + Sync> Send for SendSyncMutPtr<T> {}
unsafe impl<T: Send + Sync> Sync for SendSyncMutPtr<T> {}

/// A node waiting to be freed.
#[repr(transparent)]
struct DeferredNode<T> {
	ptr: SendSyncMutPtr<Node<T>>,
}

impl<T> DeferredFreeObject for DeferredNode<T> {
	fn free(self: Box<Self>) {
		// SAFETY: We own this pointer and no readers can access it anymore
		unsafe {
			let _ = Box::from_raw(self.ptr.0);
		}
	}
}

pub(crate) struct RcuGuard(/*private*/ ());

impl RcuGuard {
	pub fn new() -> Self {
		rcu_read_lock();
		Self(())
	}
}

impl Drop for RcuGuard {
	fn drop(&mut self) {
		rcu_read_unlock();
	}
}

fn rcu_read_lock() {
	RCU_READER_COUNT.with(|count| {
		let mut c = count.borrow_mut();
		*c += 1;
		if *c == 1 {
			fence(Ordering::SeqCst);
		}
	});
}

fn rcu_read_unlock() {
	RCU_READER_COUNT.with(|count| {
		let mut c = count.borrow_mut();
		*c = c.saturating_sub(1);
		if *c == 0 {
			fence(Ordering::SeqCst);
		}
	});
}

/// Wait for all current readers to finish.
///
/// After this returns, any readers that were active when it was called have
/// completed their critical sections. However, new readers may have started.
fn synchronize_rcu() {
	fence(Ordering::SeqCst);
	// FIXME: Implement a real synchronization mechanism
	std::thread::sleep(std::time::Duration::from_millis(20));
	GRACE_PERIOD_COUNTER.fetch_add(1, Ordering::SeqCst);
	fence(Ordering::SeqCst);
}

/// Queue a node to be freed after RCU grace period.
pub(crate) fn call_rcu<T: 'static>(ptr: *mut Node<T>) {
	let gp = GRACE_PERIOD_COUNTER.load(Ordering::SeqCst);
	let deferred = DeferredFree {
		ptr: Box::new(DeferredNode {
			ptr: SendSyncMutPtr(ptr),
		}),
		gp,
	};

	let mut queue = DEFERRED_FREE_QUEUE.lock();
	queue.push_back(deferred);
}

/// Processes the deferred free queue and frees objects that were queued at least 2 grace periods ago.
pub(crate) fn rcu_barrier() {
	synchronize_rcu();

	let mut queue = DEFERRED_FREE_QUEUE.lock();
	let current_gp = GRACE_PERIOD_COUNTER.load(Ordering::SeqCst);

	while let Some(deferred) = queue.front() {
		if deferred.gp + 2 > current_gp {
			break;
		}

		let deferred = queue.pop_front().unwrap();
		deferred.ptr.free();
	}
}
