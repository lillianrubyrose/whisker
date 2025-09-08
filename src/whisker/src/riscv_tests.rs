use bitfield::prelude::*;
use num_conv::prelude::*;

#[bitfields]
#[derive(Debug)]
pub struct RiscTestCommand {
	tohost: U16,
	_pad: U32,
	cmd: U8,
	device: U8,
}

impl RiscTestCommand {
	pub fn get_print_char(&self) -> Option<char> {
		if self.get_cmd() == 1 && self.get_device() == 1 {
			char::from_u32(self.get_tohost().extend())
		} else {
			None
		}
	}

	pub fn passed_test(&self) -> Option<bool> {
		if self.get_cmd() == 0 && self.get_device() == 0 && (self.get_tohost() & 1) == 1 {
			let bits = u64::from_le_bytes(self.inner());
			Some((((bits << 16) >> 16) >> 1) == 0)
		} else {
			None
		}
	}
}
