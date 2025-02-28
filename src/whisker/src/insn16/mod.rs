use tracing::trace;

use crate::insn16::ty::CWideImmType;
use crate::{
	cpu::WhiskerCpu,
	insn::{compressed::CompressedInstruction, int::IntInstruction, Instruction},
	insn16::ty::{CAType, CBArithType, CBranchType, CImmType, CJType, CLoadType, CRType, CStackStoreType, CStoreType},
	ty::{GPRegisterIndex, TrapIdx},
	util::extract_bits_16,
};

impl CompressedInstruction {
	pub fn parse_c0(cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
		use consts::opcode::c0::*;

		let ty = extract_bits_16(parcel, 13, 15) as u8;
		match ty {
			ADDI4SPN => {
				let iw = CWideImmType::parse(parcel);
				if iw.imm() == 0 {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else {
					Ok(IntInstruction::AddImmediate {
						dst: iw.dst(),
						lhs: GPRegisterIndex::SP,
						rhs: iw.imm(),
					}
					.into())
				}
			}
			FLD => {
				todo!("FLD (D ext)")
			}
			LOAD_WORD => {
				let cl = CLoadType::parse(parcel);
				Ok(IntInstruction::LoadWord {
					dst: cl.dst(),
					src: cl.src(),
					// the immediate was zero extended so this will never do a sign extension
					src_offset: cl.imm().cast_signed(),
				}
				.into())
			}
			LOAD_DOUBLE_WORD => {
				let cl = CLoadType::parse(parcel);
				Ok(IntInstruction::LoadDoubleWord {
					dst: cl.dst(),
					src: cl.src(),
					// the immediate was zero extended so this will never do a sign extension
					src_offset: cl.imm().cast_signed(),
				}
				.into())
			}
			RESERVED => {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
			FSD => {
				todo!("FSD (D ext)")
			}
			STORE_WORD => {
				let cs = CStoreType::parse(parcel);
				Ok(IntInstruction::StoreWord {
					dst: cs.dst(),
					// the immediate was zero extended so this will never do a sign extension
					dst_offset: cs.imm().cast_signed(),
					src: cs.src(),
				}
				.into())
			}
			STORE_DOUBLE_WORD => {
				let cs = CStoreType::parse(parcel);
				Ok(IntInstruction::StoreDoubleWord {
					dst: cs.dst(),
					// the immediate was zero extended so this will never do a sign extension
					dst_offset: cs.imm().cast_signed(),
					src: cs.src(),
				}
				.into())
			}
			_ => unreachable!(),
		}
	}

	pub fn parse_c1(cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
		let func3 = extract_bits_16(parcel, 13, 15) as u8;

		use consts::opcode::c1::*;
		match func3 {
			ADD_IMM => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO && im.imm() == 0 {
					Ok(CompressedInstruction::Nop.into())
				} else {
					Ok(IntInstruction::AddImmediate {
						dst: im.reg(),
						lhs: im.reg(),
						rhs: im.imm(),
					}
					.into())
				}
			}
			ADDIW => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO {
					// reserved
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else {
					Ok(IntInstruction::AddImmediateWord {
						dst: im.reg(),
						lhs: im.reg(),
						rhs: im.imm() as i32,
					}
					.into())
				}
			}
			LI => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO {
					todo!("HINT")
				} else {
					Ok(IntInstruction::AddImmediate {
						dst: im.reg(),
						lhs: GPRegisterIndex::ZERO,
						rhs: im.imm(),
					}
					.into())
				}
			}
			ADDI16SP_OR_LUI => {
				let im = CImmType::parse(parcel);
				if im.imm() == 0 {
					// reserved
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else if im.reg() == GPRegisterIndex::ZERO {
					todo!("HINT")
				} else if im.reg().as_usize() == 2 {
					Ok(IntInstruction::AddImmediate {
						dst: im.reg(),
						lhs: im.reg(),
						rhs: im.imm(),
					}
					.into())
				} else {
					Ok(IntInstruction::LoadUpperImmediate {
						dst: im.reg(),
						val: im.imm(),
					}
					.into())
				}
			}
			J => {
				let j = CJType::parse(parcel);
				Ok(IntInstruction::JumpAndLink {
					link_reg: GPRegisterIndex::ZERO,
					jmp_off: j.offset(),
				}
				.into())
			}
			BEQZ => {
				let b = CBranchType::parse(parcel);
				Ok(IntInstruction::BranchEqual {
					lhs: b.src(),
					rhs: GPRegisterIndex::ZERO,
					imm: b.offset(),
				}
				.into())
			}
			BNEZ => {
				let b = CBranchType::parse(parcel);
				Ok(IntInstruction::BranchNotEqual {
					lhs: b.src(),
					rhs: GPRegisterIndex::ZERO,
					imm: b.offset(),
				}
				.into())
			}
			MISC_MATH => {
				let func2 = extract_bits_16(parcel, 10, 11) as u8;
				// several branches use both of these views
				let cb = CBArithType::parse(parcel);
				match func2 {
					func2::SRLI => {
						if cb.imm() == 0 {
							todo!("HINT")
						} else {
							Ok(IntInstruction::ShiftRightLogicalImmediate {
								dst: cb.reg(),
								lhs: cb.reg(),
								shift_amt: cb.imm() as u32,
							}
							.into())
						}
					}
					func2::SRAI => {
						if cb.imm() == 0 {
							todo!("HINT")
						} else {
							Ok(IntInstruction::ShiftRightArithmeticImmediate {
								dst: cb.reg(),
								lhs: cb.reg(),
								shift_amt: cb.imm() as u32,
							}
							.into())
						}
					}
					func2::ANDI => Ok(IntInstruction::AndImmediate {
						dst: cb.reg(),
						lhs: cb.reg(),
						rhs: cb.imm(),
					}
					.into()),
					func2::SUB_XOR_OR_AND => {
						let ca = CAType::parse(parcel);
						let is_word = extract_bits_16(parcel, 12, 12) != 0;
						let sub_func2 = extract_bits_16(parcel, 5, 6) as u8;
						if is_word {
							match sub_func2 {
								SUBW => {
									todo!("SUBW (OP-32)")
								}
								ADDW => {
									todo!("ADDW (OP-32)")
								}
								_ => {
									// RESERVED
									cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
									Err(())
								}
							}
						} else {
							match sub_func2 {
								SUB => Ok(IntInstruction::Sub {
									dst: ca.src1(),
									lhs: ca.src1(),
									rhs: ca.src2(),
								}
								.into()),
								XOR => Ok(IntInstruction::Xor {
									dst: ca.src1(),
									lhs: ca.src1(),
									rhs: ca.src2(),
								}
								.into()),
								OR => Ok(IntInstruction::Or {
									dst: ca.src1(),
									lhs: ca.src1(),
									rhs: ca.src2(),
								}
								.into()),
								AND => Ok(IntInstruction::And {
									dst: ca.src1(),
									lhs: ca.src1(),
									rhs: ca.src2(),
								}
								.into()),
								_ => unreachable!(),
							}
						}
					}
					_ => unreachable!(),
				}
			}

			_ => todo!("C1 func3 {func3:#05b}"),
		}
	}

	pub fn parse_c2(cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
		use consts::opcode::c2::*;
		let func3 = extract_bits_16(parcel, 13, 15) as u8;
		match func3 {
			SLLI => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO {
					todo!("HINT")
				} else if im.imm() == 0 {
					todo!("HINT")
				} else {
					Ok(IntInstruction::ShiftLeftLogicalImmediate {
						dst: im.reg(),
						lhs: im.reg(),
						shift_amt: im.imm() as u32,
					}
					.into())
				}
			}
			FLDSP => {
				todo!("FLDSP (F extension)")
			}
			LWSP => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO {
					// reserved
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else {
					Ok(IntInstruction::LoadWord {
						dst: im.reg(),
						src: GPRegisterIndex::SP,
						src_offset: im.imm(),
					}
					.into())
				}
			}
			LDSP => {
				let im = CImmType::parse(parcel);
				if im.reg() == GPRegisterIndex::ZERO {
					// reserved
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else {
					Ok(IntInstruction::LoadDoubleWord {
						dst: im.reg(),
						src: GPRegisterIndex::SP,
						src_offset: im.imm(),
					}
					.into())
				}
			}
			JR_JALR_MV_EBREAK_ADD => {
				let func = extract_bits_16(parcel, 12, 15) as u8;
				let crtype = CRType::parse(parcel);
				match func {
					JR_MV => {
						match (crtype.src1(), crtype.src2()) {
							(GPRegisterIndex::ZERO, GPRegisterIndex::ZERO) => {
								// reserved
								cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
								Err(())
							}
							(GPRegisterIndex::ZERO, _rs2) => {
								todo!("HINT")
							}
							(rs1, GPRegisterIndex::ZERO) => Ok(IntInstruction::JumpAndLinkRegister {
								link_reg: GPRegisterIndex::ZERO,
								jmp_reg: rs1,
								jmp_off: 0,
							}
							.into()),
							(rd, rs2) => Ok(IntInstruction::Add {
								dst: rd,
								lhs: GPRegisterIndex::ZERO,
								rhs: rs2,
							}
							.into()),
						}
					}
					JALR_EBREAK_ADD => match (crtype.src1(), crtype.src2()) {
						(GPRegisterIndex::ZERO, GPRegisterIndex::ZERO) => Ok(IntInstruction::EBreak.into()),
						(GPRegisterIndex::ZERO, _rs2) => {
							todo!("HINT")
						}
						(rs1, GPRegisterIndex::ZERO) => Ok(IntInstruction::JumpAndLinkRegister {
							link_reg: GPRegisterIndex::LINK_REG,
							jmp_reg: rs1,
							jmp_off: 0,
						}
						.into()),
						(rd, rs2) => Ok(IntInstruction::Add {
							dst: rd,
							lhs: rd,
							rhs: rs2,
						}
						.into()),
					},
					_ => unreachable!(),
				}
			}
			FSDSP => {
				todo!("FSDSP (D extension)")
			}
			SWSP => {
				let ss = CStackStoreType::parse(parcel);
				Ok(IntInstruction::StoreWord {
					dst: GPRegisterIndex::SP,
					dst_offset: ss.imm(),
					src: ss.src(),
				}
				.into())
			}
			SDSP => {
				let ss = CStackStoreType::parse(parcel);
				Ok(IntInstruction::StoreDoubleWord {
					dst: GPRegisterIndex::SP,
					dst_offset: ss.imm(),
					src: ss.src(),
				}
				.into())
			}
			_ => unreachable!(),
		}
	}
}

pub fn parse(cpu: &mut WhiskerCpu, parcel: u16) -> Result<Instruction, ()> {
	use consts::opcode::*;

	let opcode_ty = extract_bits_16(parcel, 0, 1) as u8;
	trace!("(C-ext) parcel={parcel:#018b}");
	if parcel.count_zeros() == u16::BITS {
		panic!("Invalid 16-bit instruction. Cannot be all zero bits");
	}
	match opcode_ty {
		C0 => CompressedInstruction::parse_c0(cpu, parcel),
		C1 => CompressedInstruction::parse_c1(cpu, parcel),
		C2 => CompressedInstruction::parse_c2(cpu, parcel),
		_ => unreachable!(),
	}
}

mod ty {
	use crate::{
		ty::GPRegisterIndex,
		util::{extract_bits_16, sign_ext_imm},
	};

