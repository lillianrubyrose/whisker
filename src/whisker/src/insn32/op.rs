use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::RType, ty::RiscvExtensions};

pub fn parse_op(hart: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let rtype = RType::parse(parcel);
	match rtype.func() {
		ADD => Some(
			IntInstruction::Add {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SUB => Some(
			IntInstruction::Sub {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SHIFT_LEFT_LOGICAL => Some(
			IntInstruction::ShiftLeftLogical {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SHIFT_RIGHT_LOGICAL => Some(
			IntInstruction::ShiftRightLogical {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SHIFT_RIGHT_ARITHMETIC => Some(
			IntInstruction::ShiftRightArithmetic {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		AND => Some(
			IntInstruction::And {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		OR => Some(
			IntInstruction::Or {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		XOR => Some(
			IntInstruction::Xor {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SET_LESS_THAN => Some(
			IntInstruction::SetLessThan {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),
		SET_LESS_THAN_UNSIGNED => Some(
			IntInstruction::SetLessThanUnsigned {
				dst: rtype.dst().to_gp(),
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
			}
			.into(),
		),

		// ==================
		// MULTIPLY
		// ==================
		MUL if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::Multiply {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		MULH if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::MultiplyHigh {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		MULHSU if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::MultiplyHighSignedUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		MULHU if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::MultiplyHighUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		DIV if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::Divide {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		DIVU if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::DivideUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		REM if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::Remainder {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			}
			.into(),
		),
		REMU if hart.supports_extensions(RiscvExtensions::MULTIPLY) => Some(
			MultiplyInstruction::RemainderUnsigned {
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
