use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt::Debug;

use crate::tracing::*;
use bitflags::bitflags;
use gdbstub::target::ext::breakpoints::WatchKind;
use num_conv::Extend;
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub mod mmio;

mod paging;

use crate::cpu::hart::WhiskerHart;
use crate::mem::mmio::MMIOKind;
use crate::soft::double::SoftDouble;
use crate::soft::float::SoftFloat;
use crate::ty::{HartId, TrapIdx, TrapRequestGuaranteed};

pub const MEM_PAGE_SIZE: u64 = 4096;

#[derive(Debug)]
pub struct Memory {
	/// cache of page base addresses to physical addresses
	page_table_cache: parking_lot::RwLock<BTreeMap<u64, u64>>,
	/// INVARIANT: sorted by start address such that lowest addresses are first
	/// INVARIANT: regions never overlap
	regions: parking_lot::RwLock<Vec<MemoryRegion>>,

	reservations: parking_lot::RwLock<MemoryReservations>,

	pub watchpoints: RwLock<Vec<(u64, u64, WatchKind)>>,
}

impl Memory {
	fn region_for_addr(&self, phys_addr: u64) -> Option<MappedRwLockReadGuard<'_, MemoryRegion>> {
		let regions = self.regions.read();
		regions
			.iter()
			.position(|r| {
				let r_end = r.start + r.len;
				phys_addr >= r.start && phys_addr < r_end
			})
			.map(|position| RwLockReadGuard::map(regions, |regions| &regions[position]))
	}

	fn region_for_addr_mut(&self, phys_addr: u64) -> Option<MappedRwLockWriteGuard<'_, MemoryRegion>> {
		let regions = self.regions.write();
		regions
			.iter()
			.position(|r| {
				let r_end = r.start + r.len;
				phys_addr >= r.start && phys_addr < r_end
			})
			.map(|position| RwLockWriteGuard::map(regions, |regions| &mut regions[position]))
	}

	fn is_watchpoint(&self, addr: u64) -> Option<WatchKind> {
		trace!("checking watchpoint for {:#018X}", addr);
		if let Ok(idx) = self.watchpoints.read().binary_search_by(|(start, len, _)| {
			let region_end = start + len - 1;
			if addr > region_end {
				Ordering::Greater
			} else if addr < *start {
				Ordering::Less
			} else {
				Ordering::Equal
			}
		}) {
			let kind = self.watchpoints.read()[idx].2;
			trace!("watchpoint hit at {:#018X} kind {:?}", addr, kind);
			Some(kind)
		} else {
			None
		}
	}
}

