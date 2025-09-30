mod sv39;
mod sv57;

use crate::{
	cpu::{csr::AddressTranslationMode, hart::WhiskerHart},
	mem::{Memory, MemoryOpKind},
	tracing::*,
	ty::{HartMode, TrapIdx, TrapRequestGuaranteed},
};

pub const PAGE_SIZE: u64 = 4096;

impl Memory {
	pub fn translate_addr(
		&self,
		hart: &mut WhiskerHart,
		addr: u64,
		kind: MemoryOpKind,
	) -> Result<u64, TrapRequestGuaranteed> {
		if !hart.should_translate(kind) {
			trace!("hart not translating addresses");
			return Ok(addr);
		}

		// fast path to not acquire lock
		let translation_mode = hart.translation_config.get_mode();
		if matches!(translation_mode, AddressTranslationMode::Bare) {
			return Ok(addr);
		}

		let page = addr & !(PAGE_SIZE - 1);
		let page_table_cache = self.page_table_cache.upgradable_read();
		let phys_addr = if let Some(virt_base) = page_table_cache.get(&page) {
			virt_base + (addr & (PAGE_SIZE - 1))
		} else {
			core::hint::cold_path();
			let phys_addr = match translation_mode {
				AddressTranslationMode::Sv39 => sv39::translate(self, hart, addr, kind)?,
				AddressTranslationMode::Sv48 => todo!("sv48 translation not implemented"),
				AddressTranslationMode::Sv57 => sv57::translate(self, hart, addr, kind)?,
				AddressTranslationMode::Bare => unreachable!("already checked"),
				mode => unreachable!("unimplemented addr mode {:?}", mode),
			};

			let mut page_table_cache = parking_lot::RwLockUpgradableReadGuard::upgrade(page_table_cache);
			// FIXME: figure out why this breaks things
			//page_table_cache.insert(page, phys_addr & !(PAGE_SIZE - 1));
			phys_addr
		};

		trace!("translated {:#018X}->{:#018X}", addr, phys_addr);
		Ok(phys_addr)
	}

	pub fn clear_vm_cache(&self, asid: u64, vaddr: u64) {
		debug!("clearing vm cache asid {:#018X} vaddr {:#018X}", asid, vaddr);
		self.page_table_cache.write().clear();
	}
}

fn trap_page_fault(hart: &mut WhiskerHart, effective_addr: u64, kind: MemoryOpKind) -> TrapRequestGuaranteed {
	match kind {
		MemoryOpKind::Instruction => hart.request_trap(TrapIdx::INSTRUCTION_PAGE_FAULT, effective_addr),
		MemoryOpKind::Load => hart.request_trap(TrapIdx::LOAD_PAGE_FAULT, effective_addr),
		MemoryOpKind::Store => hart.request_trap(TrapIdx::STORE_PAGE_FAULT, effective_addr),
	}
}

#[allow(clippy::fn_params_excessive_bools, reason = "they're all obvious")]
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
