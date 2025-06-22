use core::slice;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::*;

use crate::soft::double::SoftDouble;
use crate::soft::float::SoftFloat;
use crate::ty::HartId;

struct MemoryReservations {
	/// map of hart ID to reservation base address
	/// reservation base addresses are aligned to [`MemoryReservations::RESERVATION_SET_SIZE`]
	reservations: HashMap<HartId, u64>,
}

impl MemoryReservations {
	// MUST be a power of 2
	const RESERVATION_SET_SIZE: u64 = 64;

	fn new() -> Self {
		Self {
			reservations: HashMap::with_capacity(usize::from(HartId::NUM_HARTS)),
		}
	}

	/// sets the reservation for the the hart specified by `hart_id` to be `phys_addr`
	fn reserve(&mut self, hart_id: HartId, phys_addr: u64) {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations.insert(hart_id, aligned_addr);
	}

	/// unreserves `phys_addr` for all harts *other* than the hart specified by `hart_id`
	fn unreserve_addr_other_harts(&mut self, hart_id: HartId, phys_addr: u64) {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations
			.retain(|hart, addr| *hart == hart_id || *addr != aligned_addr);
	}

	/// removes the reservation from the hart specified by `hart_id`, if any exist
	fn unreserve_hart(&mut self, hart_id: HartId) {
		self.reservations.remove(&hart_id);
	}

	fn is_reserved_by_hart(&self, phys_addr: u64, hart_id: HartId) -> bool {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations
			.get(&hart_id)
			.is_some_and(|addr| *addr == aligned_addr)
	}
}

pub struct Memory {
	phys: Box<[u8]>,
	bootrom: Box<[u8]>,
	mappings: HashMap<PageBase, PageEntry>,

	// If we were to do multithreading, this would probably need to be a Send Cell type
	reservations: MemoryReservations,
	atomic_lock: AtomicBool,
}

impl Debug for Memory {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Memory").finish_non_exhaustive()
	}
}

impl Memory {
	/// the reading primitive that does page lookups and such
	/// returns Ok if the read succeeded, or Err(virt) if the read failed
	/// where virt is the failing virtual address
	#[track_caller]
	pub fn read_slice(&self, offset: u64, buf: &mut [u8]) -> Result<(), u64> {
		for (idx, val) in buf.iter_mut().enumerate() {
			let offset = offset + idx as u64;
			let base = PageBase::from_addr(offset);
			let Some(page_entry) = self.mappings.get(&base) else {
				trace!("no page entry for {:#018X}", offset);
				return Err(offset);
			};
			let page_offset = offset - base.0;

			match page_entry {
				PageEntry::PhysBacked { phys_base } => {
					let offset = phys_base + page_offset;
					trace!("Reading from physmem @ {:#018X}", offset);
					*val = self.phys[offset as usize];
				}
				PageEntry::Bootrom { page_base } => {
					let offset = page_base + page_offset;
					trace!("Reading from bootrom @ {:#018X}", offset);
					*val = self.bootrom[offset as usize];
				}
				PageEntry::MMIO { read: on_read, .. } => {
					trace!("Reading from MMIO @ {:#018X}", offset);
					let mut read_val = 0_u8;
					on_read(offset, slice::from_mut(&mut read_val));
					*val = read_val;
				}
			}
		}
		Ok(())
	}

