use crate::ty::{FPRegisterIndex, GPRegisterIndex};

use super::Instruction;

#[derive(Debug)]
pub enum FloatInstruction {
	LoadWord {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	StoreWord {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: FPRegisterIndex,
	},

	AddSinglePrecision {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	SubSinglePrecision {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
}

impl Into<Instruction> for FloatInstruction {
	fn into(self) -> Instruction {
		Instruction::FloatExtension(self)
	}
}
