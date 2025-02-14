use crate::ty::RegisterIndex;
use crate::WhiskerCpu;

#[derive(Debug)]
pub enum IntInstruction {
	AddImmediate {
		dst: RegisterIndex,
		src: RegisterIndex,
		val: u16,
	},
	// val is already shifted << 12
	LoadUpperImmediate {
		dst: RegisterIndex,
		val: u32,
	},
	StoreByte {
		dst: RegisterIndex,
		dst_offset: u16,
		src: RegisterIndex,
	},
	JumpAndLink {
		link_reg: RegisterIndex,
		jmp_addr: u32,
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
	imm: u16,
}

#[derive(Debug)]
struct UType {
	dst: RegisterIndex,
	imm: u32,
}

#[derive(Debug)]
struct SType {
	imm: u16,
	func: u8,
	src1: RegisterIndex,
	src2: RegisterIndex,
}

#[derive(Debug)]
struct JType {
	dst: RegisterIndex,
	imm: u32,
}

/// extracts bits start..=end from val
fn extract_bits_32(val: u32, start: u8, end: u8) -> u32 {
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

impl Instruction {
	fn parse_itype(parcel: u32) -> IType {
		let dst = extract_dst(parcel);
		let func = extract_bits_32(parcel, 12, 14) as u8;
		let src = extract_src1(parcel);
		let imm = extract_bits_32(parcel, 20, 31) as u16;
		IType { dst, func, src, imm }
	}

	fn parse_utype(parcel: u32) -> UType {
		let dst = extract_dst(parcel);
		let imm = extract_bits_32(parcel, 12, 31);
		UType { dst, imm }
	}

	fn parse_stype(parcel: u32) -> SType {
		let imm0 = extract_bits_32(parcel, 7, 11);
		let func = extract_bits_32(parcel, 12, 14) as u8;
		let src1 = extract_src1(parcel);
		let src2 = extract_src2(parcel);
		let imm1 = extract_bits_32(parcel, 25, 31);

		SType {
			imm: (imm1 << 5 | imm0) as u16,
			func,
			src1,
			src2,
		}
	}

	fn parse_jtype(parcel: u32) -> JType {
		let dst = extract_dst(parcel);
		let imm12_19 = extract_bits_32(parcel, 12, 19);
		let imm11 = extract_bits_32(parcel, 20, 20);
		let imm1_10 = extract_bits_32(parcel, 21, 30);
		let imm20 = extract_bits_32(parcel, 31, 31);

		JType {
			dst,
			imm: imm1_10 << 1 | imm11 << 11 | imm12_19 << 12 | imm20 << 20,
		}
	}

	fn parse_32bit_instruction(parcel: u32) -> Instruction {
		let opcode = (parcel & 0b1111100) >> 2;
		match opcode {
			// store
			0b01000 => {
				let stype = Self::parse_stype(parcel);
				match stype.func {
					// sb
					0b000 => IntInstruction::StoreByte {
						dst: stype.src1,
						dst_offset: stype.imm,
						src: stype.src2,
					}
					.into(),
					_ => unreachable!("couldnt match store stype func"),
				}
			}

			// lui
			0b01101 => {
				let utype = Self::parse_utype(parcel);
				IntInstruction::LoadUpperImmediate {
					dst: utype.dst,
					val: utype.imm << 12,
				}
				.into()
			}

			// op-imm
			0b00100 => {
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

			// jal
			0b11011 => {
				let jtype = Self::parse_jtype(parcel);
				IntInstruction::JumpAndLink {
					link_reg: jtype.dst,
					jmp_addr: jtype.imm,
				}
				.into()
			}

			_ => unreachable!("should've matched 32bit instruction opcode {parcel:#018b}"),
		}
	}

	pub fn fetch_instruction(cpu: &mut WhiskerCpu) -> Instruction {
		assert!(cpu.registers.pc % 2 == 0);

		let parcel1 = cpu.physmem.read_u16(cpu.registers.pc);
		if parcel1 & 0b11 != 0b11 {
			todo!("16bit instruction");
		} else if (parcel1 & 0b11100) >> 2 != 0b111 {
			let full_parcel = cpu.physmem.read_u32(cpu.registers.pc);
			cpu.registers.pc += 4;
			Self::parse_32bit_instruction(full_parcel)
		} else if (parcel1 & 0b111111) == 0b011111 {
			todo!("48bit instruction");
		} else if (parcel1 & 0b1111111) == 0b0111111 {
			todo!("64bit instruction");
		} else {
			unimplemented!("{parcel1:#018b}")
		}
	}
}