macro_rules! impl_mem_read_write {
	($($ty:ty),*$(,)*) => {
		impl Memory {
			paste::paste! {
				$(
				#[allow(dead_code)]
				pub fn [<read_ $ty:snake>](
					&self,
					hart: &mut WhiskerHart,
					effective_addr: u64,
					kind: ReadKind
				) -> Result<$ty, TrapRequestGuaranteed> {
					let phys_addr = self.translate_addr(hart, effective_addr, kind.as_mem_op())?;

					let Some(region) = self.region_for_addr(phys_addr) else {
						let e = match kind {
							ReadKind::Normal | ReadKind::LoadReserved => hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr),
							ReadKind::Instruction => hart.request_trap(TrapIdx::INSTRUCTION_ACCESS_FAULT, phys_addr),
							ReadKind::AMO => hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr),
						};
						return Err(e);
					};
					check_read_access(hart, &*region, phys_addr, kind, core::mem::size_of::<$ty>() as u8)?;

					let ret = match region.kind {
						MemoryKind::MainMemory { ref backing } => {
							let offset = phys_addr - region.start;
							let mut ret = <$ty>::default().to_le_bytes();
							ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<$ty>()]);
							Ok(<$ty>::from_le_bytes(ret))
						}
						MemoryKind::MMIO(kind) => {
							let mut ret = <$ty>::default().to_le_bytes();
							kind.read(hart, phys_addr, ret.as_mut_slice());
							Ok(<$ty>::from_le_bytes(ret))
						}
					};

					// watchpoints should only happen if the read actually happens
					// but should use the virtual address rather than physical
					if !hart.debug && let Some(watch_kind) = self.is_watchpoint(effective_addr) {
						if matches!(watch_kind, WatchKind::Read | WatchKind::ReadWrite) {
							hart.request_watchpoint(watch_kind, effective_addr);
						}
					}
					ret
				}

				#[allow(dead_code)]
				pub fn [<write_ $ty:snake>](
					&self,
					hart: &mut WhiskerHart,
					effective_addr: u64,
					kind: WriteKind,
					val: $ty,
				) -> Result<(), TrapRequestGuaranteed> {
					let phys_addr = self.translate_addr(hart, effective_addr, MemoryOpKind::Store)?;

					let Some(mut region_guard) = self.region_for_addr_mut(phys_addr) else {
						// FIXME: these are the same, is this always the case? should this be inline?
						let e = match kind {
							WriteKind::Normal => hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr),
							WriteKind::Atomic => hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr),
						};
						return Err(e);
					};
					let region = &mut *region_guard;
					check_write_access(hart, &*region, phys_addr, kind, core::mem::size_of::<$ty>() as u8)?;

					let ret = match region.kind {
						MemoryKind::MainMemory { ref mut backing } => {
							let offset = phys_addr - region.start;
							let bytes = val.to_le_bytes();
							backing[offset as usize..][..core::mem::size_of::<$ty>()].copy_from_slice(&bytes);
							Ok(())
						}
						MemoryKind::MMIO(kind) => {
							let bytes = val.to_le_bytes();
							kind.write(hart, phys_addr, bytes.as_slice());
							Ok(())
						}
					};

					// watchpoints should only happen if the write actually happens
					// but should use the virtual address rather than physical
					if !hart.debug && let Some(watch_kind) = self.is_watchpoint(effective_addr) {
						if matches!(watch_kind, WatchKind::Write | WatchKind::ReadWrite) {
							hart.request_watchpoint(watch_kind, effective_addr);
						}
					}

					drop(region_guard);

					self.reservations.write().unreserve_addr_other_harts(hart.hart_id(), phys_addr);
					ret
				}
				)*
			}
		}
	};
}

impl_mem_read_write!(u8, u16, u32, u64, SoftFloat, SoftDouble);

impl Memory {
	// FIXME: is this really a phys addr??? i dont think so
	pub fn load_reserved_word(&self, hart: &mut WhiskerHart, phys_addr: u64) -> Result<u32, TrapRequestGuaranteed> {
		let Some(region) = self.region_for_addr(phys_addr) else {
			return Err(hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
		};
		check_read_access(
			hart,
			&region,
			phys_addr,
			ReadKind::LoadReserved,
			core::mem::size_of::<u32>() as u8,
		)?;

		let ret = match region.kind {
			MemoryKind::MainMemory { ref backing } => {
				let offset = phys_addr - region.start;
				let mut ret = u32::default().to_le_bytes();
				ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<u32>()]);
				Ok(u32::from_le_bytes(ret))
			}
			MemoryKind::MMIO(kind) => {
				let mut ret = u32::default().to_le_bytes();
				kind.read(hart, phys_addr, ret.as_mut_slice());
				Ok(u32::from_le_bytes(ret))
			}
		};

		if !hart.debug
			&& let Some(watch_kind) = self.is_watchpoint(phys_addr)
		{
			if matches!(watch_kind, WatchKind::Read | WatchKind::ReadWrite) {
				hart.request_watchpoint(watch_kind, phys_addr);
			}
		}

