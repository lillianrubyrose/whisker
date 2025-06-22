pub mod atomic;
pub mod compressed;
pub mod csr;
pub mod float;
pub mod int;
pub mod multiply;
pub mod privileged;

use atomic::AtomicInstruction;
use compressed::CompressedInstruction;
use float::FloatInstruction;
use int::IntInstruction;
use multiply::MultiplyInstruction;
use num_conv::prelude::*;
use privileged::PrivilegedInstruction;

use crate::insn::csr::CSRInstruction;
use crate::ty::{SupportedExtensions, TrapIdx};
use crate::util::extract_bits_16;
use crate::{insn16, insn32, log, WhiskerCpu};

#[derive(Debug)]
pub enum Instruction {
	IntExtension(IntInstruction),
	FloatExtension(FloatInstruction),
	Csr(CSRInstruction),
	CompressedExtension(CompressedInstruction),
	AtomicExtension(AtomicInstruction),
	MultiplyInstruction(MultiplyInstruction),
	PrivilegedInstruction(PrivilegedInstruction),
}

impl Instruction {
	/// tries to fetch an instruction, or returns Err if a trap happened during the fetch
	pub fn fetch_instruction(cpu: &mut WhiskerCpu) -> Result<(Instruction, u64), ()> {
		let pc = cpu.pc;
		let support_compressed = cpu.supported_extensions.has(SupportedExtensions::COMPRESSED);

		let parcel1 = match cpu.mem.read_u16(pc) {
			Ok(parcel1) => parcel1,
			Err(addr) => {
				log!(cpu, "  could not read start of instruction from {:#018X}", pc);
				// FIXME: this addr is probably not right?
				cpu.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, addr);
				return Err(());
			}
		};

		// all encodings with the low 16 bits all 0s are invalid.
		// NOTE: the length of an all-zeros instruction is considered
		// to be the length of the smallest supported instruction
		// FIXME: currently we believe this does not matter?
		if parcel1 == 0 {
			log!(cpu, "  tried to execute all 0 instruction at {:#018X}", pc);
			cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
			return Err(());
		}

		if extract_bits_16(parcel1, 0, 1) != 0b11 {
			if support_compressed {
				match insn16::parse(cpu, parcel1) {
					Some(insn) => Ok((insn, 2)),
					None => {
						log!(cpu, "  unable to parse 16 bit instruction {parcel1:#06X}");
						cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
						Err(())
					}
				}
			} else {
				log!(
					cpu,
					"  tried to execute compressed instruction {:#06X} at {:#018X} when compressed instructions were disabled",
					parcel1,
					pc
				);
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
				Err(())
			}
		} else if extract_bits_16(parcel1, 2, 4) != 0b111 {
			// FIXME(alignment): parcel must be constructed from 2 reads because when the C extension is
			// enabled, 32 bit instructions may start at addresses only aligned to a multiple of 2.
			let high_parcel = match cpu.mem.read_u16(pc + 2) {
				Ok(p) => p,
				Err(addr) => {
					log!(cpu, "  could not read u32 instruction from {:#018X}", pc);
					// FIXME: this addr is probably not right?
					cpu.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, addr);
					return Err(());
				}
			};
			let full_parcel = high_parcel.extend::<u32>() << 16 | parcel1.extend::<u32>();
			match insn32::parse(cpu, full_parcel) {
				Some(insn) => Ok((insn, 4)),
				None => {
					cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, full_parcel.extend());
					Err(())
				}
			}
		} else if extract_bits_16(parcel1, 0, 5) == 0b011111 {
			if support_compressed {
				todo!("implement 48bit instruction")
			} else {
				// FIXME: this is probably not the right mtval
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
				Err(())
			}
		} else if extract_bits_16(parcel1, 0, 6) == 0b0111111 {
			if support_compressed {
				todo!("implement 64bit instruction")
			} else {
				// FIXME: this is probably not the right mtval
				cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
				Err(())
			}
		} else {
			// FIXME: this is probably not the right mtval
			cpu.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, parcel1.extend());
			Err(())
		}
	}
}
