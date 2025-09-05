use super::Instruction;

#[derive(Debug, Clone, Copy)]
pub enum CompressedInstruction {
	Nop,
}

impl From<CompressedInstruction> for Instruction {
	fn from(val: CompressedInstruction) -> Self {
		Instruction::Compressed(val)
	}
}
