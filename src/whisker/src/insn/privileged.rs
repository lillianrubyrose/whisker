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

impl From<PrivilegedInstruction> for Instruction {
	fn from(val: PrivilegedInstruction) -> Self {
		Instruction::PrivilegedInstruction(val)
	}
}
