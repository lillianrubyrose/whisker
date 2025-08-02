use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::RType, ty::SupportedExtensions};

pub fn parse_op_32(hart: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let rtype = RType::parse(parcel);

	match rtype.func() {
		ADD_WORD => Some(
			IntInstruction::AddWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		SUB_WORD => Some(
			IntInstruction::SubWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		SHIFT_LOGICAL_LEFT_WORD => Some(
			IntInstruction::ShiftLeftLogicalWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		SHIFT_LOGICAL_RIGHT_WORD => Some(
			IntInstruction::ShiftRightLogicalWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		SHIFT_ARITHMETIC_RIGHT_WORD => Some(
			IntInstruction::ShiftRightArithmeticWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		MUL_WORD if hart.supports_extensions(SupportedExtensions::MULTIPLY) => Some(
			MultiplyInstruction::MultiplyWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		DIV_WORD if hart.supports_extensions(SupportedExtensions::MULTIPLY) => Some(
			MultiplyInstruction::DivideWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		DIV_UNSIGNED_WORD if hart.supports_extensions(SupportedExtensions::MULTIPLY) => Some(
			MultiplyInstruction::DivideUnsignedWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		REM_WORD if hart.supports_extensions(SupportedExtensions::MULTIPLY) => Some(
			MultiplyInstruction::RemainderWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		REM_UNSIGNED_WORD if hart.supports_extensions(SupportedExtensions::MULTIPLY) => Some(
			MultiplyInstruction::RemainderUnsignedWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const ADD_WORD: u16 = 0b0000000000;
	pub const SUB_WORD: u16 = 0b0100000000;
	pub const SHIFT_LOGICAL_LEFT_WORD: u16 = 0b0000000001;
	pub const SHIFT_LOGICAL_RIGHT_WORD: u16 = 0b0000000101;
	pub const SHIFT_ARITHMETIC_RIGHT_WORD: u16 = 0b0100000101;

	pub const MUL_WORD: u16 = 0b0000001000;
	pub const DIV_WORD: u16 = 0b0000001100;
	pub const DIV_UNSIGNED_WORD: u16 = 0b0000001101;
	pub const REM_WORD: u16 = 0b0000001110;
	pub const REM_UNSIGNED_WORD: u16 = 0b0000001111;
}
