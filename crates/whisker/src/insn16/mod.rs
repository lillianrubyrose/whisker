use crate::{
	cpu::WhiskerCpu,
	insn::{compressed::CompressedInstruction, Instruction},
	ty::{RegisterIndex, UnknownRegisterIndex},
	util::extract_bits_16,
};

impl CompressedInstruction {
	pub fn parse_c0(parcel: u16) -> Self {
		use consts::opcode::c0::*;

		let ty = extract_bits_16(parcel, 13, 15) as u8;
		match ty {
			ADDI4SPN => todo!("addi4spn"),
			LOAD_WORD => todo!("load word"),
			LOAD_DOUBLE_WORD => todo!("load dword"),
			STORE_WORD => todo!("load word"),
			STORE_DOUBLE_WORD => todo!("load dword"),
			_ => unreachable!(),
		}
	}

	pub fn parse_c2(parcel: u16) -> Self {
		todo!("parse_c2")
	}
}

pub fn parse(_cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
	use consts::opcode::*;

	let opcode_ty = extract_bits_16(parcel, 0, 1) as u8;
	match opcode_ty {
		C0 => Ok(CompressedInstruction::parse_c0(parcel).into()),
		C1 => todo!("c1"),
		C2 => Ok(CompressedInstruction::parse_c2(parcel).into()),
		_ => unreachable!(),
	}
}

mod consts {
	pub(super) mod opcode {
		pub const C0: u8 = 0b00;
		pub const C1: u8 = 0b01;
		pub const C2: u8 = 0b10;

		pub mod c0 {
			// Add a zero-extended non-zero immediate, scaled by 4, to sp(x2), and writes the result to a gpr
			pub const ADDI4SPN: u8 = 0b000;
			pub const LOAD_WORD: u8 = 0b010;
			pub const LOAD_DOUBLE_WORD: u8 = 0b011;
			pub const STORE_WORD: u8 = 0b110;
			pub const STORE_DOUBLE_WORD: u8 = 0b111;
		}

		pub mod c1 {
			// NOP has 2-15 bits zero'd out
			pub const NOP: u8 = 0b000;
			pub const ADD_IMM: u8 = 0b000;

			pub const JAL: u8 = 0b001;
			pub const LI: u8 = 0b010;

			// ADDI16SP 7-11 = 00010
			pub const ADDI16SP: u8 = 0b011;
			pub const LUI: u8 = 0b011;

			// 10-11 = 00
			pub const SRLI: u8 = 0b100;
			// 10-11 = 01
			pub const SRAI: u8 = 0b100;
			// 10-11 = 10
			pub const ANDI: u8 = 0b100;

			// 10-15 = 100011
			// 6-5 = 00
			pub const SUB: u8 = 0b100;

			// 10-15 = 100011
			// 6-5 = 01
			pub const XOR: u8 = 0b100;

			// 10-15 = 100011
			// 6-5 = 10
			pub const OR: u8 = 0b100;

			// 10-15 = 100011
			// 6-5 = 11
			pub const AND: u8 = 0b100;

			// 10-15 = 100111
			// 6-5 = 00
			pub const SUBW: u8 = 0b100;

			// 10-15 = 100111
			// 6-5 = 01
			pub const ADDW: u8 = 0b100;

			pub const J: u8 = 0b101;
			pub const BEQZ: u8 = 0b110;
			pub const BNEZ: u8 = 0b111;
		}
	}
}