	/// the writing primitive that does page lookups and such
	/// returns Ok if the write succeeded, or Err(virt) if the write
	/// where virt is the failing virtual address
	#[track_caller]
	pub fn write_slice(&mut self, hart_id: HartId, virt_addr: u64, val: &[u8]) -> Result<(), u64> {
		for (idx, val) in val.into_iter().enumerate() {
			let offset = virt_addr + idx as u64;
			let base = PageBase::from_addr(offset);
			let Some(page_entry) = self.mappings.get(&base) else {
				trace!("no page entry for {:#018X}", offset);
				return Err(offset);
			};
			let page_offset = offset - base.0;

			match page_entry {
				PageEntry::PhysBacked { phys_base } => {
					let phys_addr = phys_base + page_offset;

					self.reservations.unreserve_addr_other_harts(hart_id, phys_addr);

					trace!("Writing to physmem @ {:#018X}", phys_base);
					self.phys[phys_addr as usize] = *val;
				}
				// writing to bootrom is allowed, this makes it easier to write bootrom code
				// without having to do loader shenanigans
				PageEntry::Bootrom { page_base } => {
					trace!("Writing to bootrom @ 0x{:#018X}", page_base);
					self.bootrom[(page_base + page_offset) as usize] = *val;
				}
				PageEntry::MMIO { write, .. } => {
					trace!("Writing to MMIO @ {:#018X}", offset);
					write(offset, slice::from_ref(val));
				}
			}
		}
		Ok(())
	}
}

/// addr MUST be aligned to the size of $ty, such that it does not cross a page boundary
macro_rules! read_simple_inner {
	($self:expr, $ty:ty, $addr:expr) => {{
		let Some((page, page_offset)) = $self.lookup_addr($addr) else {
			return Err($addr);
		};

		match page {
			PageEntry::PhysBacked { phys_base } => {
				let offset = phys_base + page_offset;
				trace!("Reading from physmem @ {:#018X}", offset);
				let mut ret = <$ty>::default().to_le_bytes();
				ret.copy_from_slice(&$self.phys[offset as usize..][..core::mem::size_of::<$ty>()]);
				Ok(<$ty>::from_le_bytes(ret))
			}
			PageEntry::Bootrom { page_base } => {
				let offset = page_base + page_offset;
				trace!("Reading from bootrom @ {:#018X}", offset);
				let mut ret = <$ty>::default().to_le_bytes();
				ret.copy_from_slice(&$self.bootrom[offset as usize..][..core::mem::size_of::<$ty>()]);
				Ok(<$ty>::from_le_bytes(ret))
			}
			PageEntry::MMIO { read, .. } => {
				trace!("Reading from MMIO @ {:#018X}", $addr);
				let mut ret = <$ty>::default().to_le_bytes();
				read($addr, &mut ret);
				Ok(<$ty>::from_le_bytes(ret))
			}
		}
	}};
}

/// addr MUST be aligned to the size of $ty, such that it does not cross a page boundary
macro_rules! write_simple_inner {
	($self:expr, $ty:ty, $addr:expr, $val:expr) => {{
		let Some((page, page_offset)) = $self.lookup_addr($addr) else {
			return Err($addr);
		};

		match page {
			PageEntry::PhysBacked { phys_base } => {
				let offset = phys_base + page_offset;
				trace!("writing to physmem @ {:#018X}", offset);
				let val = $val.to_le_bytes();
				$self.phys[offset as usize..][..core::mem::size_of::<$ty>()].copy_from_slice(&val);
				Ok(())
			}
			PageEntry::Bootrom { page_base } => {
				let offset = page_base + page_offset;
				// FIXME: this is temporarily permitted but probably shouldn't be
				warn!("writing to bootrom @ {:#018X}", offset);
				let val = $val.to_le_bytes();
				$self.bootrom[offset as usize..][..core::mem::size_of::<$ty>()].copy_from_slice(&val);
				Ok(())
			}
			PageEntry::MMIO { write, .. } => {
				trace!("writing to MMIO @ {:#018X}", $addr);
				let val = $val.to_le_bytes();
				write($addr, &val);
				Ok(())
			}
		}
	}};
}

impl Memory {
	pub fn read_u8(&self, addr: u64) -> Result<u8, u64> {
		// NOTE: all u8 addresses are aligned, no need for other cases
		read_simple_inner!(self, u8, addr)
	}

	pub fn read_u16(&self, addr: u64) -> Result<u16, u64> {
		if addr % 2 != 0 {
			todo!("unaligned u16 read");
		}

		read_simple_inner!(self, u16, addr)
	}

