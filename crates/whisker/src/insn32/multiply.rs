use crate::{cpu::WhiskerCpu, insn::multiply::MultiplyInstruction};

use super::RType;

impl MultiplyInstruction {
	pub fn parse_op(_cpu: &mut WhiskerCpu, rtype: RType) -> Result<Self, ()> {
		use crate::insn32::op::consts::*;
		Ok(match rtype.func() {
			MUL => Self::Multiply {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			MULH => Self::MultiplyHigh {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			MULHSU => Self::MultiplyHighSignedUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			MULHU => MultiplyInstruction::MultiplyHighUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			DIV => Self::Divide {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			DIVU => Self::DivideUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			REM => Self::Remainder {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			REMU => Self::RemainderUnsigned {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},

			_ => unreachable!(),
		})
	}

	pub fn parse_op_32(_cpu: &mut WhiskerCpu, rtype: RType) -> Result<Self, ()> {
		use crate::insn32::op::consts::*;

		Ok(match rtype.func() {
			MUL_WORD => Self::MultiplyWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			DIV_WORD => Self::DivideWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			DIV_UNSIGNED_WORD => Self::DivideUnsignedWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			REM_WORD => Self::RemainderWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			REM_UNSIGNED_WORD => Self::RemainderUnsignedWord {
				lhs: rtype.src1().to_gp(),
				rhs: rtype.src2().to_gp(),
				dst: rtype.dst().to_gp(),
			},
			_ => unreachable!(),
		})
	}
}
