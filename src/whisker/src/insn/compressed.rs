use super::Instruction;

#[derive(Debug, Clone)]
pub enum CompressedInstruction {
	Nop,
}

impl Into<Instruction> for CompressedInstruction {
	fn into(self) -> Instruction {
		Instruction::CompressedExtension(self)
	}
}