		self.reservations.write().reserve(hart.hart_id(), phys_addr);
		ret
	}

	// FIXME: is this really a phys addr??? i dont think so
	pub fn load_reserved_dword(&self, hart: &mut WhiskerHart, phys_addr: u64) -> Result<u64, TrapRequestGuaranteed> {
		let Some(region) = self.region_for_addr(phys_addr) else {
			return Err(hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
		};
		check_read_access(
			hart,
			&region,
			phys_addr,
			ReadKind::LoadReserved,
			core::mem::size_of::<u64>() as u8,
		)?;

		let ret = match region.kind {
			MemoryKind::MainMemory { ref backing } => {
				let offset = phys_addr - region.start;
				let mut ret = u64::default().to_le_bytes();
				ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<u64>()]);
				Ok(u64::from_le_bytes(ret))
			}
			MemoryKind::MMIO(kind) => {
				let mut ret = u64::default().to_le_bytes();
				kind.read(hart, phys_addr, ret.as_mut_slice());
				Ok(u64::from_le_bytes(ret))
			}
		};

		if !hart.debug
			&& let Some(watch_kind) = self.is_watchpoint(phys_addr)
		{
			if matches!(watch_kind, WatchKind::Read | WatchKind::ReadWrite) {
				hart.request_watchpoint(watch_kind, phys_addr);
			}
		}

		self.reservations.write().reserve(hart.hart_id(), phys_addr);
		ret
	}

	// FIXME: i dont think this is actually meant to be a phys addr
	/// returns Ok(true) if the store succeeded, Ok(false) if the address was not reserved,
	/// and Err if a trap occurred while storing
	pub fn store_conditional_word(
		&self,
		hart: &mut WhiskerHart,
		phys_addr: u64,
		val: u32,
	) -> Result<bool, TrapRequestGuaranteed> {
		let is_reserved = self.reservations.read().is_reserved_by_hart(phys_addr, hart.hart_id());

		let Some(mut region_guard) = self.region_for_addr_mut(phys_addr) else {
			return Err(hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
		};
		let region = &mut *region_guard;
		check_write_access(
			hart,
			region,
			phys_addr,
			WriteKind::Atomic,
			core::mem::size_of::<u32>() as u8,
		)?;

		let ret = if is_reserved {
			match region.kind {
				MemoryKind::MainMemory { ref mut backing } => {
					let offset = phys_addr - region.start;
					let bytes = val.to_le_bytes();
					backing[offset as usize..][..core::mem::size_of::<u32>()].copy_from_slice(&bytes);
				}
				MemoryKind::MMIO(kind) => {
					let bytes = val.to_le_bytes();
					kind.write(hart, phys_addr, bytes.as_slice());
				}
			}

			drop(region_guard);
			// writing unreserves the address written to
			self.reservations
				.write()
				.unreserve_addr_other_harts(hart.hart_id(), phys_addr);

			// unreservation for the current hart happens whenever a SC is executed, whether or not it succeeds to store
			self.reservations.write().unreserve_hart(hart.hart_id());
			Ok(true)
		} else {
			drop(region_guard);
			// unreservation for the current hart happens whenever a SC is executed, whether or not it succeeds to store
			self.reservations.write().unreserve_hart(hart.hart_id());
			Ok(false)
		};

		if !hart.debug
			&& let Some(watch_kind) = self.is_watchpoint(phys_addr)
		{
			if matches!(watch_kind, WatchKind::Write | WatchKind::ReadWrite) {
				hart.request_watchpoint(watch_kind, phys_addr);
			}
		}

		ret
	}

	// FIXME: i dont think this is actually a phys addr
	/// returns Ok(true) if the store succeeded, Ok(false) if the address was not reserved,
	/// and Err if a trap occurred while storing
	pub fn store_conditional_dword(
		&self,
		hart: &mut WhiskerHart,
		phys_addr: u64,
		val: u64,
	) -> Result<bool, TrapRequestGuaranteed> {
		let is_reserved = self.reservations.read().is_reserved_by_hart(phys_addr, hart.hart_id());

		let Some(mut region_guard) = self.region_for_addr_mut(phys_addr) else {
			return Err(hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
		};
		let region = &mut *region_guard;
		check_write_access(
			hart,
			region,
			phys_addr,
			WriteKind::Atomic,
			core::mem::size_of::<u64>() as u8,
		)?;

		let ret = if is_reserved {
			match region.kind {
				MemoryKind::MainMemory { ref mut backing } => {
					let offset = phys_addr - region.start;
					let bytes = val.to_le_bytes();
					backing[offset as usize..][..core::mem::size_of::<u64>()].copy_from_slice(&bytes);
				}
				MemoryKind::MMIO(kind) => {
					let bytes = val.to_le_bytes();
					kind.write(hart, phys_addr, bytes.as_slice());
				}
			}

			drop(region_guard);

			// writing unreserves the address written to
			self.reservations
				.write()
				.unreserve_addr_other_harts(hart.hart_id(), phys_addr);
			// unreservation for the current hart happens whenever a SC is executed, whether or not it succeeds to store
			self.reservations.write().unreserve_hart(hart.hart_id());
			Ok(true)
		} else {
			drop(region_guard);
			// unreservation for the current hart happens whenever a SC is executed, whether or not it succeeds to store
			self.reservations.write().unreserve_hart(hart.hart_id());
			Ok(false)
		};

		if !hart.debug
			&& let Some(watch_kind) = self.is_watchpoint(phys_addr)
		{
			if matches!(watch_kind, WatchKind::Read | WatchKind::ReadWrite) {
				hart.request_watchpoint(watch_kind, phys_addr);
			}
		}

		ret
	}

	pub fn atomic_op_word<F: FnOnce(&mut WhiskerHart, u32) -> Option<u32>>(
		&self,
		hart: &mut WhiskerHart,
		virt_addr: u64,
		op: F,
	) -> Result<u32, TrapRequestGuaranteed> {
		// FIXME: maybe actually do atomics here
		let word = self.read_u32(hart, virt_addr, ReadKind::AMO)?;
		if let Some(replacement) = op(hart, word) {
			self.write_u32(hart, virt_addr, WriteKind::Atomic, replacement)?;
		}
		Ok(word)
	}

	pub fn atomic_op_dword<F: FnOnce(&mut WhiskerHart, u64) -> Option<u64>>(
		&self,
		hart: &mut WhiskerHart,
		virt_addr: u64,
		op: F,
	) -> Result<u64, TrapRequestGuaranteed> {
		// FIXME: maybe actually do atomics here
		let dword = self.read_u64(hart, virt_addr, ReadKind::AMO)?;
		if let Some(replacement) = op(hart, dword) {
			self.write_u64(hart, virt_addr, WriteKind::Atomic, replacement)?;
		}
		Ok(dword)
	}
}

impl Memory {
	fn read_pte(
		&self,
		hart: &mut WhiskerHart,
		phys_addr: u64,
		kind: MemoryOpKind,
	) -> Result<u64, TrapRequestGuaranteed> {
		let Some(region) = self.region_for_addr(phys_addr) else {
			// FIXME: use effective addr
			return Err(pte_fault(hart, kind, phys_addr));
		};

		let access_kinds = region.attrs.access_kinds;
		let max_size = region.attrs.max_size;
		let size = core::mem::size_of::<u64>() as u8;
		if !access_kinds.contains(AccessKind::READ) || size > max_size {
			trace!("PTE access not in read region: {:?} at {:#018X}", region, phys_addr);
			// FIXME: use effective addr
			return Err(pte_fault(hart, kind, phys_addr));
		}
		// FIXME: can this be removed?
		if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
			trace!(
				"PTE access not in misaligned region: {:?} at {:#018X}",
				region,
				phys_addr
			);
			// FIXME: use effective addr
			return Err(pte_fault(hart, kind, phys_addr));
		}

		let ret = match region.kind {
			MemoryKind::MainMemory { ref backing } => {
				let offset = phys_addr - region.start;
				let mut ret = u64::default().to_le_bytes();
				ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<u64>()]);
				Ok(u64::from_le_bytes(ret))
			}
			MemoryKind::MMIO(kind) => {
				let mut ret = <u64>::default().to_le_bytes();
				kind.read(hart, phys_addr, ret.as_mut_slice());
				Ok(<u64>::from_le_bytes(ret))
			}
		};

		if !hart.debug
			&& let Some(watch_kind) = self.is_watchpoint(phys_addr)
		{
			if matches!(watch_kind, WatchKind::Read | WatchKind::ReadWrite) {
				hart.request_watchpoint(watch_kind, phys_addr);
			}
		}

		ret
	}
}