	pub fn read_u32(&self, addr: u64) -> Result<u32, u64> {
		if addr % 4 != 0 {
			todo!("unaligned u32 read {:#018X}", addr);
		}

		read_simple_inner!(self, u32, addr)
	}

	pub fn read_u64(&self, addr: u64) -> Result<u64, u64> {
		if addr % 8 != 0 {
			todo!("unaligned u64 read");
		}

		read_simple_inner!(self, u64, addr)
	}

	pub fn write_u8(&mut self, addr: u64, val: u8) -> Result<(), u64> {
		// NOTE: all u8 addresses are aligned, no need for other cases
		write_simple_inner!(self, u8, addr, val)
	}

	pub fn write_u16(&mut self, addr: u64, val: u16) -> Result<(), u64> {
		if addr % 2 != 0 {
			todo!("unaligned u16 write");
		}

		write_simple_inner!(self, u16, addr, val)
	}

	pub fn write_u32(&mut self, addr: u64, val: u32) -> Result<(), u64> {
		if addr % 4 != 0 {
			todo!("unaligned u32 write");
		}

		write_simple_inner!(self, u32, addr, val)
	}

	pub fn write_u64(&mut self, addr: u64, val: u64) -> Result<(), u64> {
		if addr % 8 != 0 {
			todo!("unaligned u64 write");
		}

		write_simple_inner!(self, u64, addr, val)
	}

	pub fn read_soft_float(&self, addr: u64) -> Result<SoftFloat, u64> {
		if addr % 4 != 0 {
			todo!("unaligned SoftFloat read");
		}

		read_simple_inner!(self, SoftFloat, addr)
	}

	#[expect(unused, reason = "doubles NYI")]
	pub fn read_soft_double(&self, addr: u64) -> Result<SoftDouble, u64> {
		if addr % 8 != 0 {
			todo!("unaligned SoftDouble read");
		}

		read_simple_inner!(self, SoftDouble, addr)
	}

	pub fn write_soft_float(&mut self, addr: u64, val: SoftFloat) -> Result<(), u64> {
		if addr % 4 != 0 {
			todo!("unaligned SoftFloat write");
		}

		write_simple_inner!(self, SoftFloat, addr, val)
	}

	#[expect(unused, reason = "doubles NYI")]
	pub fn write_soft_double(&mut self, addr: u64, val: SoftDouble) -> Result<(), u64> {
		if addr % 8 != 0 {
			todo!("unaligned SoftDouble write");
		}

		write_simple_inner!(self, SoftDouble, addr, val)
	}

	/// given a virtual address, look up its page entry and the offset into the page
	fn lookup_addr(&self, virt_addr: u64) -> Option<(&PageEntry, u64)> {
		let base = PageBase::from_addr(virt_addr);
		let page_entry = self.mappings.get(&base)?;
		let page_offset = virt_addr - base.0;
		Some((page_entry, page_offset))
	}
}

/// Atomics and reservations
impl Memory {
	/// tries to reserve `virt_addr`, if the address lies in a memory region that supports reservation.
	/// returns Ok(true) if the memory could be reserved, Ok(false) if it could not, or Err(virt_addr)
	/// if the virtual memory did not correspond to any memory.
	fn reserve_virt(&mut self, virt_addr: u64, hart_id: HartId) -> Result<bool, u64> {
		let Some((page, page_offset)) = self.lookup_addr(virt_addr) else {
			return Err(virt_addr);
		};

		match page {
			PageEntry::PhysBacked { phys_base } => {
				self.reservations.reserve(hart_id, phys_base + page_offset);
				Ok(true)
			}
			PageEntry::Bootrom { .. } | PageEntry::MMIO { .. } => return Ok(false),
		}
	}

