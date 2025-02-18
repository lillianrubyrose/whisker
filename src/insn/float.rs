use crate::ty::RegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum FloatInstruction {
	LoadWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	StoreWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},

	AddSinglePrecision {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	SubSinglePrecision {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
}

impl Into<Instruction> for FloatInstruction {
	fn into(self) -> Instruction {
		Instruction::FloatExtension(self)
	}
}
