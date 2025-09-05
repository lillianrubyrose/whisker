use super::Instruction;

#[derive(Debug, Clone)]
pub enum CompressedInstruction {
	Nop,
}

impl From<CompressedInstruction> for Instruction {
	fn from(val: CompressedInstruction) -> Self {
		Instruction::Compressed(val)
	}
}
