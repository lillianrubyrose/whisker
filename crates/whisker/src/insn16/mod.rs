use ty::CRType;

use crate::{
	cpu::WhiskerCpu,
	insn::{compressed::CompressedInstruction, Instruction},
	ty::GPRegisterIndex,
	util::{extract_bits_16, sign_ext_imm},
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

	pub fn parse_c1(parcel: u16) -> Self {
		use consts::opcode::*;
		// There are a few C1 instructions that use a func5 instead, probably have to handle those first
		let func3 = extract_bits_16(parcel, 13, 15) as u8;
		match func3 {
			// The difference between ADDI16SP and LUI at a bit level is that ADDI16SP rd must be x2, and LUI rd can't be x0 or x2
			c1::ADDI16SP_OR_LUI => {
				let crtype = CRType::parse(parcel);
				match crtype.dst() {
					// Must be ADDI16SP
					GPRegisterIndex::SP => {
						let imm = ((extract_bits_16(parcel, 6, 6) << 4)
							| (extract_bits_16(parcel, 2, 3) << 5)
							| (extract_bits_16(parcel, 5, 5) << 6)
							| (extract_bits_16(parcel, 12, 12) << 9)) as u32;
						let imm = sign_ext_imm(imm, 10);
						CompressedInstruction::AddImmediate16ToSP { imm }
					}

					GPRegisterIndex::ZERO => unreachable!("invalid c1 dst {:?}", crtype.dst()),

					// Must be LUI
					dst => {
						let imm = ((extract_bits_16(parcel, 2, 6) as u32) << 12)
							| ((extract_bits_16(parcel, 12, 12) as u32) << 17);
						let imm = sign_ext_imm(imm, 18);
						if imm == 0 {
							unreachable!("invalid instruction: C.LUI imm must be >0");
						}

						CompressedInstruction::LoadUpperImmediate { dst, imm }
					}
				}
			}
			_ => unimplemented!("C1 func3: {func3:#05b}"),
		}
	}

	pub fn parse_c2(_parcel: u16) -> Self {
		todo!("parse_c2")
	}
}

pub fn parse(_cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
	use consts::opcode::*;

	let opcode_ty = extract_bits_16(parcel, 0, 1) as u8;
	match opcode_ty {
		C0 => Ok(CompressedInstruction::parse_c0(parcel).into()),
		C1 => Ok(CompressedInstruction::parse_c1(parcel).into()),
		C2 => Ok(CompressedInstruction::parse_c2(parcel).into()),
		_ => unreachable!(),
	}
}

mod ty {
	use crate::{
		ty::{GPRegisterIndex, RegisterIndex},
		util::extract_bits_16,
	};

	fn extract_reg(parcel: u16, start: u8, end: u8) -> GPRegisterIndex {
		RegisterIndex::new(extract_bits_16(parcel, start, end) as u8)
			.unwrap()
			.to_gp()
	}

	#[derive(Debug)]
	#[allow(unused)]
	pub struct CRType {
		src2: GPRegisterIndex,
		src1: GPRegisterIndex,
		func: u8,
	}

	#[allow(unused)]
	impl CRType {
		pub fn parse(parcel: u16) -> Self {
			Self {
				src2: extract_reg(parcel, 2, 6),
				src1: extract_reg(parcel, 7, 11),
				func: extract_bits_16(parcel, 12, 15) as u8,
			}
		}

		#[inline]
		pub fn src2(&self) -> GPRegisterIndex {
			self.src2
		}
		#[inline]
		pub fn src1(&self) -> GPRegisterIndex {
			self.src1
		}
		#[inline]
		pub fn dst(&self) -> GPRegisterIndex {
			self.src1
		}
		#[inline]
		pub fn func(&self) -> u8 {
			self.func
		}
	}
}

#[allow(unused)]
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

			// ADDI16SP rd must be x2
			pub const ADDI16SP_OR_LUI: u8 = 0b011;
			// LUI rd cant be x0 or x2
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
