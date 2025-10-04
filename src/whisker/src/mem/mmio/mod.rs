use std::sync::Arc;

use spin::Mutex;

pub mod clint;
pub mod goldfish_rtc;
pub mod shutdown;
pub mod uart;
pub mod virtio_block;

pub use uart::*;

use crate::cpu::hart::WhiskerHart;

pub trait MMIODevice {
	fn read(&mut self, hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]);
	fn write(&mut self, hart: &mut WhiskerHart, addr: u64, val: &[u8]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MMIOKind {
	UART,
	PLIC,
	VirtioBlock,
	Clint,
	Shutdown,
	GoldfishRTC,
}

#[derive(Clone)]
pub struct MMIO {
	pub kind: MMIOKind,
	pub device: Arc<Mutex<dyn MMIODevice + Send>>,
}

impl MMIO {
	pub fn new(kind: MMIOKind, device: Arc<Mutex<dyn MMIODevice + Send>>) -> Self {
		MMIO { kind, device }
	}

	/// reads bytes from MMIO into `buf`.
	/// `buf` must be the size of the read to do, and must be no larger than a u64.
	pub fn read(&self, hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		debug_assert!(
			{
				let len = buf.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO read size"
		);

		self.device.lock().read(hart, addr, buf)
	}

	/// writes bytes from `val` into MMIO
	/// `val` must be the size of the write, and must be no larger than a u64
	pub fn write(&self, hart: &mut WhiskerHart, addr: u64, val: &[u8]) {
		debug_assert!(
			{
				let len = val.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO write size"
		);

		self.device.lock().write(hart, addr, val)
	}
}
