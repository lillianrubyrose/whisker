use crate::{insn::*, insn32::RType, ty::RegisterIndex, util::extract_bits_8};

impl AtomicInstruction {}

pub fn parse_amo(parcel: u32) -> Option<Instruction> {
	use consts::*;

	let rtype = RType::parse(parcel);
	match rtype.func3() {
		WORD => parse_word_insn(rtype),
		DWORD => parse_double_word_insn(rtype),
		_ => None,
	}
}

fn parse_word_insn(rtype: RType) -> Option<Instruction> {
	use consts::*;

	let rl = extract_bits_8(rtype.func7(), 0, 0) != 0;
	let aq = extract_bits_8(rtype.func7(), 1, 1) != 0;

	let func5 = extract_bits_8(rtype.func7(), 2, 7);

	match func5 {
		LOAD_RESERVED => {
			if rtype.src2() != RegisterIndex::ZERO {
				return None;
			}

			Some(
				AtomicInstruction::LoadReservedWord {
					src: rtype.src1().to_gp(),
					dst: rtype.dst().to_gp(),
					_aq: aq,
					_rl: rl,
				}
				.into(),
			)
		}
		STORE_CONDITIONAL => Some(
			AtomicInstruction::StoreConditionalWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		SWAP => Some(
			AtomicInstruction::SwapWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		ADD => Some(
			AtomicInstruction::AddWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		XOR => Some(
			AtomicInstruction::XorWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		AND => Some(
			AtomicInstruction::AndWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		OR => Some(
			AtomicInstruction::OrWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MIN => Some(
			AtomicInstruction::MinWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MAX => Some(
			AtomicInstruction::MaxWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MIN_UNSIGNED => Some(
			AtomicInstruction::MinUnsignedWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MAX_UNSIGNED => Some(
			AtomicInstruction::MaxUnsignedWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		_ => None,
	}
}

fn parse_double_word_insn(rtype: RType) -> Option<Instruction> {
	use consts::*;

	let rl = extract_bits_8(rtype.func7(), 0, 0) != 0;
	let aq = extract_bits_8(rtype.func7(), 1, 1) != 0;

	let func5 = extract_bits_8(rtype.func7(), 2, 7);
	match func5 {
		LOAD_RESERVED => {
			if rtype.src2() != RegisterIndex::ZERO {
				return None;
			}

			Some(
				AtomicInstruction::LoadReservedDoubleWord {
					src: rtype.src1().to_gp(),
					dst: rtype.dst().to_gp(),
					_aq: aq,
					_rl: rl,
				}
				.into(),
			)
		}
		STORE_CONDITIONAL => Some(
			AtomicInstruction::StoreConditionalDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		SWAP => Some(
			AtomicInstruction::SwapDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		ADD => Some(
			AtomicInstruction::AddDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		XOR => Some(
			AtomicInstruction::XorDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		AND => Some(
			AtomicInstruction::AndDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		OR => Some(
			AtomicInstruction::OrDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MIN => Some(
			AtomicInstruction::MinDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MAX => Some(
			AtomicInstruction::MaxDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MIN_UNSIGNED => Some(
			AtomicInstruction::MinUnsignedDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		MAX_UNSIGNED => Some(
			AtomicInstruction::MaxUnsignedDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			}
			.into(),
		),
		_ => None,
	}
}

pub mod consts {
	pub const WORD: u8 = 0b010;
	pub const DWORD: u8 = 0b011;

	pub const LOAD_RESERVED: u8 = 0b00010;
	pub const STORE_CONDITIONAL: u8 = 0b00011;
	pub const SWAP: u8 = 0b00001;
	pub const ADD: u8 = 0b00000;
	pub const XOR: u8 = 0b00100;
	pub const AND: u8 = 0b01100;
	pub const OR: u8 = 0b01000;
	pub const MIN: u8 = 0b10000;
	pub const MAX: u8 = 0b10100;
	pub const MIN_UNSIGNED: u8 = 0b11000;
	pub const MAX_UNSIGNED: u8 = 0b11100;
}
