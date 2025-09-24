use bitfield::prelude::*;

use crate::{
	cpu::hart::WhiskerHart,
	mem::{
		Memory, MemoryOpKind,
		paging::{PAGE_SIZE, is_access_allowed, trap_page_fault},
	},
	tracing::*,
	ty::TrapRequestGuaranteed,
};

const SV_39_LEVELS: u8 = 3;
const PTE_SIZE: u64 = core::mem::size_of::<Sv39PageTableEntry>() as u64;

pub fn translate(
	memory: &Memory,
	hart: &mut WhiskerHart,
	addr: u64,
	access_kind: MemoryOpKind,
) -> Result<u64, TrapRequestGuaranteed> {
	trace!("translating addr {:#018X} for access {:?} with SV39", addr, access_kind);
	let Some(va) = Sv39Addr::from_effective_addr(addr) else {
		trace!("addr not valid SV39 virt addr: {:#018X}", addr);
		return Err(trap_page_fault(hart, addr, access_kind));
	};

	let base = hart.translation_config.get_root_page_num() * PAGE_SIZE;
	trace!("PTE root at {:#018X}", base);
	let (pte, level_idx) = find_page(memory, hart, access_kind, SV_39_LEVELS - 1, base, va)?;
	trace!("found final PTE {:#018X} at level {}", pte.as_u64(), level_idx);

	let r = pte.get_read();
	let w = pte.get_write();
	let x = pte.get_execute();
	let u = pte.get_user_accessible();
	if !is_access_allowed(hart, access_kind, r, w, x, u) {
		trace!("access not allowed: {:?} r:{} w:{} x:{} u:{}", access_kind, r, w, x, u);
		return Err(trap_page_fault(hart, addr, access_kind));
	}

	trace!("PTE access allowed");

	// TODO: check A and D bits

	let mut phys_addr = Sv39PhysAddr::new();
	trace!("va page offset {:#018X}", va.get_page_offset());
	phys_addr.set_page_offset(va.get_page_offset());

	// copy superpage bits from virt addr
	if level_idx > 0 {
		for idx in 0..(level_idx - 1) {
			let page_num = va.get_page_num(idx);
			phys_addr.set_ppn_idx(page_num, idx);
		}
	}

	for idx in level_idx..SV_39_LEVELS {
		let page_num = pte.get_phys_page_num(idx);
		phys_addr.set_ppn_idx(page_num, idx);
	}

	Ok(phys_addr.as_u64())
}

