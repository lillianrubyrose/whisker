use crate::ty::RegisterIndex;
use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::RType,
	soft::RoundingMode,
	ty::SupportedExtensions,
};

/// Returns the parsed instruction if it was valid, or None if the instruction could not be decoded.
/// Caller is responsible for error handling in the None case, including producing exceptions.
pub fn parse_op_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use consts::*;

	// OP-FP type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		return None;
	}

	let rtype = RType::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(rtype.func3()) else {
		return None;
	};
	let func7 = rtype.func7();
	match func7 {
		ADD_SINGLE => Some(
			FloatInstruction::Add {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		SUB_SINGLE => Some(
			FloatInstruction::Sub {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		MUL_SINGLE => Some(
			FloatInstruction::Mul {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		DIV_SINGLE => Some(
			FloatInstruction::Div {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		SQRT_SINGLE => {
			if rtype.src2() != RegisterIndex::ZERO {
				None
			} else {
				Some(
					FloatInstruction::Sqrt {
						dst: rtype.dst().to_fp(),
						val: rtype.src1().to_fp(),
						rm,
					}
					.into(),
				)
			}
		}
		MIN_MAX => match rtype.func3() {
			min_max::MIN => Some(
				FloatInstruction::Min {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			min_max::MAX => Some(
				FloatInstruction::Max {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			_ => None,
		},
		CMP_SINGLE => match rtype.func3() {
			cmp::EQ => Some(
				FloatInstruction::Equal {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			cmp::LESS_EQ => Some(
				FloatInstruction::LessOrEqual {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			cmp::LESS_THAN => Some(
				FloatInstruction::LessThan {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			_ => None,
		},
		_ => unimplemented!("OP-FP func7={func7:#09b}"),
	}
}

pub mod consts {

	pub const ADD_SINGLE: u8 = 0b0000000;
	pub const SUB_SINGLE: u8 = 0b0000100;
	pub const MUL_SINGLE: u8 = 0b0001000;
	pub const DIV_SINGLE: u8 = 0b0001100;
	pub const SQRT_SINGLE: u8 = 0b0101100;
	pub const MIN_MAX: u8 = 0b0010100;
	pub const CMP_SINGLE: u8 = 0b1010000;

	pub mod min_max {
		pub const MIN: u8 = 0b000;
		pub const MAX: u8 = 0b001;
	}

	pub mod cmp {
		pub const LESS_EQ: u8 = 0b000;
		pub const LESS_THAN: u8 = 0b001;
		pub const EQ: u8 = 0b010;
	}
}
