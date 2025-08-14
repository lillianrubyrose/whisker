mod sv39;

use tracing::*;

use crate::cpu::csr::AddressTranslationMode;
use crate::cpu::hart::WhiskerHart;
use crate::mem::{Memory, MemoryOpKind};
use crate::ty::{HartMode, TrapIdx, TrapRequestGuaranteed};

pub const PAGE_SIZE: u64 = 4096;
const PAGE_NUM_BITS: u64 = 9;

impl Memory {
	pub fn translate_addr(
		&mut self,
		hart: &mut WhiskerHart,
		addr: u64,
		kind: MemoryOpKind,
	) -> Result<u64, TrapRequestGuaranteed> {
		// translation is only used when in S or U mode
		if !matches!(hart.mode(), HartMode::Supervisor | HartMode::User) {
			trace!("not translating addr {:#018X}, hart not in S or U mode", addr);
			return Ok(addr);
		}

		let phys_addr = match hart.translation_config.get_mode() {
			AddressTranslationMode::Bare => addr,
			AddressTranslationMode::Sv39 => sv39::translate(self, hart, addr, kind)?,
			AddressTranslationMode::Sv48 => todo!(),
			AddressTranslationMode::Sv57 => todo!(),
			mode => unreachable!("unimplemented addr mode {:?}", mode),
		};

		trace!("translated {:#018X}->{:#018X}", addr, phys_addr);
		Ok(phys_addr)
	}
}

fn trap_page_fault(hart: &mut WhiskerHart, effective_addr: u64, kind: MemoryOpKind) -> TrapRequestGuaranteed {
	match kind {
		MemoryOpKind::Instruction => hart.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, effective_addr),
		MemoryOpKind::Load => hart.request_trap(TrapIdx::LOAD_PAGE_FAULT, effective_addr),
		MemoryOpKind::Store => hart.request_trap(TrapIdx::STORE_PAGE_FAULT, effective_addr),
	}
}

fn is_access_allowed(
	hart: &mut WhiskerHart,
	access_kind: MemoryOpKind,
	pte_r: bool,
	pte_w: bool,
	pte_x: bool,
	pte_u: bool,
) -> bool {
	if hart.mode() == HartMode::User && !pte_u {
		return false;
	}

	let permit_supervisor_user = hart.mstatus.get_sum();
	if hart.mode() == HartMode::Supervisor && pte_u && !permit_supervisor_user {
		return false;
	}

	let exec_implicit_readable = hart.mstatus.get_mxr();
	match access_kind {
		MemoryOpKind::Instruction => pte_x,
		MemoryOpKind::Load => pte_r || (exec_implicit_readable && pte_x),
		MemoryOpKind::Store => pte_w,
	}
}
