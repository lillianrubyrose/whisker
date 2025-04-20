use crate::cpu::csr::CSRIndex;
use crate::insn::privileged::PrivilegedInstruction;
use crate::{
	cpu::WhiskerCpu,
	insn::{csr::CSRInstruction, int::IntInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_system(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		funcs::FUNC_0 => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				match parse_func_0(itype) {
					Some(inst) => Ok(inst),
					None => {
						cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
						Err(())
					}
				}
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		funcs::CSRRW | funcs::CSRRS | funcs::CSRRC | funcs::CSRRWI | funcs::CSRRSI | funcs::CSRRCI => {
			// FIXME: check csr support somehow
			Ok(parse_csr(itype).into())
		}
		// NOTE: some of the Zicsr SYSTEM instructions are not yet implemented
		_ => unimplemented!("SYSTEM func={:#05b}", itype.func()),
	}
}

fn parse_func_0(itype: IType) -> Option<Instruction> {
	use consts::*;
	match (itype.dst().to_gp().as_usize(), itype.src().to_gp().as_usize()) {
		(0, 0) => match itype.imm() {
			func0::ECALL => Some(IntInstruction::ECall.into()),
			func0::EBREAK => Some(IntInstruction::EBreak.into()),
			func0::MRET => Some(PrivilegedInstruction::Mret.into()),
			imm => unimplemented!("SYSTEM func=0b000 rd=0b00000 rs1=0b00000 imm={imm:#014b}"),
		},
		(rd, rs1) => unimplemented!(
			"SYSTEM func=0b000 rd={rd:#07b} rs1={rs1:#07b} imm={imm:#014b}",
			imm = itype.imm()
		),
	}
}

// csr numbers are NOT sign extended
fn imm_to_csr(imm: i64) -> CSRIndex {
	// UNWRAP: this mask makes sure that the value is in range
	CSRIndex::new((imm & 0xFFF) as u16).unwrap()
}

fn parse_csr(itype: IType) -> CSRInstruction {
	use consts::*;
	match itype.func() {
		funcs::CSRRW => CSRInstruction::CSRReadWrite {
			dst: itype.dst().to_gp(),
			src: itype.src().to_gp(),
			csr: imm_to_csr(itype.imm()),
		},
		funcs::CSRRS => CSRInstruction::CSRReadAndSet {
			dst: itype.dst().to_gp(),
			mask: itype.src().to_gp(),
			csr: imm_to_csr(itype.imm()),
		},
		funcs::CSRRC => CSRInstruction::CSRReadAndClear {
			dst: itype.dst().to_gp(),
			mask: itype.src().to_gp(),
			csr: imm_to_csr(itype.imm()),
		},
		funcs::CSRRWI => CSRInstruction::CSRReadWriteImm {
			dst: itype.dst().to_gp(),
			imm: itype.src().as_usize() as u64,
			csr: imm_to_csr(itype.imm()),
		},
		funcs::CSRRSI => CSRInstruction::CSRReadAndSetImm {
			dst: itype.dst().to_gp(),
			mask: itype.src().as_usize() as u64,
			csr: imm_to_csr(itype.imm()),
		},
		funcs::CSRRCI => CSRInstruction::CSRReadAndClearImm {
			dst: itype.dst().to_gp(),
			mask: itype.src().as_usize() as u64,
			csr: imm_to_csr(itype.imm()),
		},
		_ => unreachable!(),
	}
}

pub mod consts {
	pub mod funcs {
		pub const FUNC_0: u8 = 0b000;
		pub const CSRRW: u8 = 0b001;
		pub const CSRRS: u8 = 0b010;
		pub const CSRRC: u8 = 0b011;
		pub const CSRRWI: u8 = 0b101;
		pub const CSRRSI: u8 = 0b110;
		pub const CSRRCI: u8 = 0b111;
	}

	pub mod func0 {
		pub const ECALL: i64 = 0;
		pub const EBREAK: i64 = 1;
		pub const MRET: i64 = 0b001100000010;
	}
}
