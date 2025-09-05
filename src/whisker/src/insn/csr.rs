use crate::cpu::csr::CSRIndex;
use crate::insn::Instruction;
use crate::ty::GPRegisterIndex;

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names, reason = "All CSR instructions read")]
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

impl From<CSRInstruction> for Instruction {
	fn from(val: CSRInstruction) -> Self {
		Instruction::Zicsr(val)
	}
}
