use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::SType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_store(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let stype = SType::parse(parcel);
	match stype.func() {
		STORE_BYTE | STORE_HALF | STORE_WORD | STORE_DOUBLE_WORD => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(IntInstruction::parse_store(stype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("store func: {:#05b}", stype.func()),
	}
}

pub mod consts {
	pub const STORE_BYTE: u8 = 0b000;
	pub const STORE_HALF: u8 = 0b001;
	pub const STORE_WORD: u8 = 0b010;
	pub const STORE_DOUBLE_WORD: u8 = 0b011;
}
