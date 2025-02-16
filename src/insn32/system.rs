use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_system(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		funcs::E_CALL_BREAK => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				Ok(IntInstruction::parse_system(itype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		// NOTE: some of the Zicsr SYSTEM instructions are not yet implemented
		_ => unimplemented!("SYSTEM func={:#05b}", itype.func()),
	}
}

pub mod consts {
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
