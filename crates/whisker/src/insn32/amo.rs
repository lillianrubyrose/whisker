use crate::{
	cpu::WhiskerCpu,
	insn::{atomic::AtomicInstruction, Instruction},
	insn32::RType,
	ty::{RegisterIndex, TrapIdx},
	util::extract_bits_8,
};

impl AtomicInstruction {
	pub fn parse_word_insn(cpu: &mut WhiskerCpu, rtype: RType) -> Result<Self, ()> {
		use consts::*;

		let rl = extract_bits_8(rtype.func7(), 0, 0) != 0;
		let aq = extract_bits_8(rtype.func7(), 1, 1) != 0;

		let func5 = extract_bits_8(rtype.func7(), 2, 7);

		Ok(match func5 {
			LOAD_RESERVED => {
				if rtype.src2() != RegisterIndex::ZERO {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					return Err(());
				}

				Self::LoadReservedWord {
					src: rtype.src1().to_gp(),
					dst: rtype.dst().to_gp(),
					_aq: aq,
					_rl: rl,
				}
			}
			STORE_CONDITIONAL => Self::StoreConditionalWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			SWAP => Self::SwapWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			ADD => Self::AddWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			XOR => Self::XorWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			AND => Self::AndWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			OR => Self::OrWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MIN => Self::MinWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MAX => Self::MaxWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MIN_UNSIGNED => Self::MinUnsignedWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MAX_UNSIGNED => Self::MaxUnsignedWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			_ => unreachable!(),
		})
	}

	pub fn parse_double_word_insn(cpu: &mut WhiskerCpu, rtype: RType) -> Result<Self, ()> {
		use consts::*;

		let rl = extract_bits_8(rtype.func7(), 0, 0) != 0;
		let aq = extract_bits_8(rtype.func7(), 1, 1) != 0;

		let func5 = extract_bits_8(rtype.func7(), 2, 7);

		Ok(match func5 {
			LOAD_RESERVED => {
				if rtype.src2() != RegisterIndex::ZERO {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					return Err(());
				}

				Self::LoadReservedDoubleWord {
					src: rtype.src1().to_gp(),
					dst: rtype.dst().to_gp(),
					_aq: aq,
					_rl: rl,
				}
			}
			STORE_CONDITIONAL => Self::StoreConditionalDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			SWAP => Self::SwapDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			ADD => Self::AddDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			XOR => Self::XorDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			AND => Self::AndDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			OR => Self::OrDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MIN => Self::MinDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MAX => Self::MaxDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MIN_UNSIGNED => Self::MinUnsignedDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			MAX_UNSIGNED => Self::MaxUnsignedDoubleWord {
				src1: rtype.src1().to_gp(),
				src2: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
				_aq: aq,
				_rl: rl,
			},
			_ => unreachable!(),
		})
	}
}

pub fn parse_amo(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let rtype = RType::parse(parcel);

	match rtype.func3() {
		WORD => Ok(AtomicInstruction::parse_word_insn(cpu, rtype).map(AtomicInstruction::into)?),
		DWORD => Ok(AtomicInstruction::parse_double_word_insn(cpu, rtype).map(AtomicInstruction::into)?),
		_ => unreachable!(),
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
