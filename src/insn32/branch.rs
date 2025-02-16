use crate::{
	cpu::WhiskerCpu,
	insn::{int::IntInstruction, Instruction},
	insn32::BType,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_branch(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let btype = BType::parse(parcel);
	match btype.func() {
		BRANCH_EQ
		| BRANCH_NEQ
		| BRANCH_LESS_THAN
		| BRANCH_GREATER_EQ
		| BRANCH_LESS_THAN_UNSIGNED
		| BRANCH_GREATER_EQ_UNSIGNED => {
			if cpu.supported_extensions.has(SupportedExtensions::INTEGER) {
				let insn = IntInstruction::parse_branch(btype);
				Ok(insn.into())
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				Err(())
			}
		}
		_ => unimplemented!(),
	}
}

pub mod consts {
	pub const BRANCH_EQ: u8 = 0b000;
	pub const BRANCH_NEQ: u8 = 0b001;
	pub const BRANCH_LESS_THAN: u8 = 0b100;
	pub const BRANCH_GREATER_EQ: u8 = 0b101;
	pub const BRANCH_LESS_THAN_UNSIGNED: u8 = 0b110;
	pub const BRANCH_GREATER_EQ_UNSIGNED: u8 = 0b111;
}
