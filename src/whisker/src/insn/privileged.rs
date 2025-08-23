use crate::ty::GPRegisterIndex;

use super::Instruction;

#[derive(Debug, Clone)]
pub enum PrivilegedInstruction {
	Mret,
	Sret,
	WaitForInterrupt,
	Sfence {
		asid: GPRegisterIndex,
		vaddr: GPRegisterIndex,
	},
}

impl Into<Instruction> for PrivilegedInstruction {
	fn into(self) -> Instruction {
		Instruction::PrivilegedInstruction(self)
	}
}
