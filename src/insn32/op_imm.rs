use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op_imm(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		ADD_IMM
		| XOR_IMM
		| OR_IMM
		| AND_IMM
		| SHIFT_LEFT_IMM
		| SHIFT_RIGHT_IMM
		| SET_LESS_THAN_IMM
		| SET_LESS_THAN_UNSIGNED_IMM => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(IntInstruction::parse_op_imm(itype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		// exhaustively matched all 3 bits in func3
		_ => unreachable!(),
	}
}

pub mod consts {
	pub const ADD_IMM: u8 = 0b000;
	pub const SHIFT_LEFT_IMM: u8 = 0b001;
	pub const SET_LESS_THAN_IMM: u8 = 0b010;
	pub const SET_LESS_THAN_UNSIGNED_IMM: u8 = 0b011;
	pub const XOR_IMM: u8 = 0b100;
	pub const SHIFT_RIGHT_IMM: u8 = 0b101;
	pub const OR_IMM: u8 = 0b110;
	pub const AND_IMM: u8 = 0b111;

	pub const SHIFT_LOGICAL: u8 = 0b000000;
	pub const SHIFT_ARITHMETIC: u8 = 0b010000;
}