fn pte_fault(hart: &mut WhiskerHart, kind: MemoryOpKind, effective_addr: u64) -> TrapRequestGuaranteed {
	match kind {
		MemoryOpKind::Instruction => hart.request_trap(TrapIdx::INSTRUCTION_ACCESS_FAULT, effective_addr),
		MemoryOpKind::Load => hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, effective_addr),
		MemoryOpKind::Store => hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, effective_addr),
	}
}

macro_rules! impl_hw_read_write {
	($($ty:ty),*$(,)*) => {
		impl Memory {
			paste::paste! {$(
				#[allow(dead_code)]
				/// reads from an address as if the read was not done by a hart, instead by other hardware.
				/// returns Err(()) if the access could not be performed.
				pub fn [<read_hw_ $ty:snake>](
					&self,
					phys_addr: u64,
				) -> Result<$ty, ()> {
					let Some(region) = self.region_for_addr(phys_addr) else {
						return Err(());
					};

					let access_kinds = region.attrs.access_kinds;
					let max_size = region.attrs.max_size;
					let size = core::mem::size_of::<$ty>() as u8;
					if !access_kinds.contains(AccessKind::READ) {
						trace!("HW access not in read region: {:?} at {:#018X}", region, phys_addr);
						return Err(());
					}
					if size > max_size {
						trace!("HW access of size {:?} greater than region max: {:?}", size, region);
						return Err(());
					}
					if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
						trace!(
							"HW access for size {:?} not in misaligned region: {:?} at {:#018X}",
							size,
							region,
							phys_addr
						);
						return Err(());
					}

					match region.kind {
						MemoryKind::MainMemory { ref backing } => {
							let offset = phys_addr - region.start;
							let mut ret = <$ty>::default().to_le_bytes();
							ret.copy_from_slice(&backing[offset as usize..][..core::mem::size_of::<$ty>()]);
							Ok(<$ty>::from_le_bytes(ret))
						}
						MemoryKind::MMIO(_kind) => {
							// FIXME: can this be relaxed?
							error!("HW mem ops cannot interact with MMIO");
							Err(())
						}
					}
				}

				#[allow(dead_code)]
				/// writes to an address as if the write was not done by a hart, instead by other hardware.
				/// returns Err(()) if the access could not be performed.
				pub fn [<write_hw_ $ty:snake>](
					&self,
					phys_addr: u64,
					val: $ty,
				) -> Result<(), ()> {
					let Some(mut region_guard) = self.region_for_addr_mut(phys_addr) else {
						return Err(());
					};
					let region = &mut *region_guard;

					let access_kinds = region.attrs.access_kinds;
					let max_size = region.attrs.max_size;
					let size = core::mem::size_of::<$ty>() as u8;
					if !access_kinds.contains(AccessKind::WRITE) {
						trace!("HW access not in write region: {:?} at {:#018X}", region, phys_addr);
						return Err(());
					}
					if size > max_size {
						trace!("HW access of size {:?} greater than region max: {:?}", size, region);
						return Err(());
					}
					if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
						trace!(
							"HW access for size {:?} not in misaligned region: {:?} at {:#018X}",
							size,
							region,
							phys_addr
						);
						return Err(());
					}

					match region.kind {
						MemoryKind::MainMemory { ref mut backing } => {
							let offset = phys_addr - region.start;
							let bytes = val.to_le_bytes();
							backing[offset as usize..][..core::mem::size_of::<$ty>()].copy_from_slice(&bytes);
							Ok(())
						}
						MemoryKind::MMIO(_kind) => {
							// FIXME: can this be relaxed?
							error!("HW mem ops cannot interact with MMIO");
							Err(())
						}
					}
				}
			)*}
		}
	};
}

