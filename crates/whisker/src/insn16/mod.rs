use log::trace;
use ty::{extract_reg, CRType, CSSType};

use crate::{
	cpu::{GPRegisters, WhiskerCpu},
	insn::{compressed::CompressedInstruction, Instruction},
	ty::GPRegisterIndex,
	util::{extract_bits_16, sign_ext_imm},
};

impl CompressedInstruction {
	pub fn parse_c0(parcel: u16) -> Self {
		use consts::opcode::c0::*;

		let ty = extract_bits_16(parcel, 13, 15) as u8;
		match ty {
			ADDI4SPN => {
				let dst = GPRegisterIndex::new(extract_bits_16(parcel, 2, 4) as u8 + 8).unwrap();
				let imm = ((extract_bits_16(parcel, 6, 6) << 2)
					| (extract_bits_16(parcel, 5, 5) << 3)
					| (extract_bits_16(parcel, 11, 11) << 4)
					| (extract_bits_16(parcel, 12, 12) << 5)
					| (extract_bits_16(parcel, 7, 7) << 6)
					| (extract_bits_16(parcel, 8, 8) << 7)
					| (extract_bits_16(parcel, 9, 9) << 8)
					| (extract_bits_16(parcel, 10, 10) << 9)) as u32;

				let imm = sign_ext_imm(imm, 9);
				CompressedInstruction::ADDI4SPN { dst, imm }
			}
			LOAD_WORD => {
				let dst = GPRegisterIndex::new(extract_bits_16(parcel, 2, 4) as u8 + 8).unwrap();
				let src = GPRegisterIndex::new(extract_bits_16(parcel, 7, 9) as u8 + 8).unwrap();
				let imm = ((extract_bits_16(parcel, 5, 6) << 2) | (extract_bits_16(parcel, 10, 12) << 6)) as i64;
				CompressedInstruction::LoadWord {
					dst,
					src,
					src_offset: imm,
					// src_offset: 4i64.wrapping_mul(imm),
				}
			}
			LOAD_DOUBLE_WORD => {
				let dst = GPRegisterIndex::new(extract_bits_16(parcel, 2, 4) as u8 + 8).unwrap();
				let src = GPRegisterIndex::new(extract_bits_16(parcel, 7, 9) as u8 + 8).unwrap();
				let imm = ((extract_bits_16(parcel, 10, 11) << 3)
					| (extract_bits_16(parcel, 5, 5) << 5)
					| (extract_bits_16(parcel, 6, 6) << 6)
					| (extract_bits_16(parcel, 12, 12) << 7)) as i64;

				CompressedInstruction::LoadDoubleWord {
					dst,
					src,
					src_offset: imm,
				}
			}
			STORE_WORD => {
				let dst = GPRegisterIndex::new(extract_bits_16(parcel, 7, 9) as u8 + 8).unwrap();
				let src = GPRegisterIndex::new(extract_bits_16(parcel, 2, 4) as u8 + 8).unwrap();
				let imm = ((extract_bits_16(parcel, 5, 6) << 2) | (extract_bits_16(parcel, 10, 12) << 6)) as i64;
				CompressedInstruction::StoreWord {
					dst,
					dst_offset: imm,
					// dst_offset: 4i64.wrapping_mul(imm),
					src,
				}
			}
			STORE_DOUBLE_WORD => {
				let dst = GPRegisterIndex::new(extract_bits_16(parcel, 2, 4) as u8 + 8).unwrap();
				let src = GPRegisterIndex::new(extract_bits_16(parcel, 7, 9) as u8 + 8).unwrap();
				let imm = ((extract_bits_16(parcel, 10, 11) << 3)
					| (extract_bits_16(parcel, 5, 5) << 5)
					| (extract_bits_16(parcel, 6, 6) << 6)
					| (extract_bits_16(parcel, 12, 12) << 7)) as i64;

				CompressedInstruction::StoreDoubleWord {
					dst,
					dst_offset: imm,
					src,
				}
			}
			_ => unreachable!(),
		}
	}

	pub fn parse_c1(parcel: u16) -> Self {
		use consts::opcode::*;

		if parcel == 0b0000000000000001 {
			return CompressedInstruction::Nop;
		}

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
							| (extract_bits_16(parcel, 2, 2) << 5)
							| (extract_bits_16(parcel, 5, 5) << 6)
							| (extract_bits_16(parcel, 3, 3) << 7)
							| (extract_bits_16(parcel, 4, 4) << 8)
							| (extract_bits_16(parcel, 12, 12) << 9)) as u32;

