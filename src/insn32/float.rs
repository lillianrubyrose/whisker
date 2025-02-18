use crate::insn::float::FloatInstruction;

use super::{IType, RType, SType};

macro_rules! define_op_float {
    ($rtype:ident, $func7:ident, $($const:ident, $inst:ident),*) => {
        {
            use crate::insn32::op_fp::consts::*;
            match $func7 {
                $( $const => FloatInstruction::$inst { dst: $rtype.dst(), lhs: $rtype.src1(), rhs: $rtype.src2() }.into(), )*
                _ => unimplemented!("OP-FP func7={:#09b}", $func7),
            }
        }
    };
}

impl FloatInstruction {
	pub fn parse_load_fp(itype: IType) -> FloatInstruction {
		use crate::insn32::load_fp::consts::*;
		match itype.func() {
			FLOAT_LOAD_WORD => FloatInstruction::LoadWord {
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
			FLOAT_STORE_WORD => FloatInstruction::StoreWord {
				dst: stype.src1(),
				dst_offset: stype.imm(),
				src: stype.src2(),
			},
			_ => unimplemented!("store-fp: {:#05b}", stype.func()),
		}
	}

	#[rustfmt::skip]
	pub fn parse_op_fp(rtype: RType, func7: u8) -> FloatInstruction {
		define_op_float!(
    		rtype, func7,
    		ADD_SINGLE, AddSinglePrecision,
    		SUB_SINGLE, SubSinglePrecision
    	)
	}
}
