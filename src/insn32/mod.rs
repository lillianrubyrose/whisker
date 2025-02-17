pub mod branch;
pub mod float;
pub mod int;
pub mod jalr;
pub mod load;
pub mod load_fp;
pub mod op;
pub mod op_imm;
pub mod op_imm_32;
pub mod store;
pub mod system;

pub use ty::*;

use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	ty::RegisterIndex,
	util::extract_bits_32,
};

pub fn parse(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	let opcode_ty = extract_bits_32(parcel, 2, 6);
	use consts::opcode::*;
	match opcode_ty {
		LOAD => load::parse_load(cpu, parcel),
		LOAD_FP => load_fp::parse_load_fp(cpu, parcel),
		CUSTOM_0 => todo!("CUSTOM_0"),
		MISC_MEM => todo!("MISC_MEM"),
		OP_IMM => op_imm::parse_op_imm(cpu, parcel),
		AUIPC => {
			let utype = UType::parse(parcel);
			Ok(IntInstruction::AddUpperImmediateToPc {
				dst: utype.dst(),
				val: utype.imm(),
			}
			.into())
		}
		OP_IMM_32 => op_imm_32::parse_op_imm_32(cpu, parcel),
		UNK_48B => todo!("UNK_48B"),
		STORE => store::parse_store(cpu, parcel),
		STORE_FP => todo!("STORE_FP"),
		CUSTOM_1 => todo!("CUSTOM_1"),
		AMO => todo!("AMO"),
		OP => op::parse_op(cpu, parcel),
		LUI => {
			let utype = UType::parse(parcel);
			Ok(IntInstruction::LoadUpperImmediate {
				dst: utype.dst(),
				val: utype.imm(),
			}
			.into())
		}
		OP_32 => todo!("OP_32"),
		UNK_64B => todo!("UNK_64B"),
		MADD => todo!("MADD"),
		MSUB => todo!("MSUB"),
		NMSUB => todo!("NMSUB"),
		NMADD => todo!("NMADD"),
		OP_FP => todo!("OP_FP"),
		OP_V => todo!("OP_V"),
		CUSTOM_2 => todo!("CUSTOM_2"),
		UNK_48B2 => todo!("UNK_48B2"),
		BRANCH => branch::parse_branch(cpu, parcel),
		JALR => jalr::parse_jalr(cpu, parcel),
		RESERVED => todo!("RESERVED"),
		JAL => {
			let jtype = JType::parse(parcel);
			Ok(IntInstruction::JumpAndLink {
				link_reg: jtype.dst(),
				jmp_off: jtype.imm(),
			}
			.into())
		}
		SYSTEM => system::parse_system(cpu, parcel),
		OP_VE => todo!("OP_VE"),
		CUSTOM_3 => todo!("CUSTOM_3"),
		UNK_80B => todo!("UNK_80B"),
		// should have exhaustively matched all possible opcode types
		_ => unreachable!(),
	}
}

pub fn extract_dst(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 7, 11) as u8).unwrap()
}

pub fn extract_src1(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 15, 19) as u8).unwrap()
}

pub fn extract_src2(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 20, 24) as u8).unwrap()
}

/// here to prevent things from using the fields directly
mod ty {
	use super::{extract_dst, extract_src1, extract_src2};
	use crate::{
		ty::RegisterIndex,
		util::{extract_bits_32, sign_ext_imm},
	};

	#[derive(Debug)]
	pub struct IType {
		dst: RegisterIndex,
		src: RegisterIndex,
		func: u8,
		imm: i64,
	}

	impl IType {
		pub fn parse(parcel: u32) -> Self {
			let dst = extract_dst(parcel);
			let func = extract_bits_32(parcel, 12, 14) as u8;
			let src = extract_src1(parcel);
			let imm = extract_bits_32(parcel, 20, 31);
			let imm = sign_ext_imm(imm, 11);
			Self { dst, func, src, imm }
		}

