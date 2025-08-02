use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::SType};

pub fn parse_store(_: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let stype = SType::parse(parcel);

	match stype.func() {
		STORE_BYTE => Some(
			IntInstruction::StoreByte {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			}
			.into(),
		),
		STORE_HALF => Some(
			IntInstruction::StoreHalf {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			}
			.into(),
		),
		STORE_WORD => Some(
			IntInstruction::StoreWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			}
			.into(),
		),
		STORE_DOUBLE_WORD => Some(
			IntInstruction::StoreDoubleWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const STORE_BYTE: u8 = 0b000;
	pub const STORE_HALF: u8 = 0b001;
	pub const STORE_WORD: u8 = 0b010;
	pub const STORE_DOUBLE_WORD: u8 = 0b011;
}
