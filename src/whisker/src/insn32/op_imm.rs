use crate::{insn::*, insn32::IType, util::extract_bits_32};

pub fn parse_op_imm(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);

	match itype.func() {
		ADD_IMM => Some(
			IntInstruction::AddImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
		XOR_IMM => Some(
			IntInstruction::XorImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
		OR_IMM => Some(
			IntInstruction::OrImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
		AND_IMM => Some(
			IntInstruction::AndImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
		SHIFT_LEFT_IMM => {
			let shift_kind = extract_bits_32(itype.imm() as u32, 6, 11) as u8;
			let shift_amt = extract_bits_32(itype.imm() as u32, 0, 5);

			match shift_kind {
				SHIFT_LOGICAL => Some(
					IntInstruction::ShiftLeftLogicalImmediate {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				// left shift only has logical (arithmetic is the same)
				_ => None,
			}
		}
		SHIFT_RIGHT_IMM => {
			let shift_kind = extract_bits_32(itype.imm() as u32, 6, 11) as u8;
			let shift_amt = extract_bits_32(itype.imm() as u32, 0, 5);

			match shift_kind {
				SHIFT_LOGICAL => Some(
					IntInstruction::ShiftRightLogicalImmediate {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				SHIFT_ARITHMETIC => Some(
					IntInstruction::ShiftRightArithmeticImmediate {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				_ => None,
			}
		}
		SET_LESS_THAN_IMM => Some(
			IntInstruction::SetLessThanImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
		SET_LESS_THAN_UNSIGNED_IMM => Some(
			IntInstruction::SetLessThanUnsignedImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			}
			.into(),
		),
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
