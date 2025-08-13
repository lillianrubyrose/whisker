use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::R4Type, soft::RoundingMode, ty::RiscvExtensions};

pub fn parse_nmadd(hart: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use crate::insn32::consts::*;

	// NMADD type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !hart.supports_extensions(RiscvExtensions::FLOAT) {
		return None;
	}

	let r4type = R4Type::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(r4type.func3()) else {
		return None;
	};

	let fmt = r4type.func2();
	match fmt {
		SINGLE_PRECISION => Some(
			FloatInstruction::NegMulAddSingle {
				dst: r4type.dst().to_fp(),
				mul_lhs: r4type.src1().to_fp(),
				mul_rhs: r4type.src2().to_fp(),
				add: r4type.src3().to_fp(),
				rm,
			}
			.into(),
		),
		_ => None,
	}
}
