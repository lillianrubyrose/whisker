use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::SType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_store_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let stype = SType::parse(parcel);
	match stype.func() {
		FLOAT_STORE_WORD => {
			if cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
				Ok(FloatInstruction::parse_store_fp(stype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unreachable!("LOAD-FP func={:#05b}", stype.func()),
	}
}

pub mod consts {
	pub const FLOAT_STORE_WORD: u8 = 0b010;
}
