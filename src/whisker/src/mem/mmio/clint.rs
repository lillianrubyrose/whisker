use crate::{cpu::hart::WhiskerHart, mem::mmio::MMIODevice};

pub const CLINT_BASE: u64 = 0x0200_0000;
pub const CLINT_SIZE: u64 = 0x10000;

const MSIP: u64 = CLINT_BASE;
const MSIP_END: u64 = MSIP + 4;

const MTIMECMP: u64 = CLINT_BASE + 0x4000;
const MTIMECMP_END: u64 = MTIMECMP + 8;

const MTIME: u64 = CLINT_BASE + 0xBFF8;
const MTIME_END: u64 = MTIME + 8;

#[derive(Default, Debug)]
pub struct Clint {
	msip: u32,
	mtimecmp: u64,
}

impl Clint {
	pub fn step(&self, hart: &mut WhiskerHart) {
		if (self.msip & 1) != 0 {
			hart.mip.set_m_soft_interrupt(true);
		} else {
			hart.mip.set_m_soft_interrupt(false);
		}

		if self.mtimecmp > 0 && hart.cycles >= self.mtimecmp {
			hart.mip.set_m_timer_interrupt(true);
		} else {
			hart.mip.set_m_timer_interrupt(false);
		}
	}
}

impl MMIODevice for Clint {
	fn read(&mut self, hart: &mut crate::cpu::hart::WhiskerHart, addr: u64, buf: &mut [u8]) {
		let (value, off) = match addr {
			MSIP..=MSIP_END => (self.msip as u64, addr - MSIP),
			MTIMECMP..=MTIMECMP_END => (self.mtimecmp, addr - MTIMECMP),
			MTIME..=MTIME_END => (hart.cycles, addr - MTIME),
			_ => return, // do we load fault here lol
		};

		buf.copy_from_slice(&(value >> (off * 8)).to_le_bytes()[..buf.len()]);
	}

	fn write(&mut self, hart: &mut crate::cpu::hart::WhiskerHart, addr: u64, val: &[u8]) {
		let mut bytes = [0u8; 8];
		bytes[..val.len()].copy_from_slice(val);
		let value = u64::from_le_bytes(bytes);

		match addr {
			MSIP..=MSIP_END => {
				let offset = addr - MSIP;
				let mask = (u64::MAX >> (64 - val.len() * 8)) << (offset * 8);
				self.msip &= !(mask as u32);
				self.msip |= (value << (offset * 8)) as u32;
			}
			MTIMECMP..=MTIMECMP_END => {
				let offset = addr - MTIMECMP;
				let mask = (u64::MAX >> (64 - val.len() * 8)) << (offset * 8);
				self.mtimecmp &= !mask;
				self.mtimecmp |= value << (offset * 8);
			}
			MTIME..=MTIME_END => {
				let offset = addr - MTIME;
				let mask = (u64::MAX >> (64 - val.len() * 8)) << (offset * 8);
				hart.cycles &= !mask;
				hart.cycles |= value << (offset * 8);
			}
			_ => {}
		}
	}
}
