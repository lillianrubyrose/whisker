#![allow(dead_code, reason = "FIXME: Finish implementation. meowmeow")]

use bitflags::bitflags;
use num_conv::Extend;

use crate::{mem::Memory, tracing::*, virtio::descriptor::DescriptorChain};

pub mod descriptor;

pub const MAX_QUEUE_SIZE: u16 = 16;

#[derive(Debug, Clone)]
pub struct VirtQueue {
	pub descriptor_table: u64,
	/// data supplied by the driver
	/// points to a struct of type `virtq_avail`
	pub avail_ring: u64,
	/// data supplied by the device
	pub used_ring: u64,
	/// the size of the queue as negotiated by the driver
	size: u16,
	/// the maximum size of the queue supported by the device
	max_size: u16,
	/// the index of the next entry in the available ring that the device should read
	pub avail_idx: u16,
	pub used_idx: u16,
	pub ready: bool,
}

const VIRTQ_AVAIL_IDX_OFFSET: u64 = 2;
const VIRTQ_AVAIL_RING_OFFSET: u64 = 4;
const VIRTQ_AVAIL_RING_ELEM_SIZE: u64 = core::mem::size_of::<u16>() as u64;

impl VirtQueue {
	pub const MAX_NUM_ELEMS: usize = 32768;

	pub fn new() -> Self {
		Self {
			size: 0,
			max_size: MAX_QUEUE_SIZE,
			descriptor_table: 0,
			avail_ring: 0,
			avail_idx: 0,
			used_ring: 0,
			used_idx: 0,
			ready: false,
		}
	}

	pub fn set_size(&mut self, size: u16) {
		self.size = u16::max(size, self.max_size);
	}

	pub fn size(&self) -> u16 {
		self.size
	}

	pub fn max_size(&self) -> u16 {
		self.max_size
	}

	pub fn next_avail(&mut self, mem: &Memory) -> Option<(DescriptorChain, u16)> {
		// the descriptor table MUST be physically continuous in memory, so wrapping cannot happen
		// for any of these offset calculations.
		// spec v1.3 section 2.7
		// https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html#x1-350007

		// addr of the `idx` field
		let addr = self.avail_ring + VIRTQ_AVAIL_IDX_OFFSET;
		let Ok(avail_next_idx) = mem.read_hw_u16(addr) else {
			error!("virtio could not read virtq_avail.idx at {:#018X}", addr);
			return None;
		};

		// get the next entry in the ring if we have not yet read it
		if self.avail_idx < avail_next_idx {
			let idx = u64::from(self.avail_idx % self.size);
			let addr = self.avail_ring + VIRTQ_AVAIL_RING_OFFSET + (idx * VIRTQ_AVAIL_RING_ELEM_SIZE);
			let Ok(descriptor_start_idx) = mem.read_hw_u16(addr) else {
				error!("virtio could not read virtq_avail.ring[{}] at {:#018X}", idx, addr);
				return None;
			};
			self.avail_idx = self.avail_idx.wrapping_add(1);

			Some((
				DescriptorChain::new(self.descriptor_table, self.size, descriptor_start_idx),
				descriptor_start_idx,
			))
		} else {
			None
		}
	}

	pub fn set_used(&mut self, mem: &Memory, desc_id: u16, used_len: u32) {
		const USED_ELEM_SIZE: u16 = 8;

		trace!("setting used desc {} len {}", desc_id, used_len);

		let offset = (4 + self.used_idx * USED_ELEM_SIZE).extend::<u64>();
		let Ok(()) = mem.write_hw_u32(self.used_ring + offset, desc_id.extend::<u32>()) else {
			error!(
				"virtio blk unable to write to used ring at {:#018X}[{}]",
				self.used_ring, offset
			);
			return;
		};
		let Ok(()) = mem.write_hw_u32(self.used_ring + offset + 4, used_len) else {
			error!(
				"virtio blk unable to write to used ring at {:#018X}[{}]",
				self.used_ring,
				offset + 4
			);
			return;
		};

		self.used_idx += 1;
		let Ok(()) = mem.write_hw_u16(self.used_ring + 2, self.used_idx) else {
			error!(
				"virtio blk unable to write to used idx to used ring at {:#018X}",
				self.used_ring,
			);
			return;
		};

		trace!("set used.idx to {}", self.used_idx);
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct VirtioFeatures: u128 {
		const INDIRECT_DESCRIPTOR = 1<< 28;
		const EVENT_IDX = 1 << 29;
		const SPEC_VERSION_1 = 1 << 32;
		const ACCESS_PLATFORM = 1 << 33;
		const RING_PACKED = 1 << 34;
		const IN_ORDER = 1 << 35;
		const ORDER_PLATFORM = 1 << 36;
		const SINGLE_ROOT_IO_VIRT = 1 << 37;
		const NOTIFICATION_DATA = 1 << 38;
		const NOTIFICATION_CONFIG_DATA = 1 << 39;
		const RING_RESET = 1 << 40;
		const ADMIN_VIRTQUEUE = 1 << 41;
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct VirtioDeviceStatus: u32 {
		const ACKNOWLEDGED = 1 << 0;
		const DRIVER_ACK = 1 << 1;
		const DRIVER_READY = 1 << 2;
		const FEATURES_OK = 1 << 3;
		const DEVICE_NEEDS_RESET = 1 << 6;
		const FAILED = 1 << 7;
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct InterruptStatus: u8 {
		const USED_BUFFER = 1 << 0;
		const CONFIG_CHANGE = 1 << 1;
	}
}

#[derive(Debug)]
struct VirtQueueAvailableRing {
	flags: VirtQueueAvailRingFlags,
	// the index into `ring` where the driver will put the next descriptor
	idx: u16,
	ring: [u16; MAX_QUEUE_SIZE as usize],
	// ignored if flags.NO_INTERRUPT is not set
	used_event: u16,
}

bitflags! {
	#[derive(Debug)]
	pub struct VirtQueueAvailRingFlags : u16 {
		const NO_INTERRUPT = 1 << 0;
	}
}