	/// determines whether `virt_addr` is reserved by the hart corresponding to `hart_id`
	/// returns Ok(true) if the memory is reserved, Ok(false) if it is not, including if the memory
	/// does not support reservation, and Err(virt_addr) if the virtual memory did not correspond to
	/// any memory.
	fn is_reserved_virt(&self, virt_addr: u64, hart_id: HartId) -> Result<bool, u64> {
		let Some((page, page_offset)) = self.lookup_addr(virt_addr) else {
			return Err(virt_addr);
		};

		match page {
			PageEntry::PhysBacked { phys_base } => {
				Ok(self.reservations.is_reserved_by_hart(phys_base + page_offset, hart_id))
			}
			PageEntry::Bootrom { .. } | PageEntry::MMIO { .. } => return Ok(false),
		}
	}

	#[inline(always)]
	fn with_atomic_lock<R, F: FnOnce(&mut Memory) -> R>(&mut self, f: F) -> R {
		while self.atomic_lock.swap(true, Ordering::Acquire) {
			std::hint::spin_loop();
		}

		let result = f(self);

		self.atomic_lock.store(false, Ordering::Release);

		result
	}

	/// Returns Err(virt_addr) on failure
	pub fn load_reserved_word(&mut self, virt_addr: u64, hart_id: HartId) -> Result<u32, u64> {
		self.reserve_virt(virt_addr, hart_id)?;
		Ok(self.read_u32(virt_addr)?)
	}

	/// Returns Err(virt_addr) on failure
	pub fn load_reserved_dword(&mut self, virt_addr: u64, hart_id: HartId) -> Result<u64, u64> {
		self.reserve_virt(virt_addr, hart_id)?;
		Ok(self.read_u64(virt_addr)?)
	}

	/// Returns Ok(successful) or Err(virt_addr)
	pub fn store_conditional_word(&mut self, virt_addr: u64, hart_id: HartId, word: u32) -> Result<bool, u64> {
		let is_reserved = self.is_reserved_virt(virt_addr, hart_id)?;

		// unreservation happens whenever a SC is executed, whether or not it succeeds to store
		self.reservations.unreserve_hart(hart_id);

		if !is_reserved {
			return Ok(false);
		}

		self.with_atomic_lock(|this| this.write_u32(virt_addr, word))?;
		Ok(true)
	}

	/// Returns Ok(successful) or Err(virt_addr)
	pub fn store_conditional_dword(&mut self, virt_addr: u64, hart_id: HartId, dword: u64) -> Result<bool, u64> {
		let is_reserved = self.is_reserved_virt(virt_addr, hart_id)?;

		// unreservation happens whenever a SC is executed, whether or not it succeeds to store
		self.reservations.unreserve_hart(hart_id);

		if !is_reserved {
			return Ok(false);
		}

		self.with_atomic_lock(|this| this.write_u64(virt_addr, dword))?;
		Ok(true)
	}

	/// Returns Ok(original_value) or Err(virt_addr)
	pub fn atomic_op_word<F: FnOnce(u32) -> Option<u32>>(&mut self, virt_addr: u64, op: F) -> Result<u32, u64> {
		// FIXME(memory protection): this may need to ensure that the address is writable before calling op?
		self.with_atomic_lock(|this| {
			let word = this.read_u32(virt_addr)?;

			if let Some(replacement) = op(word) {
				if let Err(failure_addr) = this.write_u32(virt_addr, replacement) {
					return Err(failure_addr);
				}
			}

			Ok(word)
		})
	}

	/// Returns Ok(original_value) or Err(virt_addr)
	pub fn atomic_op_dword<F: FnOnce(u64) -> Option<u64>>(&mut self, virt_addr: u64, op: F) -> Result<u64, u64> {
		// FIXME(memory protection): this may need to ensure that the address is writable before calling op?
		self.with_atomic_lock(|this| {
			let dword = this.read_u64(virt_addr)?;

			if let Some(replacement) = op(dword) {
				if let Err(failure_addr) = this.write_u64(virt_addr, replacement) {
					return Err(failure_addr);
				}
			}

			Ok(dword)
		})
	}
}

pub enum PageEntry {
	PhysBacked {
		phys_base: u64,
	},
	Bootrom {
		page_base: u64,
	},
	MMIO {
		read: Box<dyn Fn(u64, &mut [u8])>,
		write: Box<dyn Fn(u64, &[u8])>,
	},
}

