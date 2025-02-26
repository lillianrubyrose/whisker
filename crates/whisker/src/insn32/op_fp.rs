use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::RType,
	soft::RoundingMode,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_op_fp(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	// OP-FP type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
		return Err(());
	}

	let rtype = RType::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(rtype.func3()) else {
		cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
		return Err(());
	};
	let func7 = rtype.func7();
	match func7 {
		ADD_SINGLE | SUB_SINGLE | MUL_SINGLE | DIV_SINGLE | SQRT_SINGLE | MIN_MAX | CMP_SINGLE => {
			FloatInstruction::parse_op_fp(cpu, rtype, rm).map(|i| i.into())
		}
		_ => unimplemented!("op-fp func7={func7:#09b}"),
	}
}

pub mod consts {

	pub const ADD_SINGLE: u8 = 0b0000000;
	pub const SUB_SINGLE: u8 = 0b0000100;
	pub const MUL_SINGLE: u8 = 0b0001000;
	pub const DIV_SINGLE: u8 = 0b0001100;
	pub const SQRT_SINGLE: u8 = 0b0101100;
	pub const MIN_MAX: u8 = 0b0010100;
	pub const CMP_SINGLE: u8 = 0b1010000;

	pub mod min_max {
		pub const MIN: u8 = 0b000;
		pub const MAX: u8 = 0b001;
	}

	pub mod cmp {
		pub const LESS_EQ: u8 = 0b000;
		pub const LESS_THAN: u8 = 0b001;
		pub const EQ: u8 = 0b010;
	}
}
