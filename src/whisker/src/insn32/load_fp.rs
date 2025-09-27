use crate::{insn::*, insn32::IType};

pub fn parse_load_fp(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		LOAD_WORD => Some(
			FloatInstruction::Load {
				dst: itype.dst().to_fp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_DOUBLE_WORD => Some(
			DoubleInstruction::Load {
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
	pub const LOAD_WORD: u8 = 0b010;
	pub const LOAD_DOUBLE_WORD: u8 = 0b011;
}
