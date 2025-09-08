use super::Instruction;
use crate::ty::GPRegisterIndex;

#[derive(Debug, Clone, Copy)]
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
		Instruction::Privileged(val)
	}
}
