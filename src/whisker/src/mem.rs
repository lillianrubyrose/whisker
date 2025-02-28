use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::*;

use crate::soft::double::SoftDouble;
use crate::soft::float::SoftFloat;

struct MemoryReservations {
	// Physical address to hart id, this would be important if we ever do multithreading
	reservations: HashMap<u64, usize>,
}

impl MemoryReservations {
	// For RV64 hardware I believe this is common
	const CACHE_LINE_SIZE: u64 = 64;

	fn new() -> Self {
		Self {
			reservations: HashMap::with_capacity(1024),
		}
	}

	fn reserve(&mut self, phys_addr: u64, hart_id: usize) {
		let aligned_addr = phys_addr & !(Self::CACHE_LINE_SIZE - 1);
		self.reservations.insert(aligned_addr, hart_id);
	}

	fn unreserve(&mut self, phys_addr: u64) {
		let aligned_addr = phys_addr & !(Self::CACHE_LINE_SIZE - 1);
		self.reservations.remove(&aligned_addr);
	}

	fn is_reserved(&mut self, phys_addr: u64, hart_id: usize) -> bool {
		let aligned_addr = phys_addr & !(Self::CACHE_LINE_SIZE - 1);
		self.reservations
			.get(&aligned_addr)
			.is_some_and(|hart| *hart == hart_id)
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
				PageEntry::MMIO { on_read, .. } => {
					trace!("Reading from MMIO @ {:#018X}", offset);
					*val = on_read(offset);
				}
			}
		}
		Ok(())
	}

	/// the writing primitive that does page lookups and such
	/// returns Ok if the write succeeded, or Err(virt) if the write
	/// where virt is the failing virtual address
	#[track_caller]
	pub fn write_slice(&mut self, offset: u64, val: &[u8]) -> Result<(), u64> {
		for (idx, val) in val.into_iter().enumerate() {
			let offset = offset + idx as u64;
			let base = PageBase::from_addr(offset);
			let Some(page_entry) = self.mappings.get(&base) else {
				trace!("no page entry for {:#018X}", offset);
				return Err(offset);
			};
			let page_offset = offset - base.0;

			match page_entry {
				PageEntry::PhysBacked { phys_base } => {
					// Invalidate reservations on memory whenever it's written to
					let phys_addr = phys_base + page_offset;
					self.reservations.unreserve(phys_addr);

					trace!("Writing to physmem @ {:#018X}", phys_base);
					self.phys[phys_addr as usize] = *val;
				}
				// writing to bootrom is allowed, this makes it easier to write bootrom code
				// without having to do loader shenanigans
				PageEntry::Bootrom { page_base } => {
					trace!("Writing to bootrom @ 0x{:#018X}", page_base);
					self.bootrom[(page_base + page_offset) as usize] = *val;
				}
				PageEntry::MMIO { on_write, .. } => {
					trace!("Writing to MMIO @ {:#018X}", offset);
					on_write(offset, *val);
				}
			}
		}
		Ok(())
	}

	/// Returns Err(virt_addr) on failure
	fn translate_address(&self, virt_addr: u64) -> Result<u64, u64> {
		let base = PageBase::from_addr(virt_addr);
		let Some(page_entry) = self.mappings.get(&base) else {
			return Err(virt_addr);
		};
		let page_offset = virt_addr - base.0;

		match page_entry {
			PageEntry::PhysBacked { phys_base } => Ok(phys_base + page_offset),
			PageEntry::Bootrom { page_base: _ }
			| PageEntry::MMIO {
				on_read: _,
				on_write: _,
			} => Err(virt_addr), // TODO: What to do for Bootrom & MMIO?
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
	pub fn load_reserved_word(&mut self, virt_addr: u64, hart_id: usize) -> Result<u32, u64> {
		let phys_addr = self.translate_address(virt_addr)?;
		self.reservations.reserve(phys_addr, hart_id);
		Ok(self.read_u32(virt_addr)?)
	}

	/// Returns Err(virt_addr) on failure
	pub fn load_reserved_dword(&mut self, virt_addr: u64, hart_id: usize) -> Result<u64, u64> {
		let phys_addr = self.translate_address(virt_addr)?;
		self.reservations.reserve(phys_addr, hart_id);
		Ok(self.read_u64(virt_addr)?)
	}

	/// Returns Ok(successful) or Err(virt_addr)
	pub fn store_conditional_word(&mut self, virt_addr: u64, hart_id: usize, word: u32) -> Result<bool, u64> {
		let phys_addr = self.translate_address(virt_addr)?;

		let is_reserved = self.reservations.is_reserved(phys_addr, hart_id);
		if !is_reserved {
			return Ok(false);
		}

		self.with_atomic_lock(|this| {
			let write_result = this.write_u32(virt_addr, word);

			this.reservations.unreserve(phys_addr);

			Ok(write_result.is_ok())
		})
	}

	/// Returns Ok(successful) or Err(virt_addr)
	pub fn store_conditional_dword(&mut self, virt_addr: u64, hart_id: usize, dword: u64) -> Result<bool, u64> {
		let phys_addr = self.translate_address(virt_addr)?;

		let is_reserved = self.reservations.is_reserved(phys_addr, hart_id);
		if !is_reserved {
			return Ok(false);
		}

		self.with_atomic_lock(|this| {
			let write_result = this.write_u64(virt_addr, dword);

			this.reservations.unreserve(phys_addr);

			Ok(write_result.is_ok())
		})
	}

	/// Returns Ok(original_value) or Err(virt_addr)
	pub fn atomic_op_word<F: FnOnce(u32) -> Option<u32>>(&mut self, virt_addr: u64, op: F) -> Result<u32, u64> {
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
		on_read: Box<dyn Fn(u64) -> u8>,
		on_write: Box<dyn Fn(u64, u8)>,
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

macro_rules! impl_mem_rw {
	($($ty:ty),*) => {
		#[allow(unused)]
		impl Memory {
			$(paste::paste!{
				pub fn [<read_ $ty:snake>](&self, offset: u64) -> Result<$ty, u64> {
					let mut buf = <$ty>::to_le_bytes($ty::default());
					self.read_slice(offset, &mut buf)?;
					Ok(<$ty>::from_le_bytes(buf))
				}

				pub fn [<write_ $ty:snake>](&mut self, offset: u64, val: $ty) -> Result<(), u64> {
					self.write_slice(offset, $ty::to_le_bytes(val).as_slice())?;
					Ok(())
				}
			})*
		}
	};
}

impl_mem_rw!(u8, u16, u32, u64, SoftFloat, SoftDouble);

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
