use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::IType,
	ty::SupportedExtensions,
};

pub fn parse_load_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use consts::*;

	// all LOAD-FP instructions need the F extension
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		return None;
	}

	let itype = IType::parse(parcel);
	match itype.func() {
		FLOAT_LOAD_WORD => Some(
			FloatInstruction::LoadWord {
				dst: itype.dst().to_fp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const FLOAT_LOAD_WORD: u8 = 0b010;
}
