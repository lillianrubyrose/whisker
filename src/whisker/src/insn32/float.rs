use crate::{
	cpu::WhiskerCpu,
	insn::float::FloatInstruction,
	soft::RoundingMode,
	ty::{RegisterIndex, TrapIdx},
};

use super::{IType, RType, SType};

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

	pub fn parse_op_fp(cpu: &mut WhiskerCpu, rtype: RType, rm: RoundingMode) -> Result<FloatInstruction, ()> {
		use crate::insn32::op_fp::consts::*;
		match rtype.func7() {
			ADD_SINGLE => Ok(FloatInstruction::Add {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}),
			SUB_SINGLE => Ok(FloatInstruction::Sub {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}),
			MUL_SINGLE => Ok(FloatInstruction::Mul {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}),
			DIV_SINGLE => Ok(FloatInstruction::Div {
				dst: rtype.dst().into(),
				lhs: rtype.src1().into(),
				rhs: rtype.src2().into(),
				rm,
			}),
			SQRT_SINGLE => {
				if rtype.src2() != RegisterIndex::ZERO {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				} else {
					Ok(FloatInstruction::Sqrt {
						dst: rtype.dst().to_fp(),
						val: rtype.src1().to_fp(),
						rm,
					})
				}
			}
			MIN_MAX => match rtype.func3() {
				min_max::MIN => Ok(FloatInstruction::Min {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}),
				min_max::MAX => Ok(FloatInstruction::Max {
					dst: rtype.dst().into(),
					lhs: rtype.src1().into(),
					rhs: rtype.src2().into(),
				}),
				_ => {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				}
			},
			CMP_SINGLE => match rtype.func3() {
				cmp::EQ => Ok(FloatInstruction::Equal {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}),
				cmp::LESS_EQ => Ok(FloatInstruction::LessOrEqual {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}),
				cmp::LESS_THAN => Ok(FloatInstruction::LessThan {
					dst: rtype.dst().to_gp(),
					lhs: rtype.src1().to_fp(),
					rhs: rtype.src2().to_fp(),
				}),
				_ => {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					Err(())
				}
			},

			_ => unimplemented!("OP-FP func7={:#09b}", rtype.func7()),
		}
	}
}