impl_hw_read_write!(u8, u16, u32, u64);

#[derive(Debug)]
struct MemoryReservations {
	/// map of hart ID to reservation base address
	/// reservation base addresses are aligned to [`MemoryReservations::RESERVATION_SET_SIZE`]
	reservations: Vec<Option<u64>>,
}

impl Default for MemoryReservations {
	fn default() -> Self {
		Self::new()
	}
}

impl MemoryReservations {
	// MUST be a power of 2
	const RESERVATION_SET_SIZE: u64 = 64;

	pub fn new() -> Self {
		Self {
			reservations: vec![None; HartId::MAX_NUM_HARTS.extend::<usize>()],
		}
	}

	/// sets the reservation for the the hart specified by `hart_id` to be `phys_addr`
	fn reserve(&mut self, hart_id: HartId, phys_addr: u64) {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations[hart_id.inner().extend::<usize>()] = Some(aligned_addr);
	}

	/// unreserves `phys_addr` for all harts *other* than the hart specified by `hart_id`
	fn unreserve_addr_other_harts(&mut self, hart_id: HartId, phys_addr: u64) {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations.iter_mut().enumerate().for_each(|(idx, addr)| {
			if idx != hart_id.inner().extend() && addr.is_some_and(|a| a == aligned_addr) {
				*addr = None;
			}
		});
	}

