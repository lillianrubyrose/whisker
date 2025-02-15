use crate::ty::RegisterIndex;
use crate::WhiskerCpu;

#[derive(Debug)]
pub enum IntInstruction {
	AddImmediate {
		dst: RegisterIndex,
		src: RegisterIndex,
		val: i64,
	},
	LoadUpperImmediate {
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
	JumpAndLink {
		link_reg: RegisterIndex,
		jmp_off: i64,
	},
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
				let itype = Self::parse_itype(parcel);
				match itype.func {
					0b000 => IntInstruction::AddImmediate {
						dst: itype.dst,
						src: itype.src,
						val: itype.imm,
					}
					.into(),
					_ => unreachable!("should've matched op-imm func"),
				}
			}
			AUIPC => unimplemented!("AUIPC"),
			OP_IMM_32 => unimplemented!("OP-IMM-32"),
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
			BRANCH => unimplemented!("BRANCH"),
			JALR => unimplemented!("JALR"),
			RESERVED => unimplemented!("RESERVED"),
			JAL => {
				let jtype = Self::parse_jtype(parcel);
				IntInstruction::JumpAndLink {
					link_reg: jtype.dst,
					jmp_off: jtype.imm,
				}
				.into()
			}
			SYSTEM => unimplemented!("SYSTEM"),
			OP_VE => unimplemented!("OP-VE"),
			CUSTOM_3 => unimplemented!("CUSTOM-3"),
			UNK_80B => unimplemented!("80b"),

			_ => unreachable!("??? opcode {opcode:#07b}"),
		}
	}

	pub fn fetch_instruction(cpu: &mut WhiskerCpu) -> Instruction {
		assert!(cpu.registers.pc % 2 == 0);

		let parcel1 = cpu.mem.read_u16(cpu.registers.pc);
		if extract_bits_16(parcel1, 0, 1) != 0b11 {
			todo!("16bit instruction");
		} else if extract_bits_16(parcel1, 2, 4) != 0b111 {
			let full_parcel = cpu.mem.read_u32(cpu.registers.pc);
			cpu.registers.pc += 4;
			Self::parse_32bit_instruction(full_parcel)
		} else if extract_bits_16(parcel1, 0, 5) == 0b011111 {
			todo!("48bit instruction");
		} else if extract_bits_16(parcel1, 0, 6) == 0b0111111 {
			todo!("64bit instruction");
		} else {
			unimplemented!("{parcel1:#018b}")
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
}
