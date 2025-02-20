use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_load(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		LOAD_BYTE
		| LOAD_HALF
		| LOAD_WORD
		| LOAD_DOUBLE_WORD
		| LOAD_BYTE_ZERO_EXTEND
		| LOAD_HALF_ZERO_EXTEND
		| LOAD_WORD_ZERO_EXTEND => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(IntInstruction::parse_load(itype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unreachable!("LOAD func={:#05b}", itype.func()),
	}
}

pub mod consts {
	pub const LOAD_BYTE: u8 = 0b000;
	pub const LOAD_HALF: u8 = 0b001;
	pub const LOAD_WORD: u8 = 0b010;
	pub const LOAD_DOUBLE_WORD: u8 = 0b011;

	pub const LOAD_BYTE_ZERO_EXTEND: u8 = 0b100;
	pub const LOAD_HALF_ZERO_EXTEND: u8 = 0b101;
	pub const LOAD_WORD_ZERO_EXTEND: u8 = 0b110;
}
