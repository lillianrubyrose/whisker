use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op_imm_32(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		ADD_IMM_WORD | SHIFT_LEFT_IMM_WORD | SHIFT_RIGHT_IMM_WORD => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(IntInstruction::parse_op_imm_32(itype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("op-imm-32 func: {:#05b}", itype.func()),
	}
}

pub mod consts {
	pub const ADD_IMM_WORD: u8 = 0b000;
	pub const SHIFT_LEFT_IMM_WORD: u8 = 0b001;
	pub const SHIFT_RIGHT_IMM_WORD: u8 = 0b101;

	pub const SHIFT_LOGICAL: u8 = 0b0000000;
	pub const SHIFT_ARITHMETIC: u8 = 0b0100000;
}
