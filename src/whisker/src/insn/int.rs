use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum IntInstruction {
	LoadUpperImmediate {
		dst: GPRegisterIndex,
		val: i64,
	},
	AddUpperImmediateToPc {
		dst: GPRegisterIndex,
		val: i64,
	},
	StoreByte {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},
	StoreHalf {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},
	StoreWord {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},
	StoreDoubleWord {
		dst: GPRegisterIndex,
		dst_offset: i64,
		src: GPRegisterIndex,
	},

	LoadByte {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadHalf {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadWord {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadDoubleWord {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadByteZeroExtend {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadHalfZeroExtend {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	LoadWordZeroExtend {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		src_offset: i64,
	},
	Add {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	Sub {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	Xor {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	Or {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	And {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	ShiftLeftLogical {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	ShiftRightLogical {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	ShiftRightArithmetic {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	SetLessThan {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	SetLessThanUnsigned {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
	},
	AddImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	XorImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	OrImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	AndImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	ShiftLeftLogicalImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},
	SetLessThanImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	SetLessThanUnsignedImmediate {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i64,
	},
	JumpAndLink {
		link_reg: GPRegisterIndex,
		jmp_off: i64,
	},
	JumpAndLinkRegister {
		link_reg: GPRegisterIndex,
		jmp_reg: GPRegisterIndex,
		jmp_off: i64,
	},
	BranchEqual {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	BranchNotEqual {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	BranchLessThan {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	BranchGreaterEqual {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	BranchLessThanUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	BranchGreaterEqualUnsigned {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		imm: i64,
	},
	AddImmediateWord {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		rhs: i32,
	},
	ShiftLeftLogicalImmediateWord {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediateWord {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediateWord {
		dst: GPRegisterIndex,
		lhs: GPRegisterIndex,
		shift_amt: u32,
	},

	AddWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	SubWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	ShiftLeftLogicalWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	ShiftRightLogicalWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},
	ShiftRightArithmeticWord {
		lhs: GPRegisterIndex,
		rhs: GPRegisterIndex,
		dst: GPRegisterIndex,
	},

	// =========
	// SYSTEM
	// =========
	ECall,
	EBreak,
}

impl Into<Instruction> for IntInstruction {
	fn into(self) -> Instruction {
		Instruction::IntExtension(self)
	}
}
