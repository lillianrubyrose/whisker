use crate::insn::float::FloatInstruction;

use super::IType;

impl FloatInstruction {
	pub fn parse_load_fp(itype: IType) -> FloatInstruction {
		use crate::insn32::load_fp::consts::*;
		match itype.func() {
			FLOAT_LOAD_WORD => FloatInstruction::FloatLoadWord {
				dst: itype.dst(),
				src: itype.src(),
				src_offset: itype.imm(),
			},
			_ => unimplemented!("load-fp: {:#05b}", itype.func()),
		}
	}
}
