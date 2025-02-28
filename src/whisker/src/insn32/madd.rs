use crate::{
	cpu::WhiskerCpu,
	insn::{float::FloatInstruction, Instruction},
	insn32::R4Type,
	soft::RoundingMode,
	ty::{SupportedExtensions, TrapIdx},
};

pub fn parse_madd(cpu: &mut WhiskerCpu, parcel: u32) -> Result<Instruction, ()> {
	use crate::insn32::consts::*;

	// MADD type is reserved for standard F extension only
	// all opcodes in this type require F (and D requires F)
	if !cpu.supported_extensions.has(SupportedExtensions::FLOAT) {
		cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
		return Err(());
	}

	let r4type = R4Type::parse(parcel);

	let Some(rm) = RoundingMode::from_u8(r4type.func3()) else {
		cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
		return Err(());
	};

	let fmt = r4type.func2();

	match fmt {
		SINGLE_PRECISION => Ok(FloatInstruction::MulAdd {
			dst: r4type.dst().to_fp(),
			mul_lhs: r4type.src1().to_fp(),
			mul_rhs: r4type.src2().to_fp(),
			add: r4type.src3().to_fp(),
			rm,
		}
		.into()),
		_ => unimplemented!("op-madd func2:{fmt:#04b}"),
	}
}
