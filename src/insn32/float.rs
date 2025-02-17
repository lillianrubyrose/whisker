use crate::insn::float::FloatInstruction;

use super::{IType, SType};

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

	pub fn parse_store_fp(stype: SType) -> FloatInstruction {
		use crate::insn32::store_fp::consts::*;
		match stype.func() {
			FLOAT_STORE_WORD => FloatInstruction::FloatStoreWord {
				dst: stype.src1(),
				dst_offset: stype.imm(),
				src: stype.src2(),
			},
			_ => unimplemented!("store-fp: {:#05b}", stype.func()),
		}
	}
}
