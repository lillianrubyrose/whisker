use std::collections::HashMap;
use std::fmt::Debug;
use std::io::{self, Write};

pub struct Memory {
	phys: Box<[u8]>,
	mappings: HashMap<PageBase, PageEntry>,
}

impl Debug for Memory {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Memory").finish_non_exhaustive()
	}
}

impl Memory {
	pub fn new(size: u64) -> Self {
		assert!(size % PAGE_SIZE == 0);
		// FIXME: let custom maps come from user

		let mut mappings = HashMap::new();

		for page_addr in (0..size).step_by(PAGE_SIZE as usize) {
			// INVARIANT: the loop construction ensures this is a multiple of page size
			let base = PageBase(page_addr);
			let entry = PageEntry::PhysBacked { phys_base: page_addr };
			mappings.insert(base, entry);
		}

		// MMIO mapping
		let base = PageBase(0x1000_0000);
		mappings.insert(
			base,
			PageEntry::MMIO {
				on_read: Box::new(|_| unimplemented!("read from UART")),
				on_write: Box::new(|addr, val| {
					if addr == 0x1000_0000 {
						print!("{}", val as char);
						io::stdout().flush().unwrap();
					}
				}),
			},
		);

		Self {
			phys: vec![0; size as usize].into_boxed_slice(),
			mappings,
		}
	}

	/// the reading primitive that does page lookups and such
	/// returns Ok if the read succeeded, or Err(virt) if the read failed
	/// where virt is the failing virtual address
	pub fn read_slice(&self, offset: u64, buf: &mut [u8]) -> Result<(), u64> {
		for (idx, val) in buf.iter_mut().enumerate() {
			let offset = offset + idx as u64;
			let base = PageBase::from_addr(offset);
			let Some(page_entry) = self.mappings.get(&base) else {
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
			}
		}
		Ok(())
	}

	/// the writing primitive that does page lookups and such
	/// returns Ok if the write succeeded, or Err(virt) if the write
	/// where virt is the failing virtual address
	pub fn write_slice(&mut self, offset: u64, val: &[u8]) -> Result<(), u64> {
		for (idx, val) in val.into_iter().enumerate() {
			let offset = offset + idx as u64;
			let base = PageBase::from_addr(offset);
			let Some(page_entry) = self.mappings.get(&base) else {
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
			}
		}
		Ok(())
	}
}

enum PageEntry {
	PhysBacked {
		phys_base: u64,
	},
	MMIO {
		on_read: Box<dyn Fn(u64) -> u8>,
		on_write: Box<dyn Fn(u64, u8)>,
	},
}

const PAGE_SIZE: u64 = 4096;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// INVARIANT: is a multiple of PAGE_SIZE
struct PageBase(u64);

impl PageBase {
	fn from_addr(addr: u64) -> Self {
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
