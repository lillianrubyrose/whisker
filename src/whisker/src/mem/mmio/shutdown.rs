use crate::{
	cpu::hart::{HartBreakKind, WhiskerHart},
	mem::mmio::MMIODevice,
	tracing::warn,
};

const SHUTDOWN: u32 = 0x5555;
const RESET: u32 = 0x7777;

pub struct ShutdownDevice;

impl MMIODevice for ShutdownDevice {
	fn read(&mut self, _hart: &mut WhiskerHart, _addr: u64, _buf: &mut [u8]) {
		// no-op
	}

	fn write(&mut self, hart: &mut WhiskerHart, _addr: u64, val: &[u8]) {
		let val = u32::from_le_bytes(val.try_into().unwrap());
		if (val & 1) != 0 {
			let code = val >> 1;
			if val == SHUTDOWN {
				hart.requested_break = Some(HartBreakKind::MMIOShutdown);
			} else if val == RESET {
				warn!("system reset requested");
				hart.requested_break = Some(HartBreakKind::MMIOShutdown);
			} else {
				warn!("shutdown requested with code: {}", code);
				hart.requested_break = Some(HartBreakKind::MMIOShutdown);
			}
		}
	}
}