fn align_to_page(addr: u64) -> u64 {
	(addr + (PAGE_SIZE - 1)) & !(PAGE_SIZE - 1)
}

const PAGE_SIZE: u64 = 4096;
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// INVARIANT: is a multiple of PAGE_SIZE
pub struct PageBase(u64);

impl PageBase {
	pub fn from_addr(addr: u64) -> Self {
		Self(addr & !(PAGE_SIZE - 1))
	}
}

impl Debug for PageBase {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("PageBase")
			.field(&format_args!("{:#018X}", self.0))
			.finish()
	}
}

#[derive(Default)]
pub struct MemoryBuilder {
	// size of physical memory
	physical: Option<u64>,
	// physical addr -> (virt addr, map_size bytes)
	physical_mappings: HashMap<PageBase, (PageBase, u64)>,

	misc_maps: HashMap<PageBase, PageEntry>,
	// bootrom data, virtual offset
	bootrom: Option<(Box<[u8]>, PageBase)>,
}

impl MemoryBuilder {
	pub fn bootrom(mut self, mut bootrom: Vec<u8>, addr: PageBase) -> Self {
		assert!(self.bootrom.is_none(), "cannot set bootrom more than once");
		let padded_len = align_to_page(bootrom.len() as u64);
		bootrom.resize(padded_len as usize, 0_u8);
		self.bootrom = Some((bootrom.into_boxed_slice(), addr));
		self
	}

	pub fn physical_size(mut self, size: u64) -> Self {
		assert!(
			self.physical.is_none(),
			"cannot set physical memory size more than once"
		);
		assert_eq!(size % PAGE_SIZE, 0);

		self.physical = Some(size);
		self
	}

	pub fn phys_mapping(mut self, virt_base: PageBase, phys_base: PageBase, size: u64) -> Self {
		assert_eq!(size % PAGE_SIZE, 0);
		let prev = self.physical_mappings.insert(virt_base, (phys_base, size));
		assert!(prev.is_none());
		self
	}

	pub fn add_mapping(mut self, virt_addr: PageBase, entry: PageEntry) -> Self {
		let prev = self.misc_maps.insert(virt_addr, entry);
		assert!(
			prev.is_none(),
			"cannot overwrite mapping for virtual address {:#018X}",
			virt_addr.0
		);
		self
	}

	#[track_caller] // provides better panic location for caller
	pub fn build(self) -> Memory {
		let phys = vec![0_u8; self.physical.unwrap_or(0) as usize].into_boxed_slice();
		let mut mappings = HashMap::new();

		let (bootrom, virt_addr) = self.bootrom.unwrap_or_default();
		for offset in (0..bootrom.len() as u64).step_by(PAGE_SIZE as usize) {
			// INVARIANT: virtual address is verified to be a multiple of page size
			// and loop ensures that it's only offset by page size
			mappings.insert(PageBase(virt_addr.0 + offset), PageEntry::Bootrom { page_base: offset });
		}

		for (virt_base, (phys_base, map_size)) in self.physical_mappings.into_iter() {
			for offset in (0..map_size).step_by(PAGE_SIZE as usize) {
				let virt = PageBase(virt_base.0 + offset);
				let prev = mappings.insert(
					virt,
					PageEntry::PhysBacked {
						phys_base: phys_base.0 + offset,
					},
				);
				assert!(
					prev.is_none(),
					"overlapped virtual address {:?} in physical mapping {:?} size {:#018X})",
					virt,
					virt_base,
					map_size
				);
			}
		}

		for (virt, entry) in self.misc_maps.into_iter() {
			let prev = mappings.insert(virt, entry);
			assert!(prev.is_none(), "overlapped virtual address {:?} in misc mapping", virt);
		}

		Memory {
			phys,
			mappings,
			bootrom,
			reservations: MemoryReservations::new(),
			atomic_lock: AtomicBool::default(),
		}
	}
}
