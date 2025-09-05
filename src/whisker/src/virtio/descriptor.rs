use bitflags::bitflags;

use crate::tracing::*;
use num_conv::prelude::*;

use crate::mem::Memory;

#[derive(Debug)]
pub struct DescriptorChain {
	descriptor_table: u64,
	/// the length of the queue this descriptor chain is from
	queue_len: u16,
	next_idx: Option<u16>,
}

impl DescriptorChain {
	pub fn new(descriptor_table: u64, queue_len: u16, start_idx: u16) -> Self {
		Self {
			descriptor_table,
			queue_len,
			next_idx: Some(start_idx),
		}
	}

	pub fn next(&mut self, mem: &Memory) -> Option<Descriptor> {
		let idx = self.next_idx?;

		if idx >= self.queue_len {
			return None;
		}

		let offset = idx as u64 * core::mem::size_of::<Descriptor>() as u64;

		// the descriptor table MUST be physically continuous in memory, so wrapping cannot happen
		// spec v1.3 section 2.7
		// https://docs.oasis-open.org/virtio/virtio/v1.3/csd01/virtio-v1.3-csd01.html#x1-350007
		let descriptor_addr = self.descriptor_table + offset;

		let Ok(descriptor) = Descriptor::read_from_mem(mem, descriptor_addr) else {
			error!(
				"virtio descriptor chain failed to read mem at {:#018X}",
				descriptor_addr
			);
			return None;
		};

		if descriptor.has_next() {
			self.next_idx = Some(descriptor.next)
		} else {
			self.next_idx = None;
		}

		Some(descriptor)
	}
}

#[derive(Debug)]
pub struct Descriptor {
	addr: u64,
	len: u32,
	flags: VirtQueueDescriptorFlags,
	next: u16,
}

impl Descriptor {
	/// true if the NEXT flag is set
	/// the `next` field is only meaningful if this is true
	pub fn has_next(&self) -> bool {
		self.flags.contains(VirtQueueDescriptorFlags::NEXT)
	}

	pub fn is_device_readable(&self) -> bool {
		!self.flags.contains(VirtQueueDescriptorFlags::DEVICE_WRITE)
	}

	pub fn is_device_writeable(&self) -> bool {
		self.flags.contains(VirtQueueDescriptorFlags::DEVICE_WRITE)
	}

	pub fn addr(&self) -> u64 {
		self.addr
	}

	pub fn len(&self) -> u32 {
		self.len
	}

	pub fn flags(&self) -> VirtQueueDescriptorFlags {
		self.flags
	}

	fn read_from_mem(mem: &Memory, descriptor_addr: u64) -> Result<Self, ()> {
		let addr = mem.read_hw_u64(descriptor_addr)?;
		let rest = mem.read_hw_u64(descriptor_addr + 8)?;
		trace!("descriptor mem: {:#018X} {:#018X}", addr, rest);
		let len = rest.truncate::<u32>();
		let flags = VirtQueueDescriptorFlags::from_bits_retain((rest >> 32).truncate::<u16>());
		let next = (rest >> (32 + 16)) as u16;
		Ok(Self { addr, len, flags, next })
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct VirtQueueDescriptorFlags : u16 {
		const NEXT = 1 << 0;
		const DEVICE_WRITE = 1 << 1;
		const INDIRECT = 1 << 2;
	}
}
