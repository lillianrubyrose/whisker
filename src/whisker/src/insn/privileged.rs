use super::Instruction;

#[derive(Debug)]
pub enum PrivilegedInstruction {
	Mret,
	Sret,
	WaitForInterrupt,
}

impl Into<Instruction> for PrivilegedInstruction {
	fn into(self) -> Instruction {
		Instruction::PrivilegedInstruction(self)
	}
}
