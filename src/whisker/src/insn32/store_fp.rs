use crate::{insn::*, insn32::SType};

pub fn parse_store_fp(parcel: u32) -> Option<Instruction> {
	use consts::*;

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
