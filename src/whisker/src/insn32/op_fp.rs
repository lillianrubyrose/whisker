use crate::cpu::hart::WhiskerHart;
use crate::ty::RegisterIndex;
use crate::{insn::*, insn32::RType, soft::RoundingMode, ty::RiscvExtensions};

/// Returns the parsed instruction if it was valid, or None if the instruction could not be decoded.
/// Caller is responsible for error handling in the None case, including producing exceptions.
pub fn parse_op_fp(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let rtype = RType::parse(parcel);

	let rm = RoundingMode::from_u8(rtype.func3())?;

	// FIXME: this is actually a 5 bit func and 2 bit width
	// we could select the correct instruction based on the width
	// rather than encoding the SINGLE
	let func7 = rtype.func7();
	match func7 {
		ADD_SINGLE => Some(
			FloatInstruction::AddSingle {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		SUB_SINGLE => Some(
			FloatInstruction::SubSingle {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		MUL_SINGLE => Some(
			FloatInstruction::MulSingle {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		DIV_SINGLE => Some(
			FloatInstruction::DivSingle {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}
			.into(),
		),
		SIGN_INJECTION_SINGLE => match rtype.func3() {
			sign_injection::SIGN_INJECTION => Some(
				FloatInstruction::SignInjectionSingle {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			sign_injection::SIGN_NOT_INJECTION => Some(
				FloatInstruction::SignNotInjectionSingle {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			sign_injection::SIGN_XOR_INJECTION => Some(
				FloatInstruction::SignXorInjectionSingle {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			_ => None,
		},
		MIN_MAX_SINGLE => match rtype.func3() {
			min_max_single::MIN => Some(
				FloatInstruction::MinSingle {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			min_max_single::MAX => Some(
				FloatInstruction::MaxSingle {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}
				.into(),
			),
			_ => None,
		},
		SQRT_SINGLE => {
			if rtype.src2() == RegisterIndex::ZERO {
				Some(
					FloatInstruction::SqrtSingle {
						dst: rtype.dst().to_fp(),
						val: rtype.src1().to_fp(),
						rm,
					}
					.into(),
				)
			} else {
				None
			}
		}
		CMP_SINGLE => match rtype.func3() {
			cmp_single::EQ => Some(
				FloatInstruction::EqualSingle {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			cmp_single::LESS_EQ => Some(
				FloatInstruction::LessOrEqualSingle {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			cmp_single::LESS_THAN => Some(
				FloatInstruction::LessThanSingle {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}
				.into(),
			),
			_ => None,
		},
		CONVERT_SINGLE_TO_INT => match rtype.src2().as_u8() {
			convert_float::WORD => Some(
				FloatInstruction::ConvertSingleToWord {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
					rm,
				}
				.into(),
			),
			convert_float::UNSIGNED_WORD => Some(
				FloatInstruction::ConvertSingleToWordUnsigned {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
					rm,
				}
				.into(),
			),
			convert_float::DOUBLE_WORD => Some(
				FloatInstruction::ConvertSingleToDoubleWord {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
					rm,
				}
				.into(),
			),
			convert_float::UNSIGNED_DOUBLE_WORD => Some(
				FloatInstruction::ConvertSingleToDoubleWordUnsigned {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
					rm,
				}
				.into(),
			),
			_ => None,
		},
		CONVERT_INT_TO_SINGLE => match rtype.src2().as_u8() {
			convert_float::WORD => Some(
				FloatInstruction::ConvertWordToSingle {
					dst: rtype.dst().to_fp(),
					src: rtype.src1().to_gp(),
					rm,
				}
				.into(),
			),
			convert_float::UNSIGNED_WORD => Some(
				FloatInstruction::ConvertWordUnsignedToSingle {
					dst: rtype.dst().to_fp(),
					src: rtype.src1().to_gp(),
					rm,
				}
				.into(),
			),
			convert_float::DOUBLE_WORD => Some(
				FloatInstruction::ConvertDoubleWordToSingle {
					dst: rtype.dst().to_fp(),
					src: rtype.src1().to_gp(),
					rm,
				}
				.into(),
			),
			convert_float::UNSIGNED_DOUBLE_WORD => Some(
				FloatInstruction::ConvertDoubleWordUnsignedToSingle {
					dst: rtype.dst().to_fp(),
					src: rtype.src1().to_gp(),
					rm,
				}
				.into(),
			),
			_ => None,
		},
		MOVE_TO_INT_CLASS_SINGLE => match rm.as_u8() {
			move_class_single::MOVE if rtype.src2().as_u8() == 0 => Some(
				FloatInstruction::MoveSingleToInteger {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
				}
				.into(),
			),
			move_class_single::CLASS if rtype.src2().as_u8() == 0 => Some(
				FloatInstruction::Class {
					dst: rtype.dst().to_gp(),
					src: rtype.src1().to_fp(),
				}
				.into(),
			),
			_ => None,
		},
		MOVE_TO_FLOAT_SINGLE => match rm.as_u8() {
			move_class_single::MOVE if rtype.src2().as_u8() == 0 => Some(
				FloatInstruction::MoveIntegerToSingle {
					dst: rtype.dst().to_fp(),
					src: rtype.dst().to_gp(),
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
	pub const SIGN_INJECTION_SINGLE: u8 = 0b0010000;
	pub const MIN_MAX_SINGLE: u8 = 0b0010100;
	pub const SQRT_SINGLE: u8 = 0b0101100;
	pub const CMP_SINGLE: u8 = 0b1010000;
	pub const CONVERT_SINGLE_TO_INT: u8 = 0b1100000;
	pub const CONVERT_INT_TO_SINGLE: u8 = 0b1101000;
	pub const MOVE_TO_INT_CLASS_SINGLE: u8 = 0b1110000;
	pub const MOVE_TO_FLOAT_SINGLE: u8 = 0b1111000;

	pub mod sign_injection {
		pub const SIGN_INJECTION: u8 = 0b000;
		pub const SIGN_NOT_INJECTION: u8 = 0b001;
		pub const SIGN_XOR_INJECTION: u8 = 0b010;
	}

	pub mod min_max_single {
		pub const MIN: u8 = 0b000;
		pub const MAX: u8 = 0b001;
	}

	/// these bits are identical for `CONVERT_INT_SINGLE` and `CONVERT_SINGLE_INT`
	pub mod convert_float {
		pub const WORD: u8 = 0b00000;
		pub const UNSIGNED_WORD: u8 = 0b00001;
		pub const DOUBLE_WORD: u8 = 0b00010;
		pub const UNSIGNED_DOUBLE_WORD: u8 = 0b00011;
	}

	pub mod cmp_single {
		pub const LESS_EQ: u8 = 0b000;
		pub const LESS_THAN: u8 = 0b001;
		pub const EQ: u8 = 0b010;
	}

	pub mod move_class_single {
		pub const MOVE: u8 = 0b000;
		pub const CLASS: u8 = 0b001;
	}
}
