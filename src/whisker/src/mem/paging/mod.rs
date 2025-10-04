use std::fmt::Debug;

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
		virt_addr: u64,
		kind: MemoryOpKind,
	) -> Result<u64, TrapRequestGuaranteed> {
		let should_translate = matches!(hart.get_effective_mode(kind), HartMode::Supervisor | HartMode::User);
		if !should_translate {
			trace!("hart not translating addresses");
			return Ok(virt_addr);
		}

		// fast path to not acquire lock
		let mode = hart.translation_config.get_mode();
		if matches!(mode, AddressTranslationMode::Bare) {
			return Ok(virt_addr);
		}

		// address translation algorithm taken from priv isa section 12.3.2 - Virtual Address Translation Process
		let virt_addr = VirtAddr(virt_addr);
		let asid = hart.translation_config.get_address_space_id();
		let mut pte_base_addr = hart.translation_config.get_root_page_num() * PAGE_SIZE;
		let mut level_idx = mode.get_levels() - 1;
		trace!(
			"translating {:?} with pte base {:#018X} mode {:?}, level_idx {}",
			virt_addr, pte_base_addr, mode, level_idx
		);

		// FIXME: we just disable the cache if a pte doesn't match memory, is this the best way to do that?
		let mut use_cache = true;

		// loop over steps 2-9 as needed
		let pte = loop {
			trace!(
				"page lookup base {:#018X} va {:?} level {}",
				pte_base_addr, virt_addr, level_idx
			);

			let ptesize = mode.get_pte_size();
			let pte_addr = pte_base_addr + (ptesize * virt_addr.get_page_num(level_idx));
			trace!("pte addr: {:#018X}", pte_addr);
			let (pte, was_cached) = PageTableEntry::read_from_memory(self, use_cache, hart, asid, pte_addr, kind)?;

			if !pte.is_valid() {
				trace!("PTE valid bit clear: {:?} at {:#018X}", pte, pte_addr);
				return Err(trap_page_fault(hart, virt_addr.inner(), kind));
			}
			if !pte.is_readable() && pte.is_writeable() {
				trace!("PTE W without R: {:?} at {:#018X}", pte, pte_addr);
				return Err(trap_page_fault(hart, virt_addr.inner(), kind));
			}

			if !was_cached {
				let mut cache = self.page_table_cache.write();
				let cache_asid = if pte.is_global() { None } else { Some(asid) };
				cache.insert((cache_asid, pte_addr), pte.0);
			}

			// pointer to the next page table level
			if !pte.is_readable() && !pte.is_writeable() {
				let Some(new_level_idx) = level_idx.checked_sub(1) else {
					trace!("PTE level <0");
					return Err(trap_page_fault(hart, virt_addr.inner(), kind));
				};
				level_idx = new_level_idx;
				let ppn = pte.get_full_phys_page_num();
				trace!("parent PTE {:?} points to ppn {:#015X}", pte, ppn);
				pte_base_addr = ppn * PAGE_SIZE;
				// loop back to step 2
				continue;
			}

			// otherwise it's a leaf PTE
			trace!("leaf PTE {:?} at level {}", pte, level_idx);

			// check superpage alignment
			for i in 0..level_idx {
				if pte.get_phys_page_num(mode, i) != 0 {
					trace!(
						"misaligned superpage: pte {:?} at level {} has non-zero ppn for level {}",
						pte, level_idx, i
					);
					return Err(trap_page_fault(hart, virt_addr.inner(), kind));
				}
			}

			if !is_access_allowed(hart, pte, kind) {
				trace!("access not allowed: {:?}", kind);
				return Err(trap_page_fault(hart, virt_addr.inner(), kind));
			}

			// FIXME: should this check the bits in memory or the cached bits?
			// update A and D bits
			if !pte.is_accessed() || (kind == MemoryOpKind::Store && !pte.is_dirty()) {
				if hart.menvcfg.get_adue() {
					// Svadu
					let (mem_pte, was_cached) =
						PageTableEntry::read_from_memory(self, false, hart, asid, pte_addr, kind)?;
					assert!(!was_cached); // sanity
					if pte == mem_pte {
						let mut pte = pte;
						pte.set_accessed(true);
						if kind == MemoryOpKind::Store {
							pte.set_dirty(true);
						}
						self.write_pte(hart, pte_addr, pte.0)?;
					} else {
						// The PTE changed in memory, so we need to restart the translation.
						use_cache = false;
						continue;
					}
				} else {
					// Svade
					return Err(trap_page_fault(hart, virt_addr.inner(), kind));
				}
			}

			// if A and D bit updates did not happen, or passed, exit with the leaf pte
			break pte;
		};

		let phys_addr = PhysAddr::from_pte_and_virt_addr(mode, pte, level_idx, virt_addr);

		trace!("translated {:?}->{:?}", virt_addr, phys_addr);
		Ok(phys_addr.inner())
	}

	pub fn clear_vm_cache(&self, asid: u64, vaddr: u64) {
		debug!("clear_vm_cache: asid={:#X}, vaddr={:#X}", asid, vaddr);
		let mut page_table_cache = self.page_table_cache.write();

		// Privileged spec: Section 12.2.1
		// rs1 = vaddr, rs2 = asid

		if vaddr == 0 && asid == 0 {
			// rs1=0 && rs2=0
			// Invalidates all address-translation cache entries for all address spaces.
			trace!("clear_vm_cache: invalidating cache");
			page_table_cache.clear();
		} else if vaddr == 0 && asid != 0 {
			// rs1=0 && rs2!=0
			// Invalidates all entries for the ASID, except for global mappings.
			trace!("clear_vm_cache: invalidating asid {:#X}", asid);
			page_table_cache.retain(|(entry_asid, _), _| match entry_asid {
				Some(entry_asid) => *entry_asid != asid as u16,
				None => true,
			});
		} else if asid == 0 {
			// rs1=0 && rs2!=0
			// Invalidates all entries for a specific virtual address.
			// For now I'll implement this as a full cache invalidation.
			trace!(
				"clear_vm_cache: over-fencing vaddr {:#X} (full cache invalidation)",
				vaddr
			);
			page_table_cache.clear();
		} else {
			// rs1!=0 && rs2!=0
			// Invalidates all entries for a specific virtual address and ASID.
			// For now I'll implement this as rs1=0 && rs2!=0
			trace!("clear_vm_cache: over-fencing vaddr {:#X} for asid {:#X}", vaddr, asid);
			page_table_cache.retain(|(entry_asid, _), _| match entry_asid {
				Some(entry_asid) => *entry_asid != asid as u16,
				None => true,
			});
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

fn is_access_allowed(hart: &mut WhiskerHart, pte: PageTableEntry, access_kind: MemoryOpKind) -> bool {
	let mode = hart.get_effective_mode(access_kind);

	if mode == HartMode::User && !pte.is_user_accessible() {
		return false;
	}

	if mode == HartMode::Supervisor
		&& matches!(access_kind, MemoryOpKind::Load | MemoryOpKind::Store)
		&& pte.is_user_accessible()
		&& !hart.mstatus.get_sum()
	{
		return false;
	}

	let exec_implicit_readable = hart.mstatus.get_mxr();
	match access_kind {
		MemoryOpKind::Instruction => pte.is_executable(),
		MemoryOpKind::Load => pte.is_readable() || (exec_implicit_readable && pte.is_executable()),
		MemoryOpKind::Store => pte.is_writeable(),
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VirtAddr(u64);

impl VirtAddr {
	fn inner(self) -> u64 {
		self.0
	}

	fn get_page_num(self, idx: u8) -> u64 {
		// NOTE: currently sv64 is not ratified, so we dont include idx 5 in this check
		// even though it's implemented such that it would work correctly in the match
		debug_assert!(idx <= 4, "invalid va idx {}", idx);
		(self.0 >> (9 * idx + 12)) & 0b1_1111_1111
	}

	fn get_page_offset(self) -> u64 {
		self.0 & 0b1111_1111_1111
	}
}

impl Debug for VirtAddr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("VirtAddr")
			.field_with(|f| write!(f, "{:#018X}", self.0))
			.finish()
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PhysAddr(u64);

impl PhysAddr {
	fn inner(self) -> u64 {
		self.0
	}

	fn from_pte_and_virt_addr(
		// this may matter for sv64?
		_mode: AddressTranslationMode,
		pte: PageTableEntry,
		pte_idx: u8,
		virt_addr: VirtAddr,
	) -> Self {
		let mut phys = virt_addr.get_page_offset();
		let va_mask = ((1_u64 << (pte_idx * 9)) - 1) << 12;
		phys |= virt_addr.0 & va_mask;
		let ppn_mask = (u64::MAX << 10) >> (10 + 10 + pte_idx * 9) << 10;
		phys |= (pte.0 & ppn_mask) << 2;
		Self(phys)
	}
}

impl Debug for PhysAddr {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("PhysAddr")
			.field_with(|f| write!(f, "{:#018X}", self.0))
			.finish()
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
	fn read_from_memory(
		memory: &Memory,
		may_cache: bool,
		hart: &mut WhiskerHart,
		asid: u16,
		addr: u64,
		orig_access_kind: MemoryOpKind,
	) -> Result<(Self, bool), TrapRequestGuaranteed> {
		let cache = memory.page_table_cache.read();
		if may_cache {
			// check for this asid
			if let Some(pte) = cache.get(&(Some(asid), addr)) {
				return Ok((PageTableEntry(*pte), true));
			}
			// check global mapping cache
			if let Some(pte) = cache.get(&(None, addr)) {
				return Ok((PageTableEntry(*pte), true));
			}
		}

		let pte = memory.read_pte(hart, addr, orig_access_kind)?;
		Ok((Self(pte), false))
	}

	#[inline]
	fn is_valid(self) -> bool {
		self.0 & 0b1 != 0
	}
	#[inline]
	fn is_readable(self) -> bool {
		self.0 & 0b10 != 0
	}
	#[inline]
	fn is_writeable(self) -> bool {
		self.0 & 0b100 != 0
	}
	#[inline]
	fn is_executable(self) -> bool {
		self.0 & 0b1000 != 0
	}
	#[inline]
	fn is_user_accessible(self) -> bool {
		self.0 & 0b1_0000 != 0
	}
	#[inline]
	fn is_global(self) -> bool {
		self.0 & 0b10_0000 != 0
	}
	#[inline]
	fn is_accessed(self) -> bool {
		self.0 & 0b100_0000 != 0
	}
	#[inline]
	fn set_accessed(&mut self, accessed: bool) {
		self.0 &= !0b100_0000;
		self.0 |= u64::from(accessed) << 6;
	}
	#[inline]
	fn is_dirty(self) -> bool {
		self.0 & 0b1000_0000 != 0
	}
	#[inline]
	fn set_dirty(&mut self, dirty: bool) {
		self.0 &= !0b1000_0000;
		self.0 |= u64::from(dirty) << 7;
	}

	fn get_phys_page_num(self, mode: AddressTranslationMode, idx: u8) -> u64 {
		match mode {
			AddressTranslationMode::Sv39 => match idx {
				0 => (self.0 >> 10) & 0b1_11111111,
				1 => (self.0 >> (9 + 10)) & 0b1_11111111,
				2 => (self.0 >> (9 + 9 + 10)) & 0b11_11111111_11111111_11111111,
				_ => panic!("invalid idx for sv39: {}", idx),
			},
			AddressTranslationMode::Sv48 => todo!("get_phys_page_num sv48"),
			AddressTranslationMode::Sv57 => match idx {
				0 => (self.0 >> 10) & 0b1_11111111,
				1 => (self.0 >> (9 + 10)) & 0b1_11111111,
				2 => (self.0 >> (9 + 9 + 10)) & 0b1_11111111,
				3 => (self.0 >> (9 + 9 + 9 + 10)) & 0b1_11111111,
				4 => (self.0 >> (9 + 9 + 9 + 9 + 10)) & 0b11111111,
				_ => panic!("invalid idx for sv57: {}", idx),
			},
			AddressTranslationMode::__reserved_Sv64 => todo!("get_phys_page_num sv64"),
			_ => unimplemented!("unimplemented mode {:?}", mode),
		}
	}

	#[inline]
	fn get_full_phys_page_num(self) -> u64 {
		(self.0 & !0b11100000_00000000_00000000_00000000_00000000_00000000_00000011_11111111) >> 10
	}
}

impl Debug for PageTableEntry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("PageTableEntry")
			.field_with(|f| write!(f, "{:#018X}", self.0))
			.finish()
	}
}
