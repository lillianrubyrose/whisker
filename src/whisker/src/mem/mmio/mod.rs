use std::sync::{Arc, LazyLock};

use rustc_hash::FxHashMap;
use spin::Mutex;

pub mod uart;
pub mod virtio_block;

pub use uart::*;

use crate::cpu::hart::WhiskerHart;
use crate::error;

pub trait MMIODevice {
	fn read(&mut self, hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]);
	fn write(&mut self, hart: &mut WhiskerHart, addr: u64, val: &[u8]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MMIOKind {
	UART,
	PLIC,
	VirtioBlock,
}

static MMIO_DEVICES: LazyLock<Mutex<FxHashMap<MMIOKind, Arc<Mutex<dyn MMIODevice + Send>>>>> =
	LazyLock::new(|| Mutex::new(FxHashMap::default()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceAlreadyPresentErr(MMIOKind);

pub fn register_mmio(kind: MMIOKind, device: Arc<Mutex<dyn MMIODevice + Send>>) -> Result<(), DeviceAlreadyPresentErr> {
	let mut devices = MMIO_DEVICES.lock();
	if devices.contains_key(&kind) {
		return Err(DeviceAlreadyPresentErr(kind));
	}

	devices.insert(kind, device);
	Ok(())
}

impl MMIOKind {
	/// reads bytes from MMIO into `buf`.
	/// `buf` must be the size of the read to do, and must be no larger than a u64.
	pub fn read(self, hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		debug_assert!(
			{
				let len = buf.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO read size"
		);

		match MMIO_DEVICES.lock().get_mut(&self) {
			Some(device) => device.lock().read(hart, addr, buf),
			None => error!("missing MMIO device {:?}", self),
		}
	}

	/// writes bytes from `val` into MMIO
	/// `val` must be the size of the write, and must be no larger than a u64
	pub fn write(self, hart: &mut WhiskerHart, addr: u64, val: &[u8]) {
		debug_assert!(
			{
				let len = val.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO write size"
		);

		match MMIO_DEVICES.lock().get_mut(&self) {
			Some(device) => device.lock().write(hart, addr, val),
			None => todo!("missing MMIO device?"),
		}
	}
}
