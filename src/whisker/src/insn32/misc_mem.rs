use crate::{cpu::hart::WhiskerHart, insn::*, insn32::IType};

pub fn parse_misc_mem(hart: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use consts::*;

	let itype = IType::parse(parcel);
	match itype.func() {
		FENCE => parse_fence(hart, itype),
		FENCE_I => Some(IntInstruction::InstructionFence.into()),
		// FIXME: are there any other MISC-MEM instructions?
		_ => unreachable!("MISC-MEM func={:#05b}", itype.func()),
	}
}

#[allow(
	clippy::unnecessary_wraps,
	reason = "the weird signature here is because right now we dont actually parse the data out of the instruction, and fences are no-ops."
)]
fn parse_fence(_: &mut WhiskerHart, _itype: IType) -> Option<Instruction> {
	// FIXME: the weird signature here is because right now we dont actually parse the data out
	// of the instruction, and fences are no-ops.

	// rs1 and rd are reserved
	// the bits of the immediate are partially bitflags and partially an enumeration
	// that control the behavior of the fence.

	Some(IntInstruction::Fence {}.into())
}

pub mod consts {
	pub const FENCE: u8 = 0b000;
	pub const FENCE_I: u8 = 0b001;
}
