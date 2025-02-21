use crate::insn::Instruction;
use crate::ty::GPRegisterIndex;

#[derive(Debug)]
pub enum CSRInstruction {
	CSRReadWrite {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		csr: u16,
	},
	CSRReadAndSet {
		dst: GPRegisterIndex,
		mask: GPRegisterIndex,
		csr: u16,
	},
	CSRReadAndClear {
		dst: GPRegisterIndex,
		mask: GPRegisterIndex,
		csr: u16,
	},
	CSRReadWriteImm {
		dst: GPRegisterIndex,
		src: u64,
		csr: u16,
	},
	CSRReadAndSetImm {
		dst: GPRegisterIndex,
		mask: u64,
		csr: u16,
	},
	CSRReadAndClearImm {
		dst: GPRegisterIndex,
		mask: u64,
		csr: u16,
	},
}

impl Into<Instruction> for CSRInstruction {
	fn into(self) -> Instruction {
		Instruction::Csr(self)
	}
}
