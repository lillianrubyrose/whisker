use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::R4Type, soft::RoundingMode, ty::SupportedExtensions};

pub fn parse_msub(hart: &mut WhiskerHart, parcel: u32) -> Option<Instruction> {
	use crate::insn32::consts::*;

	// MSUB type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !hart.supports_extensions(SupportedExtensions::FLOAT) {
		return None;
	}

	let r4type = R4Type::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(r4type.func3()) else {
		return None;
	};

	let fmt = r4type.func2();
	match fmt {
		SINGLE_PRECISION => Some(
			FloatInstruction::MulSubSingle {
				dst: r4type.dst().to_fp(),
				mul_lhs: r4type.src1().to_fp(),
				mul_rhs: r4type.src2().to_fp(),
				sub: r4type.src3().to_fp(),
				rm,
			}
			.into(),
		),
		_ => None,
	}
}
