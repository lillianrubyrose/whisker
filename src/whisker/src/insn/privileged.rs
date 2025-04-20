use super::Instruction;

#[derive(Debug)]
pub enum PrivilegedInstruction {
	Mret,
}

impl Into<Instruction> for PrivilegedInstruction {
	fn into(self) -> Instruction {
		Instruction::PrivilegedInstruction(self)
	}
}
