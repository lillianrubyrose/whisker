pub mod atomic;
pub mod compressed;
pub mod csr;
pub mod float;
pub mod int;
pub mod multiply;

use atomic::AtomicInstruction;
use compressed::CompressedInstruction;
use float::FloatInstruction;
use int::IntInstruction;
use multiply::MultiplyInstruction;

use crate::insn::csr::CSRInstruction;
use crate::ty::{SupportedExtensions, TrapIdx};
use crate::util::extract_bits_16;
use crate::{insn16, insn32, WhiskerCpu};

#[derive(Debug)]
pub enum Instruction {
	IntExtension(IntInstruction),
	FloatExtension(FloatInstruction),
	Csr(CSRInstruction),
	CompressedExtension(CompressedInstruction),
	AtomicExtension(AtomicInstruction),
	MultiplyInstruction(MultiplyInstruction),
}

impl Instruction {
	/// tries to fetch an instruction, or returns Err if a trap happened during the fetch
	pub fn fetch_instruction(cpu: &mut WhiskerCpu) -> Result<(Instruction, u64), ()> {
		let pc = cpu.pc;
		let support_compressed = cpu.supported_extensions.has(SupportedExtensions::COMPRESSED);

		let parcel1 = match cpu.mem.read_u16(pc) {
			Ok(parcel1) => parcel1,
			Err(addr) => {
				cpu.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, addr);
				return Err(());
			}
		};

		if extract_bits_16(parcel1, 0, 1) != 0b11 {
			if support_compressed {
				let insn = insn16::parse(cpu, parcel1)?;
				Ok((insn.into(), 2))
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, pc);
				Err(())
			}
		} else if extract_bits_16(parcel1, 2, 4) != 0b111 {
			let full_parcel = match cpu.mem.read_u32(pc) {
				Ok(p) => p,
				Err(addr) => {
					cpu.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, addr);
					return Err(());
				}
			};
			let insn = insn32::parse(cpu, full_parcel)?;
			Ok((insn, 4))
		} else if extract_bits_16(parcel1, 0, 5) == 0b011111 {
			if support_compressed {
				todo!("implement 48bit instruction")
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, pc);
				Err(())
			}
		} else if extract_bits_16(parcel1, 0, 6) == 0b0111111 {
			if support_compressed {
				todo!("implement 64bit instruction")
			} else {
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, pc);
				Err(())
			}
		} else {
			cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, pc);
			Err(())
		}
	}
}
