use std::collections::HashMap;
use std::fmt::Debug;

use log::*;

pub struct Memory {
	phys: Box<[u8]>,
	bootrom: Box<[u8]>,
	mappings: HashMap<PageBase, PageEntry>,
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
					*val = self.phys[(phys_base + page_offset) as usize];
				}
				PageEntry::MMIO { on_read, .. } => {
					*val = on_read(offset);
				}
				PageEntry::Bootrom { page_base } => {
					*val = self.bootrom[(page_base + page_offset) as usize];
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
					self.phys[(phys_base + page_offset) as usize] = *val;
				}
				PageEntry::MMIO { on_write, .. } => {
					on_write(offset, *val);
				}
				// writing to bootrom is allowed, this makes it easier to write bootrom code
				// without having to do loader shenanigans
				PageEntry::Bootrom { page_base } => {
					self.bootrom[(page_base + page_offset) as usize] = *val;
				}
			}
		}
		Ok(())
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
		impl Memory {
			$(paste::paste!{
				pub fn [<read_ $ty>](&self, offset: u64) -> Result<$ty, u64> {
					let mut buf = <$ty>::to_le_bytes(0);
					self.read_slice(offset, &mut buf)?;
					Ok(<$ty>::from_le_bytes(buf))
				}

				pub fn [<write_ $ty>](&mut self, offset: u64, val: $ty) -> Result<(), u64> {
					self.write_slice(offset, $ty::to_le_bytes(val).as_slice())?;
					Ok(())
				}
			})*
		}
	};
}

impl_mem_rw!(u8, u16, u32, u64);

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
		}
	}
}
