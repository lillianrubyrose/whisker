use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub(crate) struct SpinLock {
	locked: AtomicBool,
}

impl SpinLock {
	pub const fn new() -> Self {
		Self {
			locked: AtomicBool::new(false),
		}
	}

	/// Acquire the lock, spinning until it's available.
	pub fn lock<'a>(&'a self) -> SpinLockGuard<'a> {
		while self
			.locked
			.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
			.is_err()
		{
			while self.locked.load(Ordering::Relaxed) {
				std::hint::spin_loop();
			}
		}
		SpinLockGuard { lock: self }
	}

	fn unlock(&self) {
		self.locked.store(false, Ordering::Release);
	}
}

/// RAII guard for spinlock.
///
/// The lock is automatically released when this guard is dropped.
pub(crate) struct SpinLockGuard<'a> {
	lock: &'a SpinLock,
}

impl Drop for SpinLockGuard<'_> {
	fn drop(&mut self) {
		self.lock.unlock();
	}
}
