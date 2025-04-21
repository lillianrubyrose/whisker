use crate::insn::int::IntInstruction;
use crate::{cpu::WhiskerCpu, insn::Instruction, insn32::IType};

pub fn parse_misc_mem(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		FENCE => parse_fence(cpu, itype),
		_ => unreachable!("LOAD-FP func={:#05b}", itype.func()),
	}
}

fn parse_fence(_cpu: &mut WhiskerCpu, _itype: IType) -> Result<Instruction, ()> {
	// rs1 and rd are reserved
	// the bits of the immediate are partially bitflags and partially an enumeration
	// that control the behavior of the fence.

	Ok(IntInstruction::Fence {}.into())
}

pub mod consts {
	pub const FENCE: u8 = 0b000;
}
