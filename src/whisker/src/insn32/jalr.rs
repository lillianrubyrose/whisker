use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
};

pub fn parse_jalr(_cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		JALR => Some(
			IntInstruction::JumpAndLinkRegister {
				link_reg: itype.dst().to_gp(),
				jmp_reg: itype.src().to_gp(),
				jmp_off: itype.imm(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const JALR: u8 = 0b000;
}
