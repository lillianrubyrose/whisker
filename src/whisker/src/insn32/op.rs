use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, multiply::MultiplyInstruction, Instruction},
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

		MUL | MULH | MULHSU | MULHU | DIV | DIVU | REM | REMU => {
			if cpu.supported_extensions.has(SupportedExtensions::MULTIPLY) {
				let insn = MultiplyInstruction::parse_op(cpu, rtype)?;
				Ok(insn.into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("OP func={:#014b} | {:#X}", rtype.func(), cpu.pc),
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

	pub const MUL: u16 = 0b0000001000;
	pub const MULH: u16 = 0b0000001001;
	pub const MULHSU: u16 = 0b0000001010;
	pub const MULHU: u16 = 0b0000001011;
	pub const DIV: u16 = 0b0000001100;
	pub const DIVU: u16 = 0b0000001101;
	pub const REM: u16 = 0b0000001110;
	pub const REMU: u16 = 0b0000001111;
}
