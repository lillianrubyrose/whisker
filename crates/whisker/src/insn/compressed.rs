use super::Instruction;

#[derive(Debug)]
pub enum CompressedInstruction {}

impl Into<Instruction> for CompressedInstruction {
	fn into(self) -> Instruction {
		Instruction::CompressedExtension(self)
	}
}
