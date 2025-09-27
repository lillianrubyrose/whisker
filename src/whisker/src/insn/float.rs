use super::Instruction;
use crate::{
	soft::RoundingMode,
	ty::{FPRegisterIndex, GPRegisterIndex},
};

#[derive(Debug, Clone, Copy)]
pub enum FloatInstruction {
	Load {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	Store {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: FPRegisterIndex,
	},

	Add {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	Sub {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	Mul {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	Div {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	Sqrt {
		dst: FPRegisterIndex,
		val: FPRegisterIndex,
		rm: RoundingMode,
	},
	// fsgnj.s
	SignInjection {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	// fsgnjn.s
	SignNotInjection {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	// fsgnjx.s
	SignXorInjection {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	Min {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	Max {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	Equal {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	LessThan {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	LessOrEqual {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	MulAdd {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		add: FPRegisterIndex,
		rm: RoundingMode,
	},
	MulSub {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		sub: FPRegisterIndex,
		rm: RoundingMode,
	},
	NegMulAdd {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		add: FPRegisterIndex,
		rm: RoundingMode,
	},
	NegMulSub {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		sub: FPRegisterIndex,
		rm: RoundingMode,
	},

	ConvertToWord {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertToWordUnsigned {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertToDoubleWord {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertToDoubleWordUnsigned {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
		rm: RoundingMode,
	},

	ConvertFromWord {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertFromWordUnsigned {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertFromDoubleWord {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		rm: RoundingMode,
	},
	ConvertFromDoubleWordUnsigned {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
		rm: RoundingMode,
	},

	MoveToInteger {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
	},
	MoveFromInteger {
		dst: FPRegisterIndex,
		src: GPRegisterIndex,
	},

	Class {
		dst: GPRegisterIndex,
		src: FPRegisterIndex,
	},
}

impl From<FloatInstruction> for Instruction {
	fn from(val: FloatInstruction) -> Self {
		Instruction::Float(val)
	}
}
