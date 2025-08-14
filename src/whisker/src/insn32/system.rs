use tracing::warn;

use crate::cpu::csr::CSRIndex;
use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::IType};

pub fn parse_system(_: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	// FIXME: check csr support somehow?
	match itype.func() {
		funcs::FUNC_0 => parse_func_0(itype),
		funcs::CSRRW => Some(
			CSRInstruction::CSRReadWrite {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		funcs::CSRRS => Some(
			CSRInstruction::CSRReadAndSet {
				dst: itype.dst().to_gp(),
				mask: itype.src().to_gp(),
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		funcs::CSRRC => Some(
			CSRInstruction::CSRReadAndClear {
				dst: itype.dst().to_gp(),
				mask: itype.src().to_gp(),
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		funcs::CSRRWI => Some(
			CSRInstruction::CSRReadWriteImm {
				dst: itype.dst().to_gp(),
				imm: itype.src().as_usize() as u64,
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		funcs::CSRRSI => Some(
			CSRInstruction::CSRReadAndSetImm {
				dst: itype.dst().to_gp(),
				mask: itype.src().as_usize() as u64,
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		funcs::CSRRCI => Some(
			CSRInstruction::CSRReadAndClearImm {
				dst: itype.dst().to_gp(),
				mask: itype.src().as_usize() as u64,
				csr: imm_to_csr(itype.imm()),
			}
			.into(),
		),
		// NOTE: some of the Zicsr SYSTEM instructions are not yet implemented
		_ => None,
	}
}

fn parse_func_0(itype: IType) -> Option<Instruction> {
	use consts::*;
	match (itype.dst().to_gp().as_usize(), itype.src().to_gp().as_usize()) {
		(0, 0) => match itype.imm() {
			func0::ECALL => Some(IntInstruction::ECall.into()),
			func0::EBREAK => Some(IntInstruction::EBreak.into()),
			func0::MRET => Some(PrivilegedInstruction::Mret.into()),
			func0::SRET => Some(PrivilegedInstruction::Sret.into()),
			func0::WFI => Some(PrivilegedInstruction::WaitForInterrupt.into()),
			imm => {
				warn!("UNIMPLEMENTED: SYSTEM func=0b000 rd=0b00000 rs1=0b00000 imm={imm:#014b}");
				None
			}
		},
		(rd, rs1) => {
			warn!(
				"UNIMPLEMENTED: SYSTEM func=0b000 rd={rd:#07b} rs1={rs1:#07b} imm={imm:#014b}",
				imm = itype.imm()
			);
			None
		}
	}
}

// csr numbers are NOT sign extended
fn imm_to_csr(imm: i64) -> CSRIndex {
	// UNWRAP: this mask makes sure that the value is in range
	CSRIndex::new((imm & 0xFFF) as u16).unwrap()
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
		pub const MRET: i64 = 0b0011_0000_0010;
		pub const SRET: i64 = 0b0001_0000_0010;
		pub const WFI: i64 = 0b0001_0000_0101;
	}
}
