use std::sync::{LazyLock, Mutex};

use rustc_hash::FxHashMap;

pub mod uart;
pub use uart::*;

use crate::cpu::WhiskerCpu;

pub trait MMIODevice {
	fn read(&mut self, cpu: &mut WhiskerCpu, addr: u64, buf: &mut [u8]);
	fn write(&mut self, cpu: &mut WhiskerCpu, addr: u64, val: &[u8]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MMIOKind {
	UART,
}

static MMIO_DEVICES: LazyLock<Mutex<FxHashMap<MMIOKind, Box<dyn MMIODevice + Send>>>> =
	LazyLock::new(|| Mutex::new(FxHashMap::default()));

pub fn register_mmio(kind: MMIOKind, device: Box<dyn MMIODevice + Send>) -> Result<(), ()> {
	let mut devices = MMIO_DEVICES.lock().unwrap();
	if devices.contains_key(&kind) {
		return Err(());
	}

	devices.insert(kind, device);
	Ok(())
}

impl MMIOKind {
	/// reads bytes from MMIO into `buf`.
	/// `buf` must be the size of the read to do, and must be no larger than a u64.
	pub fn read(self, cpu: &mut WhiskerCpu, addr: u64, buf: &mut [u8]) {
		debug_assert!(
			{
				let len = buf.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO read size"
		);

		match MMIO_DEVICES.lock().unwrap().get_mut(&self) {
			Some(device) => device.read(cpu, addr, buf),
			None => todo!("missing MMIO device?"),
		}
	}

	/// writes bytes from `val` into MMIO
	/// `val` must be the size of the write, and must be no larger than a u64
	pub fn write(self, cpu: &mut WhiskerCpu, addr: u64, val: &[u8]) {
		debug_assert!(
			{
				let len = val.len();
				len == 1 || len == 2 || len == 4 || len == 8
			},
			"invalid MMIO write size"
		);

		match MMIO_DEVICES.lock().unwrap().get_mut(&self) {
			Some(device) => device.write(cpu, addr, val),
			None => todo!("missing MMIO device?"),
		}
	}
}