fn find_page(
	memory: &Memory,
	hart: &mut WhiskerHart,
	access_kind: MemoryOpKind,
	level_idx: u8,
	base: u64,
	va: Sv39Addr,
) -> Result<(Sv39PageTableEntry, u8), TrapRequestGuaranteed> {
	trace!(
		"page lookup base {:#018X} level {} va {:#018X}",
		base,
		level_idx,
		va.as_effective_addr()
	);
	let pte_addr = base + PTE_SIZE * va.get_page_num(level_idx);
	trace!("pte addr for level {}: {:#018X}", level_idx, pte_addr);
	let pte = Sv39PageTableEntry::read_from_mem(memory, hart, pte_addr, access_kind)?;

	if !pte.get_valid() {
		trace!("PTE valid bit clear: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}
	if !pte.get_read() && pte.get_write() {
		trace!("PTE W without R: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}
	if pte.get_res_54_60() != 0 || pte.get_res_61_62() != 0 || pte.get_res_63_63() != 0 {
		trace!("PTE reserved bits set: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);
		return Err(trap_page_fault(hart, va.as_effective_addr(), access_kind));
	}

	trace!("found valid PTE: {:#018X} at {:#018X}", pte.as_u64(), pte_addr);

	// PTE with R or X are valid leaf PTEs
	if pte.get_read() || pte.get_execute() {
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

		return Ok((pte, level_idx));
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

	find_page(memory, hart, access_kind, level_idx, base, va)
}

//	fn get_pte(&mut self, hart: &mut WhiskerHart, pte_addr: u64) ->

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Sv39Addr {
	pub page_offset: U12,
	pub virt_page_0: U9,
	pub virt_page_1: U9,
	pub virt_page_2: U9,
	_pad: U25,
}

impl Sv39Addr {
	/// Creates a virtual address from an effective address `addr`.
	/// Effective addresses are the address that an instruction or instruction fetch tried to access.
	/// Returns Some if the effective address is a valid Sv39 virtual address and None otherwise.
	/// Addresses are valid if and only if bits 39..=63 are equal to bit 38, that is, addresses
	/// are sign extended.
	fn from_effective_addr(addr: u64) -> Option<Self> {
		let sign_extended = ((addr << 25) as i64 >> 25) as u64;
		if addr != sign_extended {
			return None;
		}

		let mut vaddr = Sv39Addr::new();
		vaddr.set_inner(addr.to_le_bytes());
		Some(vaddr)
	}

	fn as_effective_addr(self) -> u64 {
		u64::from_le_bytes(self.inner())
	}

	fn get_page_num(self, idx: u8) -> u64 {
		match idx {
			0 => u64::from(self.get_virt_page_0()),
			1 => u64::from(self.get_virt_page_1()),
			2 => u64::from(self.get_virt_page_2()),
			_ => unreachable!("Sv39 only has page index 0,1,2, found {}", idx),
		}
	}
}

#[bitfields]
struct Sv39PageTableEntry {
	valid: bool,
	read: bool,
	write: bool,
	execute: bool,
	user_accessible: bool,
	global: bool,
	accessed: bool,
	dirty: bool,
	_impl_ignore: U2,
	phys_page_num_0: U9,
	phys_page_num_1: U9,
	phys_page_num_2: U26,
	res_54_60: U7,
	res_61_62: U2,
	res_63_63: U1,
}

impl Sv39PageTableEntry {
	fn read_from_mem(
		memory: &Memory,
		hart: &mut WhiskerHart,
		pte_addr: u64,
		access_kind: MemoryOpKind,
	) -> Result<Self, TrapRequestGuaranteed> {
		let pte = memory.read_pte(hart, pte_addr, access_kind)?;
		let mut this = Self::new();
		this.set_inner(pte.to_le_bytes());
		Ok(this)
	}

	fn get_phys_page_num(&self, idx: u8) -> u64 {
		match idx {
			0 => u64::from(self.get_phys_page_num_0()),
			1 => u64::from(self.get_phys_page_num_1()),
			2 => u64::from(self.get_phys_page_num_2()),
			_ => unreachable!("Sv39 PTEs only have idx 0..=2, got {}", idx),
		}
	}

	fn get_full_phys_page_num(&self) -> u64 {
		u64::from(self.get_phys_page_num_0())
			| u64::from(self.get_phys_page_num_1()) << 9
			| u64::from(self.get_phys_page_num_2()) << 18
	}

	fn as_u64(&self) -> u64 {
		u64::from_le_bytes(self.inner())
	}
}

#[bitfields]
struct Sv39PhysAddr {
	page_offset: U12,
	ppn0: U9,
	ppn1: U9,
	ppn2: U26,
	_pad_56_64: U8,
}

impl Sv39PhysAddr {
	fn set_ppn_idx(&mut self, ppn: u64, idx: u8) {
		match idx {
			0 => self.set_ppn0(ppn as u16),
			1 => self.set_ppn1(ppn as u16),
			2 => self.set_ppn2(ppn as u32),
			_ => unreachable!(),
		}
	}

	fn as_u64(&self) -> u64 {
		u64::from_le_bytes(self.inner())
	}
}

const _SIZE_ASSERTS: () = {
	assert!(core::mem::size_of::<Sv39Addr>() == core::mem::size_of::<u64>());
	assert!(core::mem::size_of::<Sv39PageTableEntry>() == core::mem::size_of::<u64>());
	assert!(core::mem::size_of::<Sv39PhysAddr>() == core::mem::size_of::<u64>());
};