						let imm = sign_ext_imm(imm, 9);

						CompressedInstruction::AddImmediate16ToSP {
							// imm: 16i64.wrapping_mul(imm),
							imm, // FIXME: This value seems right before the multiplication?
						}
					}

					GPRegisterIndex::ZERO => unreachable!("invalid c1 dst {:?}", crtype.dst()),

					// Must be LUI
					dst => {
						let imm = ((extract_bits_16(parcel, 2, 6) as u32) << 12)
							| ((extract_bits_16(parcel, 12, 12) as u32) << 17);
						let imm = sign_ext_imm(imm, 17);
						if imm == 0 {
							unreachable!("invalid instruction: C.LUI imm must be >0");
						}

						CompressedInstruction::LoadUpperImmediate { dst, imm }
					}
				}
			}
			c1::BEQZ => {
				let src = GPRegisterIndex::new(extract_bits_16(parcel, 7, 9) as u8 + 8).unwrap();
				let offset = ((extract_bits_16(parcel, 3, 3) << 1)
					| (extract_bits_16(parcel, 4, 4) << 2)
					| (extract_bits_16(parcel, 10, 10) << 3)
					| (extract_bits_16(parcel, 11, 11) << 4)
					| (extract_bits_16(parcel, 2, 2) << 5)
					| (extract_bits_16(parcel, 5, 5) << 6)
					| (extract_bits_16(parcel, 6, 6) << 7)
					| (extract_bits_16(parcel, 12, 12) << 8)) as u32;
				let offset = sign_ext_imm(offset, 8);
				CompressedInstruction::BranchIfZero { src, offset }
			}
			c1::ADDIW => {
				let offset = ((extract_bits_16(parcel, 2, 2))
					| (extract_bits_16(parcel, 3, 3) << 1)
					| (extract_bits_16(parcel, 4, 4) << 2)
					| (extract_bits_16(parcel, 5, 5) << 3)
					| (extract_bits_16(parcel, 6, 6) << 4)
					| (extract_bits_16(parcel, 12, 12) << 5)) as u32;
				let offset = sign_ext_imm(offset, 5);
				let dst = extract_reg(parcel, 7, 11);
				CompressedInstruction::AddImmediateWord {
					dst,
					rhs: offset as i32,
				}
			}
			c1::LI => {
				let dst = extract_reg(parcel, 7, 11);
				if dst == GPRegisterIndex::ZERO {
					panic!("Invalid instruction: C.LI RD must not be x0");
				}

				let imm = ((extract_bits_16(parcel, 2, 2))
					| (extract_bits_16(parcel, 3, 3) << 1)
					| (extract_bits_16(parcel, 4, 4) << 2)
					| (extract_bits_16(parcel, 5, 5) << 3)
					| (extract_bits_16(parcel, 6, 6) << 4)
					| (extract_bits_16(parcel, 12, 12) << 5)) as u32;
				let imm = sign_ext_imm(imm, 5);
				CompressedInstruction::LoadImmediate { dst, imm }
			}
			c1::J => {
				let offset = ((extract_bits_16(parcel, 3, 3) << 1)
					| (extract_bits_16(parcel, 4, 4) << 2)
					| (extract_bits_16(parcel, 5, 5) << 3)
					| (extract_bits_16(parcel, 11, 11) << 4)
					| (extract_bits_16(parcel, 2, 2) << 5)
					| (extract_bits_16(parcel, 7, 7) << 6)
					| (extract_bits_16(parcel, 6, 6) << 7)
					| (extract_bits_16(parcel, 9, 9) << 8)
					| (extract_bits_16(parcel, 10, 10) << 9)
					| (extract_bits_16(parcel, 8, 8) << 10)
					| (extract_bits_16(parcel, 12, 12) << 11)) as u32;
				let offset = sign_ext_imm(offset, 11);
				CompressedInstruction::Jump { offset }
			}
			_ => unimplemented!("C1 func3: {func3:#05b}"),
		}
	}

	pub fn parse_c2(parcel: u16) -> Self {
		use consts::opcode::c2::*;
		let func3 = extract_bits_16(parcel, 13, 15) as u8;
		match func3 {
			JR_JALR_MV_EBREAK_ADD => {
				let crtype = CRType::parse(parcel);
				match crtype.func() {
					JR_MV => {
						if crtype.src2() == GPRegisterIndex::ZERO {
							CompressedInstruction::JumpToRegister {
								src: extract_reg(parcel, 7, 11),
							}
						} else {
							CompressedInstruction::Move {
								src: crtype.src2(),
								dst: crtype.dst(),
							}
						}
					}
					JALR_EBREAK_ADD => match (crtype.src1(), crtype.src2()) {
						(GPRegisterIndex::ZERO, GPRegisterIndex::ZERO) => todo!("EBREAK"),
						(src1, src2) if src1 != GPRegisterIndex::ZERO && src2 == GPRegisterIndex::ZERO => {
							todo!("JALR")
						}
						(dst, src) if dst != GPRegisterIndex::ZERO && src != GPRegisterIndex::ZERO => {
							CompressedInstruction::Add { src, dst }
						}

						_ => unreachable!("(C-Ext) C2 invald instruction"),
					},
					_ => unreachable!(),
				}
			}

			SDSP => {
				let src2 = extract_reg(parcel, 2, 6);

				let imm =
					((extract_bits_16(parcel, 10, 12) as u16) << 3) | ((extract_bits_16(parcel, 7, 9) as u16) << 6);

				CompressedInstruction::StoreDoubleWordToSP {
					src2,
					offset: imm as i64,
					// offset: 8i64.wrapping_mul(imm as i64),
				}
			}

			LDSP => {
				let dst = extract_reg(parcel, 7, 11);
				let offset = ((extract_bits_16(parcel, 5, 5) << 3)
					| (extract_bits_16(parcel, 6, 6) << 4)
					| (extract_bits_16(parcel, 12, 12) << 5)
					| (extract_bits_16(parcel, 2, 2) << 6)
					| (extract_bits_16(parcel, 3, 3) << 7)
					| (extract_bits_16(parcel, 4, 4) << 8)) as u32;
				CompressedInstruction::LoadDoubleWordFromSP {
					dst,
					offset: offset as i64,
				}
			}

			_ => unimplemented!("C2 op:{func3:#05b}"),
		}
	}
}

