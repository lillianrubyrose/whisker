use crate::ty::SupportedExtensions;
use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::SType,
};

pub fn parse_store_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use consts::*;

	// all STORE-FP instructions need the F extension
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		return None;
	}

	let stype = SType::parse(parcel);
	match stype.func() {
		FLOAT_STORE_WORD => Some(
			FloatInstruction::StoreWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_fp(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const FLOAT_STORE_WORD: u8 = 0b010;
}
