use crate::{
	soft::RoundingMode,
	ty::{FPRegisterIndex, GPRegisterIndex},
};

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

	AddSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	SubSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	MulSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	DivSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
		rm: RoundingMode,
	},
	SqrtSingle {
		dst: FPRegisterIndex,
		val: FPRegisterIndex,
		rm: RoundingMode,
	},
	// fsgnj.s
	SignInjectionSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	// fsgnjn.s
	SignNotInjectionSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	// fsgnjx.s
	SignXorInjectionSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	MinSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	MaxSingle {
		dst: FPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	EqualSingle {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	LessThanSingle {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},
	LessOrEqualSingle {
		dst: GPRegisterIndex,
		lhs: FPRegisterIndex,
		rhs: FPRegisterIndex,
	},

	MulAddSingle {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		add: FPRegisterIndex,
		rm: RoundingMode,
	},
	MulSubSingle {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		sub: FPRegisterIndex,
		rm: RoundingMode,
	},
	NegMulAddSingle {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		add: FPRegisterIndex,
		rm: RoundingMode,
	},
	NegMulSubSingle {
		dst: FPRegisterIndex,
		mul_lhs: FPRegisterIndex,
		mul_rhs: FPRegisterIndex,
		sub: FPRegisterIndex,
		rm: RoundingMode,
	},
}

impl Into<Instruction> for FloatInstruction {
	fn into(self) -> Instruction {
		Instruction::FloatExtension(self)
	}
}
