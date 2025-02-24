use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum CompressedInstruction {
	AddImmediate16ToSP {
		imm: i64,
	},
	LoadUpperImmediate {
		dst: GPRegisterIndex,
		imm: i64,
	},
	LoadWord {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	StoreWord {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},
	LoadDoubleWord {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	StoreDoubleWord {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},
	StoreDoubleWordToSP {
		src2: GPRegisterIndex,
		offset: i64,
	},
	LoadDoubleWordFromSP {
		dst: GPRegisterIndex,
		offset: i64,
	},
	ADDI4SPN {
		dst: GPRegisterIndex,
		imm: i64,
	},
	Add {
		src: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	BranchIfZero {
		src: GPRegisterIndex,
		offset: i64,
	},
	BranchIfNotZero {
		src: GPRegisterIndex,
		offset: i64,
	},
	AddImmediateWord {
		dst: GPRegisterIndex,
		rhs: i32,
	},
	Jump {
		offset: i64,
	},
	Nop,
	Move {
		src: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	LoadImmediate {
		dst: GPRegisterIndex,
		imm: i64,
	},
	JumpToRegister {
		src: GPRegisterIndex,
	},
	ShiftRightLogicalImmediate {
		dst: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediate {
		dst: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftLeftLogicalImmediate {
		dst: GPRegisterIndex,
		shift_amt: u32,
	},
}

impl Into<Instruction> for CompressedInstruction {
	fn into(self) -> Instruction {
		Instruction::CompressedExtension(self)
	}
}
