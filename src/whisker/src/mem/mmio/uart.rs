use std::io;
use std::io::Write as _;

use tracing::warn;

use crate::cpu::WhiskerCpu;
use crate::mem::mmio::MMIODevice;

pub const UART_DATA: u64 = 0x1000_0000;

pub struct UART {}

impl UART {
	pub fn new() -> Self {
		Self {}
	}
}

impl MMIODevice for UART {
	fn read(&mut self, _cpu: &mut WhiskerCpu, addr: u64, _buf: &mut [u8]) {
		match addr {
			UART_DATA => {
				todo!("UART read")
			}
			_ => {
				warn!("read from unknown UART addr {:#018X}", addr);
			}
		}
	}

	fn write(&mut self, _cpu: &mut WhiskerCpu, addr: u64, buf: &[u8]) {
		match addr {
			UART_DATA => {
				let b = buf[0];
				let mut stdout = io::stdout();
				let _ = write!(&mut stdout, "{}", b as char);
				// FIXME: it would be nice to flush stdout all the time but its VERY slow, reconsider this
				let _ = stdout.flush();
			}
			_ => {
				warn!("write to unknown UART addr {:#018X}", addr);
			}
		}
	}
}
