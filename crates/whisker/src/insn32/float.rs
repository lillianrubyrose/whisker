use crate::insn::float::FloatInstruction;

use super::{IType, R4Type, RType, SType};

impl FloatInstruction {
	pub fn parse_load_fp(itype: IType) -> FloatInstruction {
		use crate::insn32::load_fp::consts::*;
		match itype.func() {
			FLOAT_LOAD_WORD => FloatInstruction::LoadWord {
				dst: itype.dst().to_fp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			_ => unimplemented!("load-fp: {:#05b}", itype.func()),
		}
	}

	pub fn parse_store_fp(stype: SType) -> FloatInstruction {
		use crate::insn32::store_fp::consts::*;
		match stype.func() {
			FLOAT_STORE_WORD => FloatInstruction::StoreWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_fp(),
			},
			_ => unimplemented!("store-fp: {:#05b}", stype.func()),
		}
	}

	pub fn parse_op_fp(rtype: RType, rm: u8, func7: u8) -> FloatInstruction {
		use crate::insn32::op_fp::consts::*;
		match func7 {
			ADD_SINGLE => FloatInstruction::AddSinglePrecision {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
			}
			.into(),
			SUB_SINGLE => FloatInstruction::SubSinglePrecision {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
			}
			.into(),
			EQ_SINGLE => match rm {
				_ => unimplemented!("OP-FP rm={rm:#05b}"),
			},
			_ => unimplemented!("OP-FP func7={func7:#09b}"),
		}
	}

	pub fn parse_madd(r4type: R4Type, func2: u8) -> FloatInstruction {
		use crate::insn32::madd::consts::*;
		match func2 {
			FLOAT_MUL_ADD_SINGLE => FloatInstruction::MulAddSinglePrecision {
				dst: r4type.dst().to_fp(),
				mul_lhs: r4type.src1().to_fp(),
				mul_rhs: r4type.src2().to_fp(),
				add: r4type.src3().to_fp(),
			},
			_ => unimplemented!("madd func2:{func2:#04b}"),
		}
	}
}