pub fn parse(_cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
	use consts::opcode::*;

	let opcode_ty = extract_bits_16(parcel, 0, 1) as u8;
	trace!("(C-ext) parcel={parcel:#018b}");
	if parcel.count_zeros() == u16::BITS {
		panic!("Invalid 16-bit instruction. Cannot be all zero bits");
	}
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

	pub fn extract_reg(parcel: u16, start: u8, end: u8) -> GPRegisterIndex {
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

	#[derive(Debug)]
	#[allow(unused)]
	pub struct CJType {
		offset: u16,
		func: u8,
	}

	#[allow(unused)]
	impl CJType {
		pub fn parse(parcel: u16) -> Self {
			let offset = extract_bits_16(parcel, 2, 12);
			let func = extract_bits_16(parcel, 13, 15) as u8;
			Self { offset, func }
		}

		#[inline]
		pub fn offset(&self) -> u16 {
			self.offset
		}
		#[inline]
		pub fn func(&self) -> u8 {
			self.func
		}
	}

	#[derive(Debug)]
	#[allow(unused)]
	pub struct CSSType {
		src2: GPRegisterIndex,
		imm: u16,
		func: u8,
	}

	#[allow(unused)]
	impl CSSType {
		pub fn parse(parcel: u16) -> Self {
			let src2 = extract_reg(parcel, 2, 6);
			let imm = extract_bits_16(parcel, 7, 12);
			let func = extract_bits_16(parcel, 13, 15) as u8;
			Self { src2, imm, func }
		}

		#[inline]
		pub fn src2(&self) -> GPRegisterIndex {
			self.src2
		}
		#[inline]
		pub fn imm(&self) -> u16 {
			self.imm
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

			pub const ADDIW: u8 = 0b001;
			pub const LI: u8 = 0b010;

			pub const ADDI16SP_OR_LUI: u8 = 0b011;

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

		pub mod c2 {
			// for JR, EBREAK, and JALR, src2 bits must be X0
			// for EBREAK, src1 bits must be X0
			// for MV and ADD, src1 and src2 bits must not be X0
			pub const JR_JALR_MV_EBREAK_ADD: u8 = 0b100;
			// above insns with entire func4
			pub const JR_MV: u8 = 0b1000;
			pub const JALR_EBREAK_ADD: u8 = 0b1001;

			pub const SDSP: u8 = 0b111;
			pub const LDSP: u8 = 0b011;
		}
	}
}
