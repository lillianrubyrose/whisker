use std::io::{self, Write};

use tracing::warn;

use crate::cpu::WhiskerCpu;
use crate::mem::PageBase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MMIOKind {
	UART,
}

impl MMIOKind {
	pub const fn page_base(self) -> PageBase {
		match self {
			MMIOKind::UART => PageBase::from_addr(UART_DATA),
		}
	}

	/// reads bytes from MMIO into `buf`.
	/// `buf` must be the size of the read to do, and must be no larger than a u64.
	pub fn read(self, _cpu: &mut WhiskerCpu, virt_addr: u64, buf: &mut [u8]) {
		debug_assert!({
			let len = buf.len();
			len == 1 || len == 2 || len == 4 || len == 8
		});
		match self {
			MMIOKind::UART => read_uart(virt_addr, buf),
		}
	}

	/// writes bytes from `val` into MMIO
	/// `val` must be the size of the write, and must be no larger than a u64
	pub fn write(self, _cpu: &mut WhiskerCpu, virt_addr: u64, val: &[u8]) {
		debug_assert!({
			let len = val.len();
			len == 1 || len == 2 || len == 4 || len == 8
		});

		match self {
			MMIOKind::UART => write_uart(virt_addr, val),
		}
	}
}

const UART_DATA: u64 = 0x1000_0000;

fn read_uart(virt_addr: u64, _buf: &mut [u8]) {
	match virt_addr {
		UART_DATA => {
			todo!("UART read")
		}
		_ => {
			warn!("read from unknown UART addr {:#018X}", virt_addr);
		}
	}
}

fn write_uart(virt_addr: u64, buf: &[u8]) {
	match virt_addr {
		UART_DATA => {
			let b = buf[0];
			let mut stdout = io::stdout();
			let _ = write!(&stdout, "{}", b as char);
			let _ = stdout.flush();
		}
		_ => {
			warn!("write to unknown UART addr {:#018X}", virt_addr);
		}
	}
}
