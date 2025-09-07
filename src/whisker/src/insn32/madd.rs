use crate::cpu::hart::WhiskerHart;
use crate::{insn::*, insn32::R4Type, soft::RoundingMode, ty::RiscvExtensions};

pub fn parse_madd(parcel: u32) -> Option<Instruction> {
	use crate::insn32::consts::*;

	let r4type = R4Type::parse(parcel);

	let rm = RoundingMode::from_u8(r4type.func3())?;

	let fmt = r4type.func2();
	match fmt {
		SINGLE_PRECISION => Some(
			FloatInstruction::MulAddSingle {
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