		#[inline]
		pub fn dst(&self) -> RegisterIndex {
			self.dst
		}
		#[inline]
		pub fn src(&self) -> RegisterIndex {
			self.src
		}
		#[inline]
		pub fn func(&self) -> u8 {
			self.func
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct UType {
		dst: RegisterIndex,
		imm: i64,
	}

	impl UType {
		pub fn parse(parcel: u32) -> Self {
			let dst = extract_dst(parcel);
			let imm = extract_bits_32(parcel, 12, 31) << 12;
			let imm = sign_ext_imm(imm, 31);
			Self { dst, imm }
		}

		#[inline]
		pub fn dst(&self) -> RegisterIndex {
			self.dst
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct SType {
		func: u8,
		src1: RegisterIndex,
		src2: RegisterIndex,
		imm: i64,
	}

	impl SType {
		pub fn parse(parcel: u32) -> Self {
			let imm0 = extract_bits_32(parcel, 7, 11);
			let func = extract_bits_32(parcel, 12, 14) as u8;
			let src1 = extract_src1(parcel);
			let src2 = extract_src2(parcel);
			let imm1 = extract_bits_32(parcel, 25, 31);

			let imm = sign_ext_imm(imm1 << 5 | imm0, 11);

			Self { imm, func, src1, src2 }
		}

		#[inline]
		pub fn func(&self) -> u8 {
			self.func
		}
		#[inline]
		pub fn src1(&self) -> RegisterIndex {
			self.src1
		}
		#[inline]
		pub fn src2(&self) -> RegisterIndex {
			self.src2
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct JType {
		dst: RegisterIndex,
		imm: i64,
	}

	impl JType {
		pub fn parse(parcel: u32) -> Self {
			let dst = extract_dst(parcel);
			let imm12_19 = extract_bits_32(parcel, 12, 19);
			let imm11 = extract_bits_32(parcel, 20, 20);
			let imm1_10 = extract_bits_32(parcel, 21, 30);
			let imm20 = extract_bits_32(parcel, 31, 31);

			let imm = imm1_10 << 1 | imm11 << 11 | imm12_19 << 12 | imm20 << 20;
			let imm = sign_ext_imm(imm, 20);

			Self { dst, imm }
		}

		#[inline]
		pub fn dst(&self) -> RegisterIndex {
			self.dst
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct RType {
		func: u16,
		dst: RegisterIndex,
		src1: RegisterIndex,
		src2: RegisterIndex,
	}

	impl RType {
		pub fn parse(parcel: u32) -> Self {
			let dst = extract_dst(parcel);
			let src1 = extract_src1(parcel);
			let src2 = extract_src2(parcel);
			let func3 = extract_bits_32(parcel, 12, 14);
			let func7 = extract_bits_32(parcel, 25, 31);
			Self {
				dst,
				src1,
				src2,
				func: (func3 | func7 << 3) as u16,
			}
		}

		#[inline]
		pub fn func(&self) -> u16 {
			self.func
		}
		#[inline]
		pub fn dst(&self) -> RegisterIndex {
			self.dst
		}
		#[inline]
		pub fn src1(&self) -> RegisterIndex {
			self.src1
		}
		#[inline]
		pub fn src2(&self) -> RegisterIndex {
			self.src2
		}
	}

	#[derive(Debug)]
	pub struct BType {
		func: u8,
		src1: RegisterIndex,
		src2: RegisterIndex,
		imm: i64,
	}

	impl BType {
		pub fn parse(parcel: u32) -> Self {
			let func = extract_bits_32(parcel, 12, 14) as u8;
			let src1 = extract_src1(parcel);
			let src2 = extract_src2(parcel);
			let imm_1_4 = extract_bits_32(parcel, 8, 11);
			let imm_5_10 = extract_bits_32(parcel, 25, 30);
			let imm_11 = extract_bits_32(parcel, 7, 7);
			let imm_12 = extract_bits_32(parcel, 31, 31);

			let imm = sign_ext_imm(imm_12 << 12 | imm_11 << 11 | imm_5_10 << 5 | imm_1_4 << 1, 12);

			Self { imm, func, src1, src2 }
		}

		#[inline]
		pub fn func(&self) -> u8 {
			self.func
		}
		#[inline]
		pub fn src1(&self) -> RegisterIndex {
			self.src1
		}
		#[inline]
		pub fn src2(&self) -> RegisterIndex {
			self.src2
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}
}

mod consts {
	// ==========================================
	// opcode types, from ISA volume 1 chapter 34
	// ==========================================
	pub(super) mod opcode {
		pub const LOAD: u32 = 0b00000;
		pub const LOAD_FP: u32 = 0b000001;
		pub const CUSTOM_0: u32 = 0b00010;
		pub const MISC_MEM: u32 = 0b00011;
		pub const OP_IMM: u32 = 0b00100;
		// ADD UPPER IMMEDIATE TO PC
		pub const AUIPC: u32 = 0b00101;
		pub const OP_IMM_32: u32 = 0b00110;
		// FIXME ??? what is this
		pub const UNK_48B: u32 = 0b00111;
		pub const STORE: u32 = 0b01000;
		pub const STORE_FP: u32 = 0b01001;
		pub const CUSTOM_1: u32 = 0b01010;
		pub const AMO: u32 = 0b01011;
		pub const OP: u32 = 0b01100;
		pub const LUI: u32 = 0b01101;
		pub const OP_32: u32 = 0b01110;
		// FIXME ??? what is this
		pub const UNK_64B: u32 = 0b01111;
		pub const MADD: u32 = 0b10000;
		pub const MSUB: u32 = 0b10001;
		pub const NMSUB: u32 = 0b10010;
		pub const NMADD: u32 = 0b10011;
		pub const OP_FP: u32 = 0b10100;
		pub const OP_V: u32 = 0b10101;
		pub const CUSTOM_2: u32 = 0b10110;
		// FIXME ??? what is this
		pub const UNK_48B2: u32 = 0b10111;
		pub const BRANCH: u32 = 0b11000;
		pub const JALR: u32 = 0b11001;
		pub const RESERVED: u32 = 0b11010;
		pub const JAL: u32 = 0b11011;
		pub const SYSTEM: u32 = 0b11100;
		pub const OP_VE: u32 = 0b11101;
		pub const CUSTOM_3: u32 = 0b11110;
		// FIXME ??? what is this
		pub const UNK_80B: u32 = 0b11111;
	}
}