	fn extract_smol_reg(parcel: u16, start: u8) -> GPRegisterIndex {
		// small registers are 3 bits
		let reg = extract_bits_16(parcel, start, start + 2) as u8;
		// UNWRAP: any 3 bit value plus 8 is in range
		GPRegisterIndex::new(reg + 8).unwrap()
	}

	fn extract_reg(parcel: u16, start: u8, end: u8) -> GPRegisterIndex {
		GPRegisterIndex::new(extract_bits_16(parcel, start, end) as u8).unwrap()
	}

	#[derive(Debug)]
	pub struct CRType {
		src1: GPRegisterIndex,
		src2: GPRegisterIndex,
	}

	#[allow(unused)]
	impl CRType {
		pub fn parse(parcel: u16) -> Self {
			Self {
				src2: extract_reg(parcel, 2, 6),
				src1: extract_reg(parcel, 7, 11),
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
	}

	#[derive(Debug)]
	pub struct CImmType {
		// NOTE: this is sometimes used as rs1 and sometimes as rd
		reg: GPRegisterIndex,
		imm: i64,
	}

	#[allow(unused)]
	impl CImmType {
		pub fn parse(parcel: u16) -> Self {
			// the format of the immediate depends on the exact instruction
			let ty = extract_bits_16(parcel, 0, 1) as u8;
			let func = extract_bits_16(parcel, 13, 15) as u8;
			let reg = extract_reg(parcel, 7, 11);
			use super::consts::opcode::*;
			let imm = match ty {
				C1 => {
					match func {
						// sign extended 5 bit imm
						c1::ADD_IMM | c1::ADDIW | c1::LI => {
							let imm_0_4 = extract_bits_16(parcel, 2, 6) as u32;
							let imm_5 = extract_bits_16(parcel, 12, 12) as u32;
							sign_ext_imm(imm_5 << 5 | imm_0_4, 5)
						}
						c1::ADDI16SP_OR_LUI => {
							if reg == GPRegisterIndex::SP {
								// ADDI16SP
								let imm_5 = extract_bits_16(parcel, 2, 2) as u32;
								let imm_7_8 = extract_bits_16(parcel, 3, 4) as u32;
								let imm_6 = extract_bits_16(parcel, 5, 5) as u32;
								let imm_4 = extract_bits_16(parcel, 6, 6) as u32;
								let imm_9 = extract_bits_16(parcel, 12, 12) as u32;
								sign_ext_imm(imm_9 << 9 | imm_7_8 << 7 | imm_6 << 6 | imm_5 << 5 | imm_4 << 4, 9)
							} else {
								// LUI
								let imm_12_16 = extract_bits_16(parcel, 2, 6) as u32;
								let imm_17 = extract_bits_16(parcel, 12, 12) as u32;
								sign_ext_imm(imm_17 << 17 | imm_12_16 << 12, 17)
							}
						}
						_ => unreachable!(),
					}
				}
				C2 => match func {
					c2::SLLI => {
						let imm_0_4 = extract_bits_16(parcel, 2, 6);
						let imm_5 = extract_bits_16(parcel, 12, 12);
						// zero extended
						(imm_5 << 5 | imm_0_4) as i64
					}
					c2::LDSP | c2::FLDSP => {
						let imm_6_8 = extract_bits_16(parcel, 2, 4);
						let imm_3_4 = extract_bits_16(parcel, 5, 6);
						let imm_5 = extract_bits_16(parcel, 12, 12);
						(imm_6_8 << 6 | imm_5 << 5 | imm_3_4 << 3) as i64
					}
					c2::LWSP => {
						let imm_6_7 = extract_bits_16(parcel, 2, 3);
						let imm_2_4 = extract_bits_16(parcel, 4, 6);
						let imm_5 = extract_bits_16(parcel, 12, 12);
						(imm_6_7 << 6 | imm_5 << 5 | imm_2_4 << 2) as i64
					}
					_ => unreachable!(),
				},
				// there are no CI type instructions in C0
				_ => unreachable!(),
			};
			Self { reg, imm }
		}

		pub fn reg(&self) -> GPRegisterIndex {
			self.reg
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CStackStoreType {
		src: GPRegisterIndex,
		imm: i64,
	}

	impl CStackStoreType {
		pub fn parse(parcel: u16) -> Self {
			let src = extract_reg(parcel, 2, 6);
			let func = extract_bits_16(parcel, 13, 15) as u8;
			use super::consts::opcode::c2::*;
			let imm = match func {
				FSDSP | SDSP => {
					let imm_6_8 = extract_bits_16(parcel, 7, 9);
					let imm_3_5 = extract_bits_16(parcel, 10, 12);
					(imm_6_8 << 6 | imm_3_5 << 3) as i64
				}
				SWSP => {
					let imm_6_7 = extract_bits_16(parcel, 7, 8);
					let imm_2_5 = extract_bits_16(parcel, 9, 12);
					(imm_6_7 << 6 | imm_2_5 << 2) as i64
				}
				_ => unreachable!(),
			};

			Self { src, imm }
		}

		#[inline]
		pub fn src(&self) -> GPRegisterIndex {
			self.src
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CWideImmType {
		dst: GPRegisterIndex,
		imm: i64,
	}

	impl CWideImmType {
		pub fn parse(parcel: u16) -> Self {
			let imm = ((extract_bits_16(parcel, 6, 6) << 2)
				| (extract_bits_16(parcel, 5, 5) << 3)
				| (extract_bits_16(parcel, 11, 11) << 4)
				| (extract_bits_16(parcel, 12, 12) << 5)
				| (extract_bits_16(parcel, 7, 7) << 6)
				| (extract_bits_16(parcel, 8, 8) << 7)
				| (extract_bits_16(parcel, 9, 9) << 8)
				| (extract_bits_16(parcel, 10, 10) << 9)) as i64;
			let dst = extract_smol_reg(parcel, 2);
			Self { dst, imm }
		}

		#[inline]
		pub fn dst(&self) -> GPRegisterIndex {
			self.dst
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CLoadType {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		imm: u64,
	}

	impl CLoadType {
		pub fn parse(parcel: u16) -> Self {
			// the format of the immediate depends on the exact instruction
			let func = extract_bits_16(parcel, 13, 15) as u8;
			use super::consts::opcode::c0::*;
			// NOTE: imm is ZERO extended not sign extended
			let imm = match func {
				LOAD_WORD => {
					let imm_6 = extract_bits_16(parcel, 5, 5);
					let imm_2 = extract_bits_16(parcel, 6, 6);
					let imm_3_5 = extract_bits_16(parcel, 10, 12);
					(imm_6 << 6 | imm_3_5 << 3 | imm_2 << 2) as u64
				}
				LOAD_DOUBLE_WORD => {
					let imm_6_7 = extract_bits_16(parcel, 5, 6);
					let imm_3_5 = extract_bits_16(parcel, 10, 12);
					(imm_6_7 << 6 | imm_3_5 << 3) as u64
				}
				// TODO: C.FLD not yet implemented
				_ => unreachable!("invalid CLoadType func3 {func:#05b}"),
			};

			Self {
				dst: extract_smol_reg(parcel, 2),
				src: extract_smol_reg(parcel, 7),
				imm,
			}
		}

		#[inline]
		pub fn dst(&self) -> GPRegisterIndex {
			self.dst
		}
		#[inline]
		pub fn src(&self) -> GPRegisterIndex {
			self.src
		}
		#[inline]
		pub fn imm(&self) -> u64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CStoreType {
		dst: GPRegisterIndex,
		src: GPRegisterIndex,
		imm: u64,
	}

	impl CStoreType {
		pub fn parse(parcel: u16) -> Self {
			// the format of the immediate depends on the exact instruction
			let func = extract_bits_16(parcel, 13, 15) as u8;
			use super::consts::opcode::c0::*;
			// NOTE: imm is ZERO extended not sign extended
			let imm = match func {
				STORE_WORD => {
					let imm_6 = extract_bits_16(parcel, 5, 5);
					let imm_2 = extract_bits_16(parcel, 6, 6);
					let imm_3_5 = extract_bits_16(parcel, 10, 12);
					(imm_6 << 6 | imm_3_5 << 3 | imm_2 << 2) as u64
				}
				STORE_DOUBLE_WORD => {
					let imm_6_7 = extract_bits_16(parcel, 5, 6);
					let imm_3_5 = extract_bits_16(parcel, 10, 12);
					(imm_6_7 << 6 | imm_3_5 << 3) as u64
				}
				// TODO: C.FSD not yet implemented
				_ => unreachable!("invalid CStoreType func3 {func:#05b}"),
			};

			Self {
				src: extract_smol_reg(parcel, 2),
				dst: extract_smol_reg(parcel, 7),
				imm,
			}
		}

		#[inline]
		pub fn dst(&self) -> GPRegisterIndex {
			self.dst
		}
		#[inline]
		pub fn src(&self) -> GPRegisterIndex {
			self.src
		}
		#[inline]
		pub fn imm(&self) -> u64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CAType {
		src2: GPRegisterIndex,
		src1: GPRegisterIndex,
	}

	impl CAType {
		pub fn parse(parcel: u16) -> Self {
			Self {
				src2: extract_smol_reg(parcel, 2),
				src1: extract_smol_reg(parcel, 7),
			}
		}

		#[inline]
		pub fn src1(&self) -> GPRegisterIndex {
			self.src1
		}
		#[inline]
		pub fn src2(&self) -> GPRegisterIndex {
			self.src2
		}
	}

	#[derive(Debug)]
	// NOTE: this deviates from the spec because we have a different type
	// for the weird arithmetic instructions that get lumped in here
	pub struct CBranchType {
		src: GPRegisterIndex,
		offset: i64,
	}

	impl CBranchType {
		pub fn parse(parcel: u16) -> Self {
			let imm_5 = extract_bits_16(parcel, 2, 2) as u32;
			let imm_1_2 = extract_bits_16(parcel, 3, 4) as u32;
			let imm_6_7 = extract_bits_16(parcel, 5, 6) as u32;
			let imm_3_4 = extract_bits_16(parcel, 10, 11) as u32;
			let imm_8 = extract_bits_16(parcel, 12, 12) as u32;

			let offset = sign_ext_imm(imm_1_2 << 1 | imm_3_4 << 3 | imm_5 << 5 | imm_6_7 << 6 | imm_8 << 8, 8);

			Self {
				src: extract_smol_reg(parcel, 7),
				offset,
			}
		}

		#[inline]
		pub fn src(&self) -> GPRegisterIndex {
			self.src
		}
		#[inline]
		pub fn offset(&self) -> i64 {
			self.offset
		}
	}

	#[derive(Debug)]
	pub struct CBArithType {
		reg: GPRegisterIndex,
		imm: i64,
	}

	impl CBArithType {
		pub fn parse(parcel: u16) -> Self {
			// the shift types need to not sign extend, but AND does
			let func2 = extract_bits_16(parcel, 10, 11) as u8;
			use super::consts::opcode::c1::*;
			let imm = match func2 {
				func2::ANDI => {
					let imm_0_4 = extract_bits_16(parcel, 2, 6) as u32;
					let imm_5 = extract_bits_16(parcel, 12, 12) as u32;
					sign_ext_imm(imm_5 << 5 | imm_0_4, 5)
				}
				_ => {
					let imm_0_4 = extract_bits_16(parcel, 2, 6);
					let imm_5 = extract_bits_16(parcel, 12, 12);
					(imm_5 << 5 | imm_0_4) as i64
				}
			};

			Self {
				reg: extract_smol_reg(parcel, 7),
				imm,
			}
		}

		#[inline]
		pub fn reg(&self) -> GPRegisterIndex {
			self.reg
		}
		#[inline]
		pub fn imm(&self) -> i64 {
			self.imm
		}
	}

	#[derive(Debug)]
	pub struct CJType {
		offset: i64,
	}

	#[allow(unused)]
	impl CJType {
		pub fn parse(parcel: u16) -> Self {
			let imm_5 = extract_bits_16(parcel, 2, 2) as u32;
			let imm_1_3 = extract_bits_16(parcel, 3, 5) as u32;
			let imm_7 = extract_bits_16(parcel, 6, 6) as u32;
			let imm_6 = extract_bits_16(parcel, 7, 7) as u32;
			let imm_10 = extract_bits_16(parcel, 8, 8) as u32;
			let imm_8_9 = extract_bits_16(parcel, 9, 10) as u32;
			let imm_4 = extract_bits_16(parcel, 11, 11) as u32;
			let imm_11 = extract_bits_16(parcel, 12, 12) as u32;

			let offset = sign_ext_imm(
				imm_1_3 << 1
					| imm_4 << 4 | imm_5 << 5
					| imm_6 << 6 | imm_7 << 7
					| imm_8_9 << 8 | imm_10 << 10
					| imm_11 << 11,
				11,
			);
			Self { offset }
		}

		#[inline]
		pub fn offset(&self) -> i64 {
			self.offset
		}
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
			pub const FLD: u8 = 0b001;
			pub const LOAD_WORD: u8 = 0b010;
			pub const LOAD_DOUBLE_WORD: u8 = 0b011;
			pub const RESERVED: u8 = 0b100;
			pub const FSD: u8 = 0b101;
			pub const STORE_WORD: u8 = 0b110;
			pub const STORE_DOUBLE_WORD: u8 = 0b111;
		}

		pub mod c1 {
			// NOTE: also used for NOP in some invalid cases
			pub const ADD_IMM: u8 = 0b000;
			pub const ADDIW: u8 = 0b001;
			pub const LI: u8 = 0b010;
			pub const ADDI16SP_OR_LUI: u8 = 0b011;
			// these have more function bits encoded in various ways
			pub const MISC_MATH: u8 = 0b100;
			pub const J: u8 = 0b101;
			pub const BEQZ: u8 = 0b110;
			pub const BNEZ: u8 = 0b111;

			pub mod func2 {
				pub const SRLI: u8 = 0b00;
				pub const SRAI: u8 = 0b01;
				pub const ANDI: u8 = 0b10;
				pub const SUB_XOR_OR_AND: u8 = 0b11;
			}

			/// bits for the sub-divisions of func2=0b11, func1=0
			pub const SUB: u8 = 0b00;
			pub const XOR: u8 = 0b01;
			pub const OR: u8 = 0b10;
			pub const AND: u8 = 0b11;

			/// bits for the sub-divisions of func2=0b11, func1=1
			pub const SUBW: u8 = 0b00;
			pub const ADDW: u8 = 0b01;
		}

		pub mod c2 {
			// above insns with entire func4
			pub const JR_MV: u8 = 0b1000;
			pub const JALR_EBREAK_ADD: u8 = 0b1001;

			pub const SLLI: u8 = 0b000;
			pub const FLDSP: u8 = 0b001;
			pub const LWSP: u8 = 0b010;
			pub const LDSP: u8 = 0b011;

			// for JR, EBREAK, and JALR, src2 bits must be X0
			// for EBREAK, src1 bits must be X0
			// for MV and ADD, src1 and src2 bits must not be X0
			pub const JR_JALR_MV_EBREAK_ADD: u8 = 0b100;

			pub const FSDSP: u8 = 0b101;
			pub const SWSP: u8 = 0b110;
			pub const SDSP: u8 = 0b111;
		}
	}
}
