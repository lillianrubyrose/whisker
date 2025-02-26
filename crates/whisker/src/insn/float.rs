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
}

impl Into<Instruction> for FloatInstruction {
	fn into(self) -> Instruction {
		Instruction::FloatExtension(self)
	}
}
