mod sv39;
mod sv57;

use crate::{
	cpu::{csr::AddressTranslationMode, hart::WhiskerHart},
	mem::{
		Memory, MemoryOpKind, PageTableCacheKey,
		paging::{sv39::Sv39, sv57::Sv57},
	},
	tracing::*,
	ty::{HartMode, TrapIdx, TrapRequestGuaranteed},
};

pub const PAGE_SIZE: u64 = 4096;

pub trait VAddr
where
	Self: Sized,
{
	fn from_effective_addr(addr: u64) -> Option<Self>;
	fn as_effective_addr(self) -> u64;

	fn get_page_num(self, idx: u8) -> u64;
	fn get_page_offset_bits(self) -> u16;
}

pub trait PAddr {
	fn as_u64(self) -> u64;

	fn set_ppn_idx(&mut self, ppn: u64, idx: u8);
	fn set_page_offset_bits(&mut self, offset: u16);
}

pub trait Pte
where
	Self: Sized,
{
	fn read_from_mem(
		memory: &Memory,
		hart: &mut WhiskerHart,
		pte_addr: u64,
		access_kind: MemoryOpKind,
	) -> Result<Self, TrapRequestGuaranteed>;

	fn as_u64(self) -> u64;

	fn get_valid_bit(self) -> bool;
	fn get_read_bit(self) -> bool;
	fn get_write_bit(self) -> bool;
	fn get_execute_bit(self) -> bool;
	fn get_user_accessible_bit(self) -> bool;
	fn get_global_bit(self) -> bool;
	fn get_accessed_bit(self) -> bool;
	fn get_dirty_bit(self) -> bool;
	fn set_accessed_bit(&mut self, val: bool);
	fn set_dirty_bit(&mut self, val: bool);
	fn get_phys_page_num(self, idx: u8) -> u64;
	fn get_full_phys_page_num(self) -> u64;

	fn has_reserved_bits_set(self) -> bool;
}

pub trait Paging {
	type VirtAddr: VAddr + Copy;
	type PhysAddr: PAddr + Copy + Default;
	type Pte: Pte + Copy + PartialEq;

	const LEVELS: u8;
	const PTE_SIZE: u64;
}

pub fn translate<P: Paging>(
	memory: &Memory,
	hart: &mut WhiskerHart,
	addr: u64,
	access_kind: MemoryOpKind,
) -> Result<(u64, bool), TrapRequestGuaranteed> {
	trace!("translating addr {:#018X} for access {:?} with SV39", addr, access_kind);
	let Some(va) = P::VirtAddr::from_effective_addr(addr) else {
		trace!("addr not valid SV39 virt addr: {:#018X}", addr);
		return Err(trap_page_fault(hart, addr, access_kind));
	};

	let base = hart.translation_config.get_root_page_num() * PAGE_SIZE;
	trace!("PTE root at {:#018X}", base);
	let (pte, pte_addr, level_idx) = find_page::<P>(memory, hart, access_kind, P::LEVELS - 1, base, va)?;
	trace!("found final PTE {:#018X} at level {}", pte.as_u64(), level_idx);

	let r = pte.get_read_bit();
	let w = pte.get_write_bit();
	let x = pte.get_execute_bit();
	let u = pte.get_user_accessible_bit();
	if !is_access_allowed(hart, access_kind, r, w, x, u) {
		trace!("access not allowed: {:?} r:{} w:{} x:{} u:{}", access_kind, r, w, x, u);
		return Err(trap_page_fault(hart, addr, access_kind));
	}

	trace!("PTE access allowed");

	let accessed = pte.get_accessed_bit();
	let dirty = pte.get_dirty_bit();

	// If ADEU, then do Svadu behavior
	if hart.menvcfg.get_adue() {
		let mut new_pte = pte;
		// any memory access sets the accessed bit
		if !accessed {
			new_pte.set_accessed_bit(true);
		}
		if !dirty && access_kind == MemoryOpKind::Store {
			new_pte.set_dirty_bit(true);
		}
		if new_pte != pte {
			memory
				.write_pte(hart, pte_addr, new_pte.as_u64())
				.map_err(|_| trap_page_fault(hart, pte_addr, MemoryOpKind::Store))?;
		}
	} else {
		// Otherwise, do Svade behavior
		if !accessed || (!dirty && access_kind == MemoryOpKind::Store) {
			return Err(trap_page_fault(hart, addr, access_kind));
		}
	}

	let mut phys_addr = P::PhysAddr::default();
	trace!("va page offset {:#018X}", va.get_page_offset_bits());
	phys_addr.set_page_offset_bits(va.get_page_offset_bits());

	// superpage bits
	for i in 0..level_idx {
		let page_num = va.get_page_num(i);
		phys_addr.set_ppn_idx(page_num, i);
	}

	for i in level_idx..P::LEVELS {
		let page_num = pte.get_phys_page_num(i);
		phys_addr.set_ppn_idx(page_num, i);
	}

	Ok((phys_addr.as_u64(), pte.get_global_bit()))
}

