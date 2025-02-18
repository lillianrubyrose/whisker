use crate::{
	insn::int::IntInstruction,
	insn32::{BType, IType, RType, SType},
	util::extract_bits_32,
};

macro_rules! define_op_int {
    ($rtype:ident, $($const:ident, $inst:ident),*) => {
        {
            use crate::insn32::op::consts::*;
            match $rtype.func() {
                $( $const => IntInstruction::$inst { dst: $rtype.dst().to_gp(), lhs: $rtype.src1().to_gp(), rhs: $rtype.src2().to_gp() }.into(), )*
                _ => unimplemented!("OP func={:#012b}", $rtype.func()),
            }
        }
    };
}

macro_rules! define_branch_int {
    ($btype:ident, $($const:ident, $inst:ident),*) => {
        {
            use crate::insn32::branch::consts::*;
            match $btype.func() {
                $( $const => IntInstruction::$inst { lhs: $btype.src1().to_gp(), rhs: $btype.src2().to_gp(), imm: $btype.imm() }.into(), )*
                _ => unimplemented!("BRANCH func={:#012b}", $btype.func()),
            }
        }
    };
}

impl IntInstruction {
	pub fn parse_load(itype: IType) -> IntInstruction {
		use crate::insn32::load::consts::*;
		match itype.func() {
			LOAD_BYTE => IntInstruction::LoadByte {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_HALF => IntInstruction::LoadHalf {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_WORD => IntInstruction::LoadWord {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_DOUBLE_WORD => IntInstruction::LoadDoubleWord {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_BYTE_ZERO_EXTEND => IntInstruction::LoadByteZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_HALF_ZERO_EXTEND => IntInstruction::LoadHalfZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			LOAD_WORD_ZERO_EXTEND => IntInstruction::LoadWordZeroExtend {
				dst: itype.dst().to_gp(),
				src: itype.src().to_gp(),
				src_offset: itype.imm(),
			},
			_ => unreachable!(),
		}
	}

	pub fn parse_op_imm(itype: IType) -> IntInstruction {
		use crate::insn32::op_imm::consts::*;
		match itype.func() {
			ADD_IMM => IntInstruction::AddImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			XOR_IMM => IntInstruction::XorImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			OR_IMM => IntInstruction::OrImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			AND_IMM => IntInstruction::AndImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			SHIFT_LEFT_IMM | SHIFT_RIGHT_IMM => {
				let shift_amt = extract_bits_32(itype.imm() as u32, 5, 11) as u32;
				let shift_kind = extract_bits_32(itype.imm() as u32, 0, 4) as u8;
				match shift_kind {
					SHIFT_LOGICAL => match itype.func() {
						SHIFT_LEFT_IMM => IntInstruction::ShiftLeftLogicalImmediate {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						SHIFT_RIGHT_IMM => IntInstruction::ShiftRightLogicalImmediate {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						_ => unreachable!(),
					},
					SHIFT_ARITHMETIC => match itype.func() {
						SHIFT_RIGHT_IMM => IntInstruction::ShiftRightArithmeticImmediate {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						_ => unreachable!(),
					},
					_ => unreachable!("op-imm shift kind: {:#09b}", shift_kind),
				}
			}
			SET_LESS_THAN_IMM => IntInstruction::SetLessThanImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			SET_LESS_THAN_UNSIGNED_IMM => IntInstruction::SetLessThanUnsignedImmediate {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm(),
			},
			_ => unreachable!(),
		}
	}

	pub fn parse_op_imm_32(itype: IType) -> IntInstruction {
		use crate::insn32::op_imm_32::consts::*;
		match itype.func() {
			ADD_IMM_WORD => IntInstruction::AddImmediateWord {
				dst: itype.dst().to_gp(),
				lhs: itype.src().to_gp(),
				rhs: itype.imm() as i32,
			},
			SHIFT_LEFT_IMM_WORD | SHIFT_RIGHT_IMM_WORD => {
				let shift_amt = extract_bits_32(itype.imm() as u32, 5, 11) as u32;
				let shift_kind = extract_bits_32(itype.imm() as u32, 0, 4) as u8;
				match shift_kind {
					SHIFT_LOGICAL => match itype.func() {
						SHIFT_LEFT_IMM_WORD => IntInstruction::ShiftLeftLogicalImmediateWord {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						SHIFT_RIGHT_IMM_WORD => IntInstruction::ShiftRightLogicalImmediateWord {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						_ => unreachable!(),
					},
					SHIFT_ARITHMETIC => match itype.func() {
						SHIFT_RIGHT_IMM_WORD => IntInstruction::ShiftRightArithmeticImmediateWord {
							dst: itype.dst().to_gp(),
							lhs: itype.src().to_gp(),
							shift_amt,
						},
						_ => unreachable!(),
					},
					_ => unreachable!("op-imm-32 shift kind: {:#09b}", shift_kind),
				}
			}
			_ => unimplemented!("op-imm-32 func: {:#05b}", itype.func()),
		}
	}

	pub fn parse_store(stype: SType) -> IntInstruction {
		use crate::insn32::store::consts::*;
		match stype.func() {
			STORE_BYTE => IntInstruction::StoreByte {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			},
			STORE_HALF => IntInstruction::StoreHalf {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			},
			STORE_WORD => IntInstruction::StoreWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			},
			STORE_DOUBLE_WORD => IntInstruction::StoreDoubleWord {
				dst: stype.src1().to_gp(),
				dst_offset: stype.imm(),
				src: stype.src2().to_gp(),
			},
			_ => unreachable!(),
		}
	}

	#[rustfmt::skip]
	pub fn parse_op(rtype: RType) -> IntInstruction {
    	define_op_int!(
    		rtype,
    		ADD, Add,
    		SUB, Sub,
    		SHIFT_LEFT_LOGICAL, ShiftLeftLogical,
    		SHIFT_RIGHT_LOGICAL, ShiftRightLogical,
    		SHIFT_RIGHT_ARITHMETIC, ShiftRightArithmetic,
    		AND, And,
    		OR, Or,
    		XOR, Xor,
    		SET_LESS_THAN, SetLessThan,
    		SET_LESS_THAN_UNSIGNED, SetLessThanUnsigned
    	)
	}

	#[rustfmt::skip]
	pub fn parse_branch(btype: BType) -> IntInstruction {
    	define_branch_int!(
    		btype,
    		BRANCH_EQ, BranchEqual,
    		BRANCH_NEQ, BranchNotEqual,
    		BRANCH_LESS_THAN, BranchLessThan,
    		BRANCH_GREATER_EQ, BranchGreaterEqual,
    		BRANCH_LESS_THAN_UNSIGNED, BranchLessThanUnsigned,
    		BRANCH_GREATER_EQ_UNSIGNED, BranchGreaterEqualUnsigned
    	)
	}

	pub fn parse_system(itype: IType) -> IntInstruction {
		use crate::insn32::system::consts::*;
		match itype.func() {
			funcs::E_CALL_BREAK => match (itype.dst().to_gp().as_usize(), itype.src().to_gp().as_usize()) {
				(0, 0) => match itype.imm() {
					0b000000000000 => IntInstruction::ECall.into(),
					0b000000000001 => IntInstruction::EBreak.into(),
					imm => unimplemented!("SYSTEM func=0b000 rd=0b00000 rs1=0b00000 imm={imm:#014b}"),
				},
				(rd, rs1) => unimplemented!("SYSTEM func=0b000 rd={rd:#07b} rs1={rs1:#07b}"),
			},
			_ => unimplemented!("SYSTEM func={:#05b}", itype.func()),
		}
	}
}
