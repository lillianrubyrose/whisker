use crate::util::extract_bits_32;
use crate::{cpu::hart::WhiskerHart, insn::*, insn32::IType};

pub fn parse_op_imm_32(_: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);

	match itype.func() {
		ADD_IMM_WORD => Some(
			IntInstruction::AddImmediateWord {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm() as i32,
			}
			.into(),
		),
		SHIFT_LEFT_IMM_WORD => {
			let shift_kind = extract_bits_32(itype.imm() as u32, 6, 11) as u8;
			let shift_amt = extract_bits_32(itype.imm() as u32, 0, 5);

			match shift_kind {
				SHIFT_LOGICAL => Some(
					IntInstruction::ShiftLeftLogicalImmediateWord {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				_ => None,
			}
		}
		SHIFT_RIGHT_IMM_WORD => {
			let shift_kind = extract_bits_32(itype.imm() as u32, 6, 11) as u8;
			let shift_amt = extract_bits_32(itype.imm() as u32, 0, 5);
			match shift_kind {
				SHIFT_LOGICAL => Some(
					IntInstruction::ShiftRightLogicalImmediateWord {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				SHIFT_ARITHMETIC => Some(
					IntInstruction::ShiftRightArithmeticImmediateWord {
						dst: itype.dst().to_gp(),
						lhs: itype.src().to_gp(),
						shift_amt,
					}
					.into(),
				),
				_ => None,
			}
		}
		_ => None,
	}
}

pub mod consts {
	pub const ADD_IMM_WORD: u8 = 0b000;
	pub const SHIFT_LEFT_IMM_WORD: u8 = 0b001;
	pub const SHIFT_RIGHT_IMM_WORD: u8 = 0b101;

	pub const SHIFT_LOGICAL: u8 = 0b000000;
	pub const SHIFT_ARITHMETIC: u8 = 0b010000;
}
