use crate::{
	soft::{double::SoftDouble, float::SoftFloat},
	ty::{FPRegisterIndex, GPRegisterIndex},
};

#[derive(Default, Debug)]
pub struct GPRegisters {
	x: [u64; 32],
}

impl GPRegisters {
	pub fn regs(&self) -> &[u64; 32] {
		&self.x
	}

	pub fn get(&self, index: GPRegisterIndex) -> u64 {
		let index = index.as_usize();
		if index == 0 {
			0
		} else {
			self.x[index]
		}
	}

	pub fn set(&mut self, index: GPRegisterIndex, value: u64) {
		let index = index.as_usize();
		if index == 0 {
			// writes to r0 are ignored
		} else {
			self.x[index] = value;
		}
	}

	/// sets all general purpose registers
	/// NOTE: writes to zero register are ignored
	pub fn set_all(&mut self, regs: &[u64; 32]) {
		self.x[1..].copy_from_slice(&regs[1..]);
	}
}

#[derive(Default, Debug)]
pub struct FPRegisters {
	x: [u64; 32],
}

impl FPRegisters {
	const NAN_BOX_MASK: u64 = 0xFFFFFFFF_00000000;

	pub fn get_raw(&self, index: FPRegisterIndex) -> u64 {
		let index = index.as_usize();
		self.x[index]
	}

	pub fn set_raw(&mut self, index: FPRegisterIndex, value: u64) {
		let index = index.as_usize();
		self.x[index] = value;
	}

	#[allow(unused)]
	pub fn get_double(&self, index: FPRegisterIndex) -> SoftDouble {
		SoftDouble::from_u64(self.get_raw(index))
	}

	#[allow(unused)]
	pub fn set_double(&mut self, index: FPRegisterIndex, val: SoftDouble) {
		self.set_raw(index, val.to_u64());
	}

	pub fn get_float(&self, index: FPRegisterIndex) -> SoftFloat {
		// floats are NaN boxed, they live in the low 32 bits of the reg
		SoftFloat::from_u32(self.get_raw(index) as u32)
	}

	pub fn set_float(&mut self, index: FPRegisterIndex, val: SoftFloat) {
		self.set_raw(index, u64::from(val.to_u32()) | Self::NAN_BOX_MASK);
	}

	pub fn get_all_raw(&self) -> &[u64; 32] {
		&self.x
	}

	pub fn set_all_raw(&mut self, regs: &[u64; 32]) {
		self.x.copy_from_slice(regs);
	}
}
