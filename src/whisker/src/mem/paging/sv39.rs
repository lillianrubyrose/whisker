use bitfield::prelude::*;

use crate::{
	assert_xlen,
	cpu::hart::WhiskerHart,
	mem::{
		Memory, MemoryOpKind,
		paging::{PAddr, Paging, Pte, VAddr},
	},
	ty::TrapRequestGuaranteed,
};

pub struct Sv39;

impl VAddr for Sv39Addr {
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

	fn get_page_offset_bits(self) -> u16 {
		self.get_page_offset()
	}
}

impl PAddr for Sv39PhysAddr {
	fn as_u64(self) -> u64 {
		u64::from_le_bytes(self.inner())
	}

	fn set_ppn_idx(&mut self, ppn: u64, idx: u8) {
		match idx {
			0 => self.set_ppn0(ppn as u16),
			1 => self.set_ppn1(ppn as u16),
			2 => self.set_ppn2(ppn as u32),
			_ => unreachable!(),
		}
	}

	fn set_page_offset_bits(&mut self, offset: u16) {
		self.set_page_offset(offset);
	}
}

impl Pte for Sv39PageTableEntry {
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

	fn as_u64(self) -> u64 {
		u64::from_le_bytes(self.inner())
	}

	fn get_valid_bit(self) -> bool {
		self.get_valid()
	}

	fn get_read_bit(self) -> bool {
		self.get_read()
	}

	fn get_write_bit(self) -> bool {
		self.get_write()
	}

	fn get_execute_bit(self) -> bool {
		self.get_execute()
	}

	fn get_user_accessible_bit(self) -> bool {
		self.get_user_accessible()
	}

	fn get_global_bit(self) -> bool {
		self.get_global()
	}

	fn get_accessed_bit(self) -> bool {
		self.get_accessed()
	}

	fn get_dirty_bit(self) -> bool {
		self.get_dirty()
	}

	fn set_accessed_bit(&mut self, val: bool) {
		self.set_accessed(val);
	}

	fn set_dirty_bit(&mut self, val: bool) {
		self.set_dirty(val);
	}

	fn get_phys_page_num(self, idx: u8) -> u64 {
		match idx {
			0 => u64::from(self.get_phys_page_num_0()),
			1 => u64::from(self.get_phys_page_num_1()),
			2 => u64::from(self.get_phys_page_num_2()),
			_ => unreachable!("Sv39 PTEs only have idx 0..=2, got {}", idx),
		}
	}

	fn get_full_phys_page_num(self) -> u64 {
		u64::from(self.get_phys_page_num_0())
			| u64::from(self.get_phys_page_num_1()) << 9
			| u64::from(self.get_phys_page_num_2()) << 18
	}

	fn has_reserved_bits_set(self) -> bool {
		self.get_res_54_60() != 0 || self.get_res_61_62() != 0 || self.get_res_63_63() != 0
	}
}

impl Paging for Sv39 {
	type VirtAddr = Sv39Addr;
	type PhysAddr = Sv39PhysAddr;
	type Pte = Sv39PageTableEntry;

	const LEVELS: u8 = 3;
	const PTE_SIZE: u64 = core::mem::size_of::<Sv39PageTableEntry>() as u64;
}

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sv39Addr {
	pub page_offset: U12,
	pub virt_page_0: U9,
	pub virt_page_1: U9,
	pub virt_page_2: U9,
	_pad: U25,
}

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sv39PageTableEntry {
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

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Sv39PhysAddr {
	page_offset: U12,
	ppn0: U9,
	ppn1: U9,
	ppn2: U26,
	_pad_56_64: U8,
}

const _SIZE_ASSERTS: () = {
	assert_xlen!(Sv39Addr, Sv39PageTableEntry, Sv39PhysAddr);
};
