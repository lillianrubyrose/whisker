use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::R4Type,
	soft::RoundingMode,
	ty::SupportedExtensions,
};

pub fn parse_nmsub(cpu: &mut WhiskerCpu, parcel: u32) -> Option<Instruction> {
	use crate::insn32::consts::*;

	// NMSUB type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		return None;
	}

	let r4type = R4Type::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(r4type.func3()) else {
		return None;
	};

	let fmt = r4type.func2();
	match fmt {
		SINGLE_PRECISION => Some(
			FloatInstruction::NegMulSubSingle {
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
