use crate::cpu::csr::CSRIndex;
use crate::insn::Instruction;
use crate::ty::GPRegisterIndex;

#[derive(Debug)]
pub enum CSRInstruction {
	CSRReadWrite {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		csr: CSRIndex,
	},
	CSRReadAndSet {
		dst: GPRegisterIndex,
		mask: GPRegisterIndex,
		csr: CSRIndex,
	},
	CSRReadAndClear {
		dst: GPRegisterIndex,
		mask: GPRegisterIndex,
		csr: CSRIndex,
	},
	CSRReadWriteImm {
		dst: GPRegisterIndex,
		imm: u64,
		csr: CSRIndex,
	},
	CSRReadAndSetImm {
		dst: GPRegisterIndex,
		mask: u64,
		csr: CSRIndex,
	},
	CSRReadAndClearImm {
		dst: GPRegisterIndex,
		mask: u64,
		csr: CSRIndex,
	},
}

impl Into<Instruction> for CSRInstruction {
	fn into(self) -> Instruction {
		Instruction::Csr(self)
	}
}