fn find_page<P: Paging>(
	memory: &Memory,
	hart: &mut WhiskerHart,
	access_kind: MemoryOpKind,
	level_idx: u8,
	base: u64,
	va: P::VirtAddr,
) -> Result<(P::Pte, u64, u8), TrapRequestGuaranteed> {
	trace!(
		"page lookup base {:#018X} level {} va {:#018X}",
		base,
		level_idx,
		va.as_effective_addr()
	);
	let pte_addr = base + P::PTE_SIZE * va.get_page_num(level_idx);
	trace!("pte addr for level {}: {:#018X}", level_idx, pte_addr);
	let pte = P::Pte::read_from_mem(memory, hart, pte_addr, access_kind)?;

	if !pte.get_valid_bit() {
		trace!("PTE valid bit clear: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}
	if !pte.get_read_bit() && pte.get_write_bit() {
		trace!("PTE W without R: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}
	if pte.has_reserved_bits_set() {
		trace!("PTE reserved bits set: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}
	// if pte.get_res_54_60() != 0 || pte.get_res_61_62() != 0 || pte.get_res_63_63() != 0 {}

	trace!("found valid PTE: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);

	// PTE with R or X are valid leaf PTEs
	if pte.get_read_bit() || pte.get_execute_bit() {
		trace!("leaf PTE {:#018X} at level {}", pte.as_u64(), level_idx);

		// check superpage alignment
		for i in 0..level_idx {
			if pte.get_phys_page_num(i) != 0 {
				trace!(
					"misaligned superpage: pte {:#018X} at level {} has non-zero ppn for level {}",
					pte.as_u64(),
					level_idx,
					i
				);
				return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
			}
		}

		return Ok((pte, pte_addr, level_idx));
	}

	trace!("parent PTE {:#018X} at level {}", pte.as_u64(), level_idx);

	// here the PTE must not have any of RWX, which makes it a pointer to a lower level
	let Some(level_idx) = level_idx.checked_sub(1) else {
		trace!("PTE level <0");
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	};
	let ppn = pte.get_full_phys_page_num();
	trace!("parent PTE {:#018X} points to ppn {:#015X}", pte.as_u64(), ppn);
	let base = ppn * PAGE_SIZE;

	find_page::<P>(memory, hart, access_kind, level_idx, base, va)
}

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
		let asid = hart.translation_config.get_address_space_id();
		let page_table_cache = self.page_table_cache.upgradable_read();

		let phys_addr = if let Some(virt_base) =
			page_table_cache.get(&PageTableCacheKey::new(PageTableCacheKey::GLOBAL_ASID, page))
		{
			virt_base + (addr & (PAGE_SIZE - 1))
		} else if let Some(virt_base) = page_table_cache.get(&PageTableCacheKey::new(asid, page)) {
			virt_base + (addr & (PAGE_SIZE - 1))
		} else {
			core::hint::cold_path();
			let (phys_addr, _is_global) = match translation_mode {
				AddressTranslationMode::Sv39 => translate::<Sv39>(self, hart, addr, kind)?,
				AddressTranslationMode::Sv48 => todo!("sv48 translation not implemented"),
				AddressTranslationMode::Sv57 => translate::<Sv57>(self, hart, addr, kind)?,
				AddressTranslationMode::Bare => unreachable!("already checked"),
				mode => unreachable!("unimplemented addr mode {:?}", mode),
			};

			// FIXME: Why doesn't this work >:c
			// let mut page_table_cache = parking_lot::RwLockUpgradableReadGuard::upgrade(page_table_cache);
			// if is_global {
			// 	page_table_cache.insert(
			// 		PageTableCacheKey::new(PageTableCacheKey::GLOBAL_ASID, page),
			// 		phys_addr & !(PAGE_SIZE - 1),
			// 	);
			// } else {
			// 	page_table_cache.insert(PageTableCacheKey::new(asid, page), phys_addr & !(PAGE_SIZE - 1));
			// }
			phys_addr
		};

		trace!("translated {:#018X}->{:#018X}", addr, phys_addr);
		Ok(phys_addr)
	}

	pub fn clear_vm_cache(&self, asid: u64, vaddr: u64) {
		debug!("clearing vm cache asid {:#018X} vaddr {:#018X}", asid, vaddr);

		let mut page_table_cache = self.page_table_cache.write();

		// Page 130 riscv-privileged.pdf
		// if rs1 = 0 & rs2 = 0   - The fence also invalidates all address-translation cache entries, for all address spaces.
		//
		// if rs1 = 0 & rs2 != 0  - The fence also invalidates all address-translation cache entries matching the address space
		//                          identified by integer register rs2, except for entries containing global mappings.
		//
		// if rs1 != 0 & rs2 = 0  - The fence also invalidates all address-translation cache entries that contain leaf page table
		//                          entries corresponding to the virtual address in rs1, for all address spaces.
		//
		// if rs1 != 0 & rs2 != 0 - The fence also invalidates all address-translation cache entries that contain leaf page table
		//                          entries corresponding to the virtual address in rs1 and that match the address space
		//                          identified by integer register rs2, except for entries containing global mappings.

		if asid == 0 && vaddr == 0 {
			page_table_cache.clear();
		} else if vaddr == 0 {
			page_table_cache.retain(|key, _| key.asid() == PageTableCacheKey::GLOBAL_ASID || key.asid() != asid as u16);
		} else if asid == 0 {
			let page = vaddr & !(PAGE_SIZE - 1);
			page_table_cache.retain(|key, _| key.page() != page);
		} else {
			let page = vaddr & !(PAGE_SIZE - 1);
			page_table_cache.remove(&PageTableCacheKey::new(asid as u16, page));
		}
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
