use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::IType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_load_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		FLOAT_LOAD_WORD => {
			if cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
				Ok(FloatInstruction::parse_load_fp(itype).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unreachable!("LOAD-FP func={:#05b}", itype.func()),
	}
}

pub mod consts {
	pub const FLOAT_LOAD_WORD: u8 = 0b010;
}
