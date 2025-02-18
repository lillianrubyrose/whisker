use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::RType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let rtype = RType::parse(parcel);

	// Can be RM or MIN/MAX. I can't find what these values are...
	// page 115 unpriv isa. (RM is 111, unsure what MIN/MAX are. idk if they're necessary at all :shrug:)
	let _rm = rtype.func() & 0b0000000111;
	let func7 = ((rtype.func() & 0b1111111000) >> 3) as u8;
	match func7 {
		ADD_SINGLE | SUB_SINGLE => {
			if cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
				let insn = FloatInstruction::parse_op_fp(rtype, func7);
				Ok(insn.into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("op-fp func7={func7:#09b}"),
	}
}

pub mod consts {
	pub const ADD_SINGLE: u8 = 0b0000000;
	pub const SUB_SINGLE: u8 = 0b0000100;
}
