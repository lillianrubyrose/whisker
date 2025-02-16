use crate::ty::RegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum IntInstruction {
	LoadUpperImmediate {
		dst: RegisterIndex,
		val: i64,
	},
	AddUpperImmediateToPc {
		dst: RegisterIndex,
		val: i64,
	},
	StoreByte {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreHalf {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreDoubleWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},

	LoadByte {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadHalf {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadDoubleWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadByteZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadHalfZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadWordZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	Add {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Sub {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Xor {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Or {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	And {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftLeftLogical {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftRightLogical {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftRightArithmetic {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	SetLessThan {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	SetLessThanUnsigned {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	AddImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	XorImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	OrImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	AndImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	ShiftLeftLogicalImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	SetLessThanImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	SetLessThanUnsignedImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	JumpAndLink {
		link_reg: RegisterIndex,
		jmp_off: i64,
	},
	JumpAndLinkRegister {
		link_reg: RegisterIndex,
		jmp_reg: RegisterIndex,
		jmp_off: i64,
	},
	BranchEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchNotEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchLessThan {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchGreaterEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchLessThanUnsigned {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchGreaterEqualUnsigned {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	AddImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i32,
	},
	ShiftLeftLogicalImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
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
