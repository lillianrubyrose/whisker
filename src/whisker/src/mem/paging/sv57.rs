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

pub struct Sv57;

impl VAddr for Sv57Addr {
	fn from_effective_addr(addr: u64) -> Option<Self> {
		let sign_extended = ((addr << 7) as i64 >> 7) as u64;
		if addr != sign_extended {
			return None;
		}

		let mut vaddr = Sv57Addr::new();
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
			3 => u64::from(self.get_virt_page_3()),
			4 => u64::from(self.get_virt_page_4()),
			_ => unreachable!("Sv57 only has page index 0-4, found {}", idx),
		}
	}

	fn get_page_offset_bits(self) -> u16 {
		self.get_page_offset()
	}
}

impl PAddr for Sv57PhysAddr {
	fn as_u64(self) -> u64 {
		u64::from_le_bytes(self.inner())
	}

	fn set_ppn_idx(&mut self, ppn: u64, idx: u8) {
		match idx {
			0 => self.set_ppn_0(ppn as u16),
			1 => self.set_ppn_1(ppn as u16),
			2 => self.set_ppn_2(ppn as u16),
			3 => self.set_ppn_3(ppn as u16),
			4 => self.set_ppn_4(ppn as u8),
			_ => unreachable!(),
		}
	}

	fn set_page_offset_bits(&mut self, offset: u16) {
		self.set_page_offset(offset);
	}
}

impl Pte for Sv57PageTableEntry {
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
			3 => u64::from(self.get_phys_page_num_3()),
			4 => u64::from(self.get_phys_page_num_4()),
			_ => unreachable!("Sv57 PTEs only have idx 0..=4, got {}", idx),
		}
	}

	fn get_full_phys_page_num(self) -> u64 {
		u64::from(self.get_phys_page_num_0())
			| u64::from(self.get_phys_page_num_1()) << 9
			| u64::from(self.get_phys_page_num_2()) << 18
			| u64::from(self.get_phys_page_num_3()) << 27
			| u64::from(self.get_phys_page_num_4()) << 36
	}

	fn has_reserved_bits_set(self) -> bool {
		self.get_res_54_60() != 0 || self.get_res_61_62() != 0 || self.get_res_63_63() != 0
	}
}

impl Paging for Sv57 {
	type VirtAddr = Sv57Addr;
	type PhysAddr = Sv57PhysAddr;
	type Pte = Sv57PageTableEntry;

	const LEVELS: u8 = 5;
	const PTE_SIZE: u64 = core::mem::size_of::<Sv57PageTableEntry>() as u64;
}

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sv57Addr {
	pub page_offset: U12,
	pub virt_page_0: U9,
	pub virt_page_1: U9,
	pub virt_page_2: U9,
	pub virt_page_3: U9,
	pub virt_page_4: U9,
	_pad: U7,
}

#[bitfields]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sv57PageTableEntry {
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
	phys_page_num_2: U9,
	phys_page_num_3: U9,
	phys_page_num_4: U8,
	res_54_60: U7,
	res_61_62: U2,
	res_63_63: U1,
}

#[bitfields]
#[derive(Clone, Copy, Default)]
pub struct Sv57PhysAddr {
	page_offset: U12,
	ppn_0: U9,
	ppn_1: U9,
	ppn_2: U9,
	ppn_3: U9,
	ppn_4: U8,
	_pad: U8,
}

const _SIZE_ASSERTS: () = {
	assert_xlen!(Sv57Addr, Sv57PageTableEntry, Sv57PhysAddr);
};
