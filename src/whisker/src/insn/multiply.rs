use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum MultiplyInstruction {
	Multiply {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	MultiplyHigh {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	// lhs should be signed, rhs should be unsigned??
	MultiplyHighSignedUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	MultiplyHighUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	Divide {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	DivideUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	Remainder {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	RemainderUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},

	MultiplyWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	DivideWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	DivideUnsignedWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	RemainderWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	RemainderUnsignedWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
}

impl Into<Instruction> for MultiplyInstruction {
	fn into(self) -> Instruction {
		Instruction::MultiplyInstruction(self)
	}
}