	/// removes the reservation from the hart specified by `hart_id`, if any exist
	fn unreserve_hart(&mut self, hart_id: HartId) {
		self.reservations[hart_id.inner().extend::<usize>()] = None;
	}

	fn is_reserved_by_hart(&self, phys_addr: u64, hart_id: HartId) -> bool {
		let aligned_addr = phys_addr & !(Self::RESERVATION_SET_SIZE - 1);
		self.reservations[hart_id.inner().extend::<usize>()].is_some_and(|addr| addr == aligned_addr)
	}
}

/// checks that a read of `size` from `phys_addr` is allowed in `region`
fn check_read_access(
	hart: &mut WhiskerHart,
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
				return Err(hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(hart.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, phys_addr));
			}
		}
		ReadKind::Instruction => {
			if !access_kinds.contains(AccessKind::EXEC) || size > max_size {
				return Err(hart.request_trap(TrapIdx::INSTRUCTION_ACCESS_FAULT, phys_addr));
			}
			// NOTE: instruction misaligned traps are generated on control flow, not when fetching
			debug_assert!(
				access_kinds.contains(AccessKind::MISALIGNED) || phys_addr % u64::from(size) == 0,
				"tried to fetch misaligned instruction (THIS SHOULD NEVER HAPPEN)"
			);
		}
		ReadKind::LoadReserved => {
			if !access_kinds.contains(AccessKind::READ | AccessKind::ATOMIC) || size > max_size {
				return Err(hart.request_trap(TrapIdx::LOAD_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(hart.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, phys_addr));
			}
		}
		ReadKind::AMO => {
			if !access_kinds.contains(AccessKind::READ | AccessKind::WRITE | AccessKind::ATOMIC) || size > max_size {
				return Err(hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(hart.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, phys_addr));
			}
		}
	}

	Ok(())
}

/// checks that a write of `size` from `phys_addr` is allowed in `region`
fn check_write_access(
	hart: &mut WhiskerHart,
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
				return Err(hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(hart.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, phys_addr));
			}
		}
		WriteKind::Atomic => {
			if !access_kinds.contains(AccessKind::WRITE | AccessKind::ATOMIC) || size > max_size {
				return Err(hart.request_trap(TrapIdx::STORE_ACCESS_FAULT, phys_addr));
			}
			if !access_kinds.contains(AccessKind::MISALIGNED) && phys_addr % u64::from(size) != 0 {
				return Err(hart.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, phys_addr));
			}
		}
	}

	Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryOpKind {
	Instruction,
	Load,
	Store,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadKind {
	/// reading memory for normal accesses
	Normal,
	/// reading memory as part of instructon fetching
	Instruction,
	/// reads for LR
	LoadReserved,
	/// reads that are part of an AMO (which should also be considered to be a write)
	AMO,
}

impl ReadKind {
	pub fn as_mem_op(self) -> MemoryOpKind {
		match self {
			ReadKind::Normal | ReadKind::LoadReserved => MemoryOpKind::Load,
			ReadKind::Instruction => MemoryOpKind::Instruction,
			ReadKind::AMO => MemoryOpKind::Store,
		}
	}
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
				"memory region {:?} overlapped with {:?}",
				region,
				existing
			);
		}

		self.regions.push(region);
		self.regions.sort_unstable_by_key(|r| r.start);

		self
	}

	pub fn build(self) -> Memory {
		Memory {
			regions: RwLock::new(self.regions),
			reservations: RwLock::new(MemoryReservations::default()),
			page_table_cache: RwLock::new(BTreeMap::default()),
			watchpoints: RwLock::new(Vec::new()),
		}
	}
}
