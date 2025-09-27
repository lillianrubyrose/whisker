use crate::{insn::*, insn32::SType};

pub fn parse_store_fp(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let stype = SType::parse(parcel);
	match stype.func() {
		STORE_WORD => Some(
			FloatInstruction::Store {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_fp(),
			}
			.into(),
		),
		STORE_DOUBLE_WORD => Some(
			DoubleInstruction::Store {
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
	pub const STORE_WORD: u8 = 0b010;
	pub const STORE_DOUBLE_WORD: u8 = 0b011;
}
