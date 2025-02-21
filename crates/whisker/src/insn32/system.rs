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
		funcs::E_CALL_BREAK => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(parse_call_break(itype).into())
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

fn parse_call_break(itype: IType) -> IntInstruction {
	match (itype.dst().to_gp().as_usize(), itype.src().to_gp().as_usize()) {
		(0, 0) => match itype.imm() {
			0b000000000000 => IntInstruction::ECall,
			0b000000000001 => IntInstruction::EBreak,
			imm => unimplemented!("SYSTEM func=0b000 rd=0b00000 rs1=0b00000 imm={imm:#014b}"),
		},
		(rd, rs1) => unimplemented!("SYSTEM func=0b000 rd={rd:#07b} rs1={rs1:#07b}"),
	}
}

// csr numbers are NOT sign extended
fn imm_to_csr(imm: i64) -> u16 {
	(imm & 0xFFF) as u16
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
			src: itype.src().as_usize() as u64,
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
		pub const E_CALL_BREAK: u8 = 0b000;
		pub const CSRRW: u8 = 0b001;
		pub const CSRRS: u8 = 0b010;
		pub const CSRRC: u8 = 0b011;
		pub const CSRRWI: u8 = 0b101;
		pub const CSRRSI: u8 = 0b110;
		pub const CSRRCI: u8 = 0b111;
	}
}
