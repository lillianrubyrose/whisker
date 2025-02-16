use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::RType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let rtype = RType::parse(parcel);
	match rtype.func() {
		ADD
		| SUB
		| SHIFT_LEFT_LOGICAL
		| SHIFT_RIGHT_LOGICAL
		| SHIFT_RIGHT_ARITHMETIC
		| AND
		| OR
		| XOR
		| SET_LESS_THAN
		| SET_LESS_THAN_UNSIGNED => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				let insn = IntInstruction::parse_op(rtype);
				Ok(insn.into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!(),
	}
}

pub mod consts {
	pub const ADD: u16 = 0b0000000000;
	pub const SUB: u16 = 0b0100000000;
	pub const SHIFT_LEFT_LOGICAL: u16 = 0b0000000001;
	pub const SET_LESS_THAN: u16 = 0b0000000010;
	pub const SET_LESS_THAN_UNSIGNED: u16 = 0b000000011;
	pub const XOR: u16 = 0b0000000100;
	pub const SHIFT_RIGHT_LOGICAL: u16 = 0b0000000101;
	pub const SHIFT_RIGHT_ARITHMETIC: u16 = 0b0100000101;
	pub const OR: u16 = 0b0000000110;
	pub const AND: u16 = 0b0000000111;
}
