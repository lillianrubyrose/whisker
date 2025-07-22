use std::fmt::Debug;

use bitflags::bitflags;
use tracing::trace;

pub mod mmio;

use crate::cpu::WhiskerCpu;
use crate::mem::mmio::MMIOKind;
use crate::soft::double::SoftDouble;
use crate::soft::float::SoftFloat;
use crate::ty::{TrapIdx, TrapRequestGuaranteed};

pub const MEM_PAGE_SIZE: u64 = 4096;

#[derive(Debug)]
pub struct Memory {
	/// INVARIANT: sorted by start address such that lowest addresses are first
	/// INVARIANT: regions never overlap
	regions: Vec<MemoryRegion>,
}

impl Memory {
	fn region_for_addr_mut(&mut self, phys_addr: u64) -> Option<&mut MemoryRegion> {
		trace!("looking for region for {:#018X}", phys_addr);
		for r in self.regions.iter_mut() {
			let r_end = r.start + r.len;
			if phys_addr >= r.start && phys_addr < r_end {
				trace!("  found region {:?}", r);
				return Some(r);
			}
		}

		trace!("  no region found");
		None
	}
}

macro_rules! impl_mem_read_write {
	($($ty:ty),*$(,)*) => {
		impl Memory {
			paste::paste! {
			    $(
			    #[allow(dead_code)]
				pub fn [<read_phys_ $ty:snake>](
					&mut self,
					cpu: &mut WhiskerCpu,
					phys_addr: u64,
					kind: ReadKind
				) -> Result<$ty, TrapRequestGuaranteed> {
					let Some(region) = self.region_for_addr_mut(phys_addr) else {
						let e = match kind {
                            ReadKind::Normal | ReadKind::Atomic => cpu.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr),
                            ReadKind::Instruction => cpu.request_trap(TrapIdx::INSTRUCTION_ACCESS_FAULT, phys_addr),
                            ReadKind::PageTable => todo!("page table read fault"),
                        };
                        return Err(e);
					};
					check_read_access(cpu, region, phys_addr, kind, core::mem::size_of::<$ty>() as u8)?;

					match region.kind {
						MemoryKind::MainMemory { ref mut backing } => {
							let offset = phys_addr - region.start;
							let mut ret = <$ty>::default().to_le_bytes();
							ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<$ty>()]);
							Ok(<$ty>::from_le_bytes(ret))
						}
						MemoryKind::MMIO(kind) => {
							let mut ret = <$ty>::default().to_le_bytes();
							kind.read(cpu, phys_addr, ret.as_mut_slice());
							Ok(<$ty>::from_le_bytes(ret))
						}
					}
				}

				#[allow(dead_code)]
               	pub fn [<write_phys_ $ty:snake>](
              		&mut self,
              		cpu: &mut WhiskerCpu,
              		phys_addr: u64,
              		kind: WriteKind,
                    val: $ty,
               	) -> Result<(), TrapRequestGuaranteed> {
                    let Some(region) = self.region_for_addr_mut(phys_addr) else {
                       	// FIXME: these are the same, is this always the case? should this be inline?
                       	let e = match kind {
                      		WriteKind::Normal => cpu.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr),
                      		WriteKind::Atomic => cpu.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr),
                       	};
                       	return Err(e);
					};
					check_write_access(cpu, region, phys_addr, kind, core::mem::size_of::<$ty>() as u8)?;

					match region.kind {
						MemoryKind::MainMemory { ref mut backing } => {
							let offset = phys_addr - region.start;
							let bytes = val.to_le_bytes();
							backing[offset as usize..][..core::mem::size_of::<$ty>()].copy_from_slice(&bytes);
							Ok(())
						}
						MemoryKind::MMIO(kind) => {
						    let bytes = val.to_le_bytes();
							kind.write(cpu, phys_addr, bytes.as_slice());
							Ok(())
						}
					}
               	}
				)*
			}
		}
	};
}

impl_mem_read_write!(u8, u16, u32, u64, SoftFloat, SoftDouble);

/// checks that a read of `size` from `phys_addr` is allowed in `region`
fn check_read_access(
	cpu: &mut WhiskerCpu,
	region: &MemoryRegion,
	phys_addr: u64,
	kind: ReadKind,
	size: u8,
) -> Result<(), TrapRequestGuaranteed> {
	trace!(
		"checking {:?} at {:#018X} for size {:02X} in region {:?}",
		kind,
		phys_addr,
		size,
		region
	);
	let access_kinds = region.attrs.access_kinds;
	let max_size = region.attrs.max_size;
	match kind {
		ReadKind::Normal => {
			if !access_kinds.contains(AccessKind::READ) || size > max_size {
				return Err(cpu.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(cpu.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, phys_addr));
			}
		}
		ReadKind::Instruction => {
			if !access_kinds.contains(AccessKind::EXEC) || size > max_size {
				return Err(cpu.request_trap(TrapIdx::INSTRUCTION_ACCESS_FAULT, phys_addr));
			}
			// NOTE: instruction misaligned traps are generated on control flow, not when fetching
			debug_assert!(
				access_kinds.contains(AccessKind::MISALIGNED) || phys_addr % u64::from(size) == 0,
				"tried to fetch misaligned instruction (THIS SHOULD NEVER HAPPEN)"
			);
		}
		ReadKind::Atomic => {
			if !access_kinds.contains(AccessKind::READ | AccessKind::ATOMIC) || size > max_size {
				return Err(cpu.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(cpu.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, phys_addr));
			}
		}

		ReadKind::PageTable => todo!("check page table access"),
	}

	Ok(())
}

