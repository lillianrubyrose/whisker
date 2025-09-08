use crate::{insn::*, insn32::IType};

pub fn parse_load(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		LOAD_BYTE => Some(
			IntInstruction::LoadByte {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_HALF => Some(
			IntInstruction::LoadHalf {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_WORD => Some(
			IntInstruction::LoadWord {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_DOUBLE_WORD => Some(
			IntInstruction::LoadDoubleWord {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_BYTE_ZERO_EXTEND => Some(
			IntInstruction::LoadByteZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_HALF_ZERO_EXTEND => Some(
			IntInstruction::LoadHalfZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		LOAD_WORD_ZERO_EXTEND => Some(
			IntInstruction::LoadWordZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const LOAD_BYTE: u8 = 0b000;
	pub const LOAD_HALF: u8 = 0b001;
	pub const LOAD_WORD: u8 = 0b010;
	pub const LOAD_DOUBLE_WORD: u8 = 0b011;

	pub const LOAD_BYTE_ZERO_EXTEND: u8 = 0b100;
	pub const LOAD_HALF_ZERO_EXTEND: u8 = 0b101;
	pub const LOAD_WORD_ZERO_EXTEND: u8 = 0b110;
}
