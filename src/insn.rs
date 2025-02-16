use crate::ty::{RegisterIndex, SupportedExtensions, TrapIdx};
use crate::WhiskerCpu;

#[derive(Debug)]
pub enum IntInstruction {
	LoadUpperImmediate {
		dst: RegisterIndex,
		val: i64,
	},
	AddUpperImmediateToPc {
		dst: RegisterIndex,
		val: i64,
	},
	StoreByte {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreHalf {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},
	StoreDoubleWord {
		dst: RegisterIndex,
		dst_offset: i64,
		src: RegisterIndex,
	},

	LoadByte {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadHalf {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadDoubleWord {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadByteZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadHalfZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	LoadWordZeroExtend {
		dst: RegisterIndex,
		src: RegisterIndex,
		src_offset: i64,
	},
	Add {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Sub {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Xor {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	Or {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	And {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftLeftLogical {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftRightLogical {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	ShiftRightArithmetic {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	SetLessThan {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	SetLessThanUnsigned {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: RegisterIndex,
	},
	AddImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	XorImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	OrImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	AndImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	ShiftLeftLogicalImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	SetLessThanImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	SetLessThanUnsignedImmediate {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i64,
	},
	JumpAndLink {
		link_reg: RegisterIndex,
		jmp_off: i64,
	},
	JumpAndLinkRegister {
		link_reg: RegisterIndex,
		jmp_reg: RegisterIndex,
		jmp_off: i64,
	},
	BranchEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchNotEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchLessThan {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchGreaterEqual {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchLessThanUnsigned {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	BranchGreaterEqualUnsigned {
		lhs: RegisterIndex,
		rhs: RegisterIndex,
		imm: i64,
	},
	AddImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		rhs: i32,
	},
	ShiftLeftLogicalImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightLogicalImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},
	ShiftRightArithmeticImmediateWord {
		dst: RegisterIndex,
		lhs: RegisterIndex,
		shift_amt: u32,
	},

	// =========
	// SYSTEM
	// =========
	ECall,
	EBreak,
}

impl Into<Instruction> for IntInstruction {
	fn into(self) -> Instruction {
		Instruction::IntExtension(self)
	}
}

#[derive(Debug)]
pub enum Instruction {
	IntExtension(IntInstruction),
}

#[derive(Debug)]
struct IType {
	dst: RegisterIndex,
	src: RegisterIndex,
	func: u8,
	imm: i64,
}

#[derive(Debug)]
struct UType {
	dst: RegisterIndex,
	imm: i64,
}

#[derive(Debug)]
struct SType {
	imm: i64,
	func: u8,
	src1: RegisterIndex,
	src2: RegisterIndex,
}

#[derive(Debug)]
struct JType {
	dst: RegisterIndex,
	imm: i64,
}

#[derive(Debug)]
struct RType {
	dst: RegisterIndex,
	src1: RegisterIndex,
	src2: RegisterIndex,
	func: u16,
}

#[derive(Debug)]
struct BType {
	imm: i64,
	func: u8,
	src1: RegisterIndex,
	src2: RegisterIndex,
}

/// extracts bits start..=end from val
fn extract_bits_16(val: u16, start: u8, end: u8) -> u16 {
	assert!(start <= end);
	assert!(start < u16::BITS as u8);
	assert!(end < u16::BITS as u8);

	// masks off the low bits
	let low_mask = (u16::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u16::MAX << (u16::BITS - u32::from(end) - 1)) >> (u16::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
fn extract_bits_32(val: u32, start: u8, end: u8) -> u32 {
	assert!(start <= end);
	assert!(start < u32::BITS as u8);
	assert!(end < u32::BITS as u8);

	// masks off the low bits
	let low_mask = (u32::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u32::MAX << (u32::BITS - u32::from(end) - 1)) >> (u32::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

fn extract_dst(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 7, 11) as u8).unwrap()
}

fn extract_src1(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 15, 19) as u8).unwrap()
}

fn extract_src2(inst: u32) -> RegisterIndex {
	RegisterIndex::new(extract_bits_32(inst, 20, 24) as u8).unwrap()
}

fn sign_ext_imm(imm: u32, sign_bit_idx: u8) -> i64 {
	let sign_mask = 1 << sign_bit_idx;
	let high_bits = if imm & sign_mask != 0 {
		i64::MIN >> (64 - sign_bit_idx - 1)
	} else {
		0
	};
	(imm as i64) | high_bits
}

macro_rules! define_op_int {
    ($parcel:ident, $($const:ident, $inst:ident),*) => {
        {
            use consts::op::*;
            let rtype = Self::parse_rtype($parcel);
            match rtype.func {
                $( $const => IntInstruction::$inst { dst: rtype.dst, lhs: rtype.src1, rhs: rtype.src2 }.into(), )*
                _ => unimplemented!("OP func={:#012b}", rtype.func),
            }
        }
    };
}

macro_rules! define_branch_int {
    ($parcel:ident, $($const:ident, $inst:ident),*) => {
        {
            use consts::branch::*;
            let btype = Self::parse_btype($parcel);
            match btype.func {
                $( $const => IntInstruction::$inst { lhs: btype.src1, rhs: btype.src2, imm: btype.imm }.into(), )*
                _ => unimplemented!("BRANCH func={:#012b}", btype.func),
            }
        }
    };
}

impl Instruction {
	fn parse_itype(parcel: u32) -> IType {
		let dst = extract_dst(parcel);
		let func = extract_bits_32(parcel, 12, 14) as u8;
		let src = extract_src1(parcel);
		let imm = extract_bits_32(parcel, 20, 31);
		let imm = sign_ext_imm(imm, 11);
		IType { dst, func, src, imm }
	}

	fn parse_utype(parcel: u32) -> UType {
		let dst = extract_dst(parcel);
		let imm = extract_bits_32(parcel, 12, 31) << 12;
		let imm = sign_ext_imm(imm, 31);
		UType { dst, imm }
	}

	fn parse_stype(parcel: u32) -> SType {
		let imm0 = extract_bits_32(parcel, 7, 11);
		let func = extract_bits_32(parcel, 12, 14) as u8;
		let src1 = extract_src1(parcel);
		let src2 = extract_src2(parcel);
		let imm1 = extract_bits_32(parcel, 25, 31);

		let imm = sign_ext_imm(imm1 << 5 | imm0, 11);

		SType { imm, func, src1, src2 }
	}

	fn parse_btype(parcel: u32) -> BType {
		let func = extract_bits_32(parcel, 12, 14) as u8;
		let src1 = extract_src1(parcel);
		let src2 = extract_src2(parcel);
		let imm_1_4 = extract_bits_32(parcel, 8, 11);
		let imm_5_10 = extract_bits_32(parcel, 25, 30);
		let imm_11 = extract_bits_32(parcel, 7, 7);
		let imm_12 = extract_bits_32(parcel, 31, 31);

		let imm = sign_ext_imm(imm_12 << 12 | imm_11 << 11 | imm_5_10 << 5 | imm_1_4 << 1, 12);

		BType { imm, func, src1, src2 }
	}

	fn parse_jtype(parcel: u32) -> JType {
		let dst = extract_dst(parcel);
		let imm12_19 = extract_bits_32(parcel, 12, 19);
		let imm11 = extract_bits_32(parcel, 20, 20);
		let imm1_10 = extract_bits_32(parcel, 21, 30);
		let imm20 = extract_bits_32(parcel, 31, 31);

		let imm = imm1_10 << 1 | imm11 << 11 | imm12_19 << 12 | imm20 << 20;
		let imm = sign_ext_imm(imm, 20);

		JType { dst, imm }
	}

	fn parse_rtype(parcel: u32) -> RType {
		let dst = extract_dst(parcel);
		let src1 = extract_src1(parcel);
		let src2 = extract_src2(parcel);
		let func3 = extract_bits_32(parcel, 12, 14);
		let func7 = extract_bits_32(parcel, 25, 31);
		RType {
			dst,
			src1,
			src2,
			func: (func3 | func7 << 3) as u16,
		}
	}

	fn parse_32bit_instruction(parcel: u32) -> Instruction {
		let opcode = extract_bits_32(parcel, 2, 6);
		use consts::opcode::*;
		match opcode {
			LOAD => {
				use consts::load::*;

				let itype = Self::parse_itype(parcel);
				match itype.func {
					LOAD_BYTE => IntInstruction::LoadByte {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_HALF => IntInstruction::LoadHalf {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_WORD => IntInstruction::LoadWord {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_DOUBLE_WORD => IntInstruction::LoadDoubleWord {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_BYTE_ZERO_EXTEND => IntInstruction::LoadByteZeroExtend {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_HALF_ZERO_EXTEND => IntInstruction::LoadHalfZeroExtend {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					LOAD_WORD_ZERO_EXTEND => IntInstruction::LoadWordZeroExtend {
						dst: itype.dst,
						src: itype.src,
						src_offset: itype.imm,
					}
					.into(),
					_ => unreachable!("LOAD func={:#05b}", itype.func),
				}
			}
			LOAD_FP => unimplemented!("LOAD-FP"),
			CUSTOM_0 => unimplemented!("CUSTOM-0"),
			MISC_MEM => unimplemented!("MISC-MEM"),
			OP_IMM => {
				use consts::op_imm::*;
				let itype = Self::parse_itype(parcel);
				match itype.func {
					ADD_IMM => IntInstruction::AddImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					XOR_IMM => IntInstruction::XorImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					OR_IMM => IntInstruction::OrImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					AND_IMM => IntInstruction::AndImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					// these are weird
					SHIFT_LEFT_IMM | SHIFT_RIGHT_IMM => {
						let shift_amt = extract_bits_32(itype.imm as u32, 5, 11) as u32;
						let shift_kind = extract_bits_32(itype.imm as u32, 0, 4) as u8;
						match shift_kind {
							SHIFT_LOGICAL => match itype.func {
								SHIFT_LEFT_IMM => IntInstruction::ShiftLeftLogicalImmediate {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),
								SHIFT_RIGHT_IMM => IntInstruction::ShiftRightLogicalImmediate {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),

								_ => unreachable!(),
							},
							SHIFT_ARITHMETIC => match itype.func {
								SHIFT_RIGHT_IMM => IntInstruction::ShiftRightArithmeticImmediate {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),
								_ => unreachable!(),
							},
							_ => unreachable!("op-imm shift kind: {:#09b}", shift_kind),
						}
					}
					SET_LESS_THAN_IMM => IntInstruction::SetLessThanImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					SET_LESS_THAN_UNSIGNED_IMM => IntInstruction::SetLessThanUnsignedImmediate {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm,
					}
					.into(),
					_ => unreachable!("op-immfunc: {:#013b}", itype.func),
				}
			}
			AUIPC => {
				let utype = Self::parse_utype(parcel);
				IntInstruction::AddUpperImmediateToPc {
					dst: utype.dst,
					val: utype.imm,
				}
				.into()
			}
			OP_IMM_32 => {
				use consts::op_imm_32::*;

				let itype = Self::parse_itype(parcel);
				match itype.func {
					ADD_IMM_WORD => IntInstruction::AddImmediateWord {
						dst: itype.dst,
						lhs: itype.src,
						rhs: itype.imm as i32,
					}
					.into(),

					SHIFT_LEFT_IMM_WORD | SHIFT_RIGHT_IMM_WORD => {
						let shift_amt = extract_bits_32(itype.imm as u32, 5, 11) as u32;
						let shift_kind = extract_bits_32(itype.imm as u32, 0, 4) as u8;
						match shift_kind {
							SHIFT_LOGICAL => match itype.func {
								SHIFT_LEFT_IMM_WORD => IntInstruction::ShiftLeftLogicalImmediateWord {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),
								SHIFT_RIGHT_IMM_WORD => IntInstruction::ShiftRightLogicalImmediateWord {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),

								_ => unreachable!(),
							},
							SHIFT_ARITHMETIC => match itype.func {
								SHIFT_RIGHT_IMM_WORD => IntInstruction::ShiftRightArithmeticImmediateWord {
									dst: itype.dst,
									lhs: itype.src,
									shift_amt,
								}
								.into(),
								_ => unreachable!(),
							},
							_ => unreachable!("op-imm-32 shift kind: {:#09b}", shift_kind),
						}
					}
					_ => unimplemented!("op-imm-32 func: {:#05b}", itype.func),
				}
			}
			UNK_48B => unimplemented!("48b"),
			STORE => {
				use consts::store::*;
				let stype = Self::parse_stype(parcel);
				match stype.func {
					STORE_BYTE => IntInstruction::StoreByte {
						dst: stype.src1,
						dst_offset: stype.imm,
						src: stype.src2,
					}
					.into(),
					STORE_HALF => IntInstruction::StoreHalf {
						dst: stype.src1,
						dst_offset: stype.imm,
						src: stype.src2,
					}
					.into(),
					STORE_WORD => IntInstruction::StoreWord {
						dst: stype.src1,
						dst_offset: stype.imm,
						src: stype.src2,
					}
					.into(),
					STORE_DOUBLE_WORD => IntInstruction::StoreDoubleWord {
						dst: stype.src1,
						dst_offset: stype.imm,
						src: stype.src2,
					}
					.into(),
					_ => unreachable!("STORE func={:#05b}", stype.func),
				}
			}
			STORE_FP => unimplemented!("STORE-FP"),
			CUSTOM_1 => unimplemented!("CUSTOM-1"),
			AMO => unimplemented!("AMO"),
			#[rustfmt::skip]
			OP => {
				define_op_int!(
					parcel,
					ADD, Add,
					SUB, Sub,
					SHIFT_LEFT_LOGICAL, ShiftLeftLogical,
					SHIFT_RIGHT_LOGICAL, ShiftRightLogical,
					SHIFT_RIGHT_ARITHMETIC, ShiftRightArithmetic,
					AND, And,
					OR, Or,
					XOR, Xor,
					SET_LESS_THAN, SetLessThan,
					SET_LESS_THAN_UNSIGNED, SetLessThanUnsigned
				)
			}
			LUI => {
				let utype = Self::parse_utype(parcel);
				IntInstruction::LoadUpperImmediate {
					dst: utype.dst,
					val: utype.imm,
				}
				.into()
			}
			OP_32 => unimplemented!("OP_32"),
			UNK_64B => unimplemented!("64b"),
			MADD => unimplemented!("MADD"),
			MSUB => unimplemented!("MSUB"),
			NMSUB => unimplemented!("NMSUB"),
			NMADD => unimplemented!("NMADD"),
			OP_FP => unimplemented!("OP-FP"),
			OP_V => unimplemented!("OP-V"),
			CUSTOM_2 => unimplemented!("CUSTOM-2"),
			UNK_48B2 => unimplemented!("48b (2)"),
			#[rustfmt::skip]
			BRANCH => {
				define_branch_int!(
					parcel,
					BRANCH_EQ, BranchEqual,
					BRANCH_NEQ, BranchNotEqual,
					BRANCH_LESS_THAN, BranchLessThan,
					BRANCH_GREATER_EQ, BranchGreaterEqual,
					BRANCH_LESS_THAN_UNSIGNED, BranchLessThanUnsigned,
					BRANCH_GREATER_EQ_UNSIGNED, BranchGreaterEqualUnsigned
				)
			}
			JALR => {
				use consts::jalr::*;
				let itype = Self::parse_itype(parcel);
				match itype.func {
					JALR => IntInstruction::JumpAndLinkRegister {
						link_reg: itype.dst,
						jmp_reg: itype.src,
						jmp_off: itype.imm,
					}
					.into(),
					_ => unimplemented!("JALR func={:#05b}", itype.func),
				}
			}
			RESERVED => unimplemented!("RESERVED"),
			JAL => {
				let jtype = Self::parse_jtype(parcel);
				IntInstruction::JumpAndLink {
					link_reg: jtype.dst,
					jmp_off: jtype.imm,
				}
				.into()
			}
			SYSTEM => {
				use consts::system::*;
				let itype = Self::parse_itype(parcel);
				match itype.func {
					funcs::E_CALL_BREAK => match (itype.dst.as_usize(), itype.src.as_usize()) {
						(0, 0) => match itype.imm {
							0b000000000000 => IntInstruction::ECall.into(),
							0b000000000001 => IntInstruction::EBreak.into(),
							imm => unimplemented!("SYSTEM func=0b000 rd=0b00000 rs1=0b00000 imm={imm:#014b}"),
						},
						(rd, rs1) => unimplemented!("SYSTEM func=0b000 rd={rd:#07b} rs1={rs1:#07b}"),
					},
					// NOTE: some of the Zicsr SYSTEM instructions are not yet implemented
					_ => unimplemented!("SYSTEM func={:#05b}", itype.func),
				}
			}
			OP_VE => unimplemented!("OP-VE"),
			CUSTOM_3 => unimplemented!("CUSTOM-3"),
			UNK_80B => unimplemented!("80b"),

			_ => unreachable!("??? opcode {opcode:#07b}"),
		}
	}

	/// tries to fetch an instruction, or returns Err if a trap happened during the fetch
	pub fn fetch_instruction(cpu: &mut WhiskerCpu) -> Result<(Instruction, u64), ()> {
		let pc = cpu.registers.pc;
		let support_compressed = cpu.supported_extensions.has(SupportedExtensions::COMPRESSED);
		let align_requirement = if support_compressed { 2 } else { 4 };
		if pc % align_requirement != 0 {
			cpu.request_trap(TrapIdx::INSTRUCTION_ADDR_MISALIGNED);
			return Err(());
		}

		let parcel1 = cpu.mem.read_u16(pc);
		if extract_bits_16(parcel1, 0, 1) != 0b11 {
			if support_compressed {
				todo!("implement 16bit instruction")
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION);
				Err(())
			}
		} else if extract_bits_16(parcel1, 2, 4) != 0b111 {
			let full_parcel = cpu.mem.read_u32(pc);
			Ok((Self::parse_32bit_instruction(full_parcel), 4))
		} else if extract_bits_16(parcel1, 0, 5) == 0b011111 {
			if support_compressed {
				todo!("implement 48bit instruction")
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION);
				Err(())
			}
		} else if extract_bits_16(parcel1, 0, 6) == 0b0111111 {
			if support_compressed {
				todo!("implement 64bit instruction")
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION);
				Err(())
			}
		} else {
			cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION);
			Err(())
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

	pub mod load {
		pub const LOAD_BYTE: u8 = 0b000;
		pub const LOAD_HALF: u8 = 0b001;
		pub const LOAD_WORD: u8 = 0b010;
		pub const LOAD_DOUBLE_WORD: u8 = 0b011;

		pub const LOAD_BYTE_ZERO_EXTEND: u8 = 0b100;
		pub const LOAD_HALF_ZERO_EXTEND: u8 = 0b101;
		pub const LOAD_WORD_ZERO_EXTEND: u8 = 0b110;
	}

	pub mod store {
		pub const STORE_BYTE: u8 = 0b000;
		pub const STORE_HALF: u8 = 0b001;
		pub const STORE_WORD: u8 = 0b010;
		pub const STORE_DOUBLE_WORD: u8 = 0b011;
	}

	pub mod op {
		pub const ADD: u16 = 0b0000000000;
		pub const SUB: u16 = 0b0100000000;
		pub const SHIFT_LEFT_LOGICAL: u16 = 0b0000000001;
		pub const SET_LESS_THAN: u16 = 0b0000000010;
		pub const SET_LESS_THAN_UNSIGNED: u16 = 0b000000011;
		pub const XOR: u16 = 0b0000000100;
		pub const SHIFT_RIGHT_LOGICAL: u16 = 0b0000000101;
		pub const SHIFT_RIGHT_ARITHMETIC: u16 = 0b0100000101;
		pub const OR: u16 = 0b0000000110;
		pub const AND: u16 = 0b0000000111;
	}

	pub mod op_imm {
		pub const ADD_IMM: u8 = 0b000;
		pub const XOR_IMM: u8 = 0b100;
		pub const OR_IMM: u8 = 0b110;
		pub const AND_IMM: u8 = 0b111;
		pub const SHIFT_LEFT_IMM: u8 = 0b001;
		pub const SHIFT_RIGHT_IMM: u8 = 0b101;
		pub const SET_LESS_THAN_IMM: u8 = 0b010;
		pub const SET_LESS_THAN_UNSIGNED_IMM: u8 = 0b011;

		pub const SHIFT_LOGICAL: u8 = 0b0000000;
		pub const SHIFT_ARITHMETIC: u8 = 0b0100000;
	}

	pub mod jalr {
		pub const JALR: u8 = 0b000;
	}

	pub mod branch {
		pub const BRANCH_EQ: u8 = 0b000;
		pub const BRANCH_NEQ: u8 = 0b001;
		pub const BRANCH_LESS_THAN: u8 = 0b100;
		pub const BRANCH_GREATER_EQ: u8 = 0b101;
		pub const BRANCH_LESS_THAN_UNSIGNED: u8 = 0b110;
		pub const BRANCH_GREATER_EQ_UNSIGNED: u8 = 0b111;
	}

	pub mod system {
		pub mod funcs {
			pub const E_CALL_BREAK: u8 = 0b000;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRW: u8 = 0b001;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRS: u8 = 0b010;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRC: u8 = 0b011;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRWI: u8 = 0b101;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRSI: u8 = 0b110;
			#[expect(dead_code, reason = "Zicsr not yet implemented")]
			pub const CSRRCI: u8 = 0b111;
		}
	}

	pub mod op_imm_32 {
		pub const ADD_IMM_WORD: u8 = 0b000;
		pub const SHIFT_LEFT_IMM_WORD: u8 = 0b001;
		pub const SHIFT_RIGHT_IMM_WORD: u8 = 0b101;

		pub const SHIFT_LOGICAL: u8 = 0b0000000;
		pub const SHIFT_ARITHMETIC: u8 = 0b0100000;
	}
}
