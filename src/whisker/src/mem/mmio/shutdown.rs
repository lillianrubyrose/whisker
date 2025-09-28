use crate::{cpu::hart::HartBreakKind, mem::mmio::MMIODevice};

pub struct ShutdownDevice;

impl MMIODevice for ShutdownDevice {
	fn read(&mut self, _hart: &mut crate::cpu::hart::WhiskerHart, _addr: u64, _buf: &mut [u8]) {
		// noop
	}

	fn write(&mut self, hart: &mut crate::cpu::hart::WhiskerHart, _addr: u64, _val: &[u8]) {
		hart.requested_break = Some(HartBreakKind::MMIOShutdown);
	}
}