/// checks that a write of `size` from `phys_addr` is allowed in `region`
fn check_write_access(
	cpu: &mut WhiskerCpu,
	region: &MemoryRegion,
	phys_addr: u64,
	kind: WriteKind,
	size: u8,
) -> Result<(), TrapRequestGuaranteed> {
	trace!(
		"checking {:?} at {:#018X} for size {:02X} in region {:?}",
		kind,
		phys_addr,
		size,
		region
	);
	let access_kinds = region.attrs.access_kinds;
	let max_size = region.attrs.max_size;
	match kind {
		WriteKind::Normal => {
			if !access_kinds.contains(AccessKind::WRITE) || size > max_size {
				return Err(cpu.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(cpu.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, phys_addr));
			}
		}
		WriteKind::Atomic => {
			if !access_kinds.contains(AccessKind::WRITE | AccessKind::ATOMIC) || size > max_size {
				return Err(cpu.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(cpu.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, phys_addr));
			}
		}
	}

	Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadKind {
	/// reading memory for normal accesses
	Normal,
	/// reading memory as part of instructon fetching
	Instruction,
	/// reads for LR (note: not AMOs, those go through writes)
	Atomic,
	/// reading memory for page table lookups
	PageTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteKind {
	/// writing memory for normal accesses
	Normal,
	/// accesses for AMO or SC
	Atomic,
}

#[derive(Debug)]
pub struct MemoryRegion {
	start: u64,
	// INVARIANT: has the same length as kind.backing if MemoryKind is MainMemory
	len: u64,
	kind: MemoryKind,
	attrs: AccessAttrs,
}

impl MemoryRegion {
	pub fn new_main_mem(start: u64, len: u64, backing: Box<[u8]>, attrs: AccessAttrs) -> Self {
		assert_eq!(backing.len() as u64, len);
		Self::new(start, len, MemoryKind::MainMemory { backing }, attrs)
	}

	pub fn new_mmio(start: u64, len: u64, mmio: MMIOKind, attrs: AccessAttrs) -> Self {
		Self::new(start, len, MemoryKind::MMIO(mmio), attrs)
	}

	pub fn new(start: u64, len: u64, kind: MemoryKind, attrs: AccessAttrs) -> Self {
		assert_eq!(start % MEM_PAGE_SIZE, 0);
		assert_eq!(len % MEM_PAGE_SIZE, 0);

		// FIXME: implement this
		assert!(
			!attrs.access_kinds.contains(AccessKind::MISALIGNED),
			"misaligned accesses not yet implemented"
		);

		if attrs.access_kinds.contains(AccessKind::EXEC) {
			assert!(
				attrs.access_kinds.contains(AccessKind::READ),
				"EXEC regions must have READ perms"
			);
		}

		match kind {
			MemoryKind::MainMemory { .. } => assert!(
				attrs
					.access_kinds
					.contains(AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC),
				"main memory regions must support RWX"
			),
			MemoryKind::MMIO { .. } => assert!(
				!attrs.access_kinds.contains(AccessKind::EXEC),
				"IO regions must not support EXEC"
			),
		}

		Self {
			start,
			len,
			kind,
			attrs,
		}
	}
}

pub enum MemoryKind {
	MainMemory { backing: Box<[u8]> },
	MMIO(MMIOKind),
}

impl Debug for MemoryKind {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			MemoryKind::MainMemory { backing } => write!(f, "MemoryKind::MainMemory({:#018X} bytes)", backing.len()),
			MemoryKind::MMIO(mmiokind) => write!(f, "MemoryKind::MMIO({:?})", mmiokind),
		}
	}
}

#[derive(Debug, Clone, Copy)]
pub struct AccessAttrs {
	/// the maximum size of an access that is supported, in bytes
	max_size: u8,
	/// what kinds of accesses are allowed on this region
	access_kinds: AccessKind,
}

impl AccessAttrs {
	pub fn new(max_size: u8, access_kinds: AccessKind) -> Self {
		Self { max_size, access_kinds }
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct AccessKind : u8 {
		const READ = 0b0000_0001;
		const WRITE = 0b0000_0010;
		const EXEC = 0b0000_0100;
		/// if set, corresponds to AMOArithmetic (all AMO operations) and RsrvNonEventual (LR/SC supported)
		/// if unset, corresponds to AMONone and RsrvNone (no atomic, no LR/SC)
		const ATOMIC = 0b0000_1000;
		const MISALIGNED = 0b0001_0000;
	}
}

#[derive(Debug, Default)]
pub struct MemoryBuilder {
	regions: Vec<MemoryRegion>,
}

impl MemoryBuilder {
	pub fn add_region(mut self, region: MemoryRegion) -> Self {
		let region_end = region.start + region.len;
		for existing in self.regions.iter() {
			let existing_end = existing.start + existing.len;
			assert!(
				region_end <= existing.start || region.start >= existing_end,
				"memory region overlapped"
			);
		}

		self.regions.push(region);
		self.regions.sort_unstable_by_key(|r| r.start);

		self
	}

	pub fn build(self) -> Memory {
		Memory { regions: self.regions }
	}
}
