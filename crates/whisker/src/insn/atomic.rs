use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum AtomicInstruction {
	LoadReservedWord {
		src: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	StoreConditionalWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	SwapWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	AddWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	XorWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	AndWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	OrWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	MinWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	MaxWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	MinUnsignedWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
	MaxUnsignedWord {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
		dst: GPRegisterIndex,
		_aq: bool,
		_rl: bool,
	},
}

impl Into<Instruction> for AtomicInstruction {
	fn into(self) -> Instruction {
		Instruction::AtomicExtension(self)
	}
}
