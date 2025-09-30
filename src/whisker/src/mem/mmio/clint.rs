use std::time::Instant;

use crate::{
	cpu::{WhiskerExecState, hart::WhiskerHart},
	mem::mmio::MMIODevice,
};

pub const CLINT_BASE: u64 = 0x0200_0000;
pub const CLINT_SIZE: u64 = 0x10000;

const MSIP: u64 = CLINT_BASE;
const MSIP_END: u64 = MSIP + 4;

const MTIMECMP: u64 = CLINT_BASE + 0x4000;
const MTIMECMP_END: u64 = MTIMECMP + 8;

pub const MTIME: u64 = CLINT_BASE + 0xBFF8;
#[allow(unused)]
const MTIME_END: u64 = MTIME + 8;

// Frequency in Hz of the tick rate of the clint
// This must be kept in sync with the device tree
const CLINT_TICK_RATE: u64 = 1000000;
const CLINT_TICK_DURATION_MICROS: u128 = ((1_f64 / CLINT_TICK_RATE as f64) * 1_000_000_f64) as u128;

#[derive(Debug)]
pub struct Clint {
	start: Instant,
	ticks: u64,
	mtimecmp: u64,
	pending: bool,
}

impl Clint {
	pub fn new() -> Self {
		let start = Instant::now();
		Self {
			start,
			ticks: 0,
			mtimecmp: 0,
			pending: false,
		}
	}

	pub fn step(&mut self, hart: &mut WhiskerHart) {
		// skip timer interrupts when in single step mode
		if hart.exec_state == WhiskerExecState::Step {
			return;
		}

		let now = Instant::now();
		// FIXME: handle overflow in Duration and in micros
		let micros = now.saturating_duration_since(self.start).as_micros();
		let ticks = micros.div_euclid(CLINT_TICK_DURATION_MICROS);
		self.ticks = ticks as u64;

		hart.mip.set_m_soft_interrupt(self.pending);

		if self.mtimecmp > 0 && self.ticks >= self.mtimecmp {
			hart.mip.set_m_timer_interrupt(true);
		} else {
			hart.mip.set_m_timer_interrupt(false);
		}
	}
}

impl MMIODevice for Clint {
	fn read(&mut self, _hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		let (value, off) = match addr {
			MSIP..=MSIP_END => (u64::from(self.pending), addr - MSIP),
			MTIMECMP..=MTIMECMP_END => (self.mtimecmp, addr - MTIMECMP),
			MTIME => (self.ticks, addr - MTIME),
			_ => return, // do we load fault here lol
		};

		buf.copy_from_slice(&(value >> (off * 8)).to_le_bytes()[..buf.len()]);
	}

	fn write(&mut self, _hart: &mut WhiskerHart, addr: u64, val: &[u8]) {
		let mut bytes = [0u8; 8];
		bytes[..val.len()].copy_from_slice(val);
		let value = u64::from_le_bytes(bytes);

		match addr {
			MSIP => self.pending = (value & 1) != 0,
			MTIMECMP => self.mtimecmp = value,
			MTIME => self.ticks = value,
			_ => {}
		}
	}
}
