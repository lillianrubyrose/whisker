use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::R4Type,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_madd(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let r4type = R4Type::parse(parcel);

	// Can be RM or MIN/MAX. I can't find what these values are...
	// page 115 unpriv isa. (RM is 111, unsure what MIN/MAX are. idk if they're necessary at all :shrug:)
	// RM = remainder mode
	let _rm = r4type.func() & 0b00111;
	let func2 = ((r4type.func() & 0b11000) >> 3) as u8;

	match func2 {
		FLOAT_MUL_ADD_SINGLE => {
			if cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
				Ok(FloatInstruction::parse_madd(r4type, func2).into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("op-madd func2:{func2:#04b}"),
	}

	// match r4type.func() {
	// 	FLOAT_LOAD_WORD => {
	// 		if cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
	// 			Ok(FloatInstruction::parse_load_fp(itype).into())
	// 		} else {
	// 			cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
	// 			Err(())
	// 		}
	// 	}
	// 	_ => unreachable!("LOAD-FP func={:#05b}", itype.func()),
	// }
}

pub mod consts {
	pub const FLOAT_MUL_ADD_SINGLE: u8 = 0b00;
}
