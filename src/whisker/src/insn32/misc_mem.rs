use crate::insn::int::IntInstruction;
use crate::{cpu::WhiskerCpu, insn::Instruction, insn32::IType};

pub fn parse_misc_mem(cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		FENCE => parse_fence(cpu, itype),
		// FIXME: are there any other MISC-MEM instructions?
		_ => unreachable!("MISC-MEM func={:#05b}", itype.func()),
	}
}

fn parse_fence(_cpu: &mut WhiskerCpu, _itype: IType) -> Option<Instruction> {
	// FIXME: the weird signature here is because right now we dont actually parse the data out
	// of the instruction, and fences are no-ops.

	// rs1 and rd are reserved
	// the bits of the immediate are partially bitflags and partially an enumeration
	// that control the behavior of the fence.

	Some(IntInstruction::Fence {}.into())
}

pub mod consts {
	pub const FENCE: u8 = 0b000;
}
