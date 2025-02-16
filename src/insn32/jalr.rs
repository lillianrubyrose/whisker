use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
};

pub fn parse_jalr(_cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		JALR => Ok(IntInstruction::JumpAndLinkRegister {
			link_reg: itype.dst(),
			jmp_reg: itype.src(),
			jmp_off: itype.imm(),
		}
		.into()),
		_ => unimplemented!("JALR func={:#05b}", itype.func()),
	}
}

pub mod consts {
	pub const JALR: u8 = 0b000;
}
