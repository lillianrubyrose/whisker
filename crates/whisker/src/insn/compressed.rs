use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug)]
pub enum CompressedInstruction {
	AddImmediate16ToSP { imm: i64 },
	LoadUpperImmediate { dst: GPRegisterIndex, imm: i64 },
}

impl Into<Instruction> for CompressedInstruction {
	fn into(self) -> Instruction {
		Instruction::CompressedExtension(self)
	}
}
