use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, multiply::MultiplyInstruction, Instruction},
	insn32::RType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op_32(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let rtype = RType::parse(parcel);
	match rtype.func() {
		MUL_WORD | DIV_WORD | DIV_UNSIGNED_WORD | REM_WORD | REM_UNSIGNED_WORD => {
			if cpu.supported_extensions.has(SupportedExtensions::MULTIPLY) {
				let insn = MultiplyInstruction::parse_op_32(cpu, rtype)?;
				Ok(insn.into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!("OP-32 func={:#014b} | {:#X}", rtype.func(), cpu.pc),
	}
}

pub mod consts {
	pub const MUL_WORD: u16 = 0b0000001000;
	pub const DIV_WORD: u16 = 0b0000001100;
	pub const DIV_UNSIGNED_WORD: u16 = 0b0000001101;
	pub const REM_WORD: u16 = 0b0000001110;
	pub const REM_UNSIGNED_WORD: u16 = 0b0000001111;
}
