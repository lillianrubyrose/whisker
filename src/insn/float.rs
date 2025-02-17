use crate::ty::RegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum FloatInstruction {
	FloatLoadWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	FloatStoreWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
}

impl Into<Instruction> for FloatInstruction {
	fn into(self) -> Instruction {
		Instruction::FloatExtension(self)
	}
}
