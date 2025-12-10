#![allow(
	clippy::match_same_arms,
	reason = "ordering the register offsets makes intent match docs"
)]

use std::{
	fs::{File, OpenOptions},
	io::{Read, Seek, SeekFrom, Write},
	path::Path,
	sync::{
		Arc, OnceLock,
		mpsc::{self, Receiver, Sender},
	},
	thread,
};

use bitflags::bitflags;
use bytemuck::{Pod, Zeroable, bytes_of, bytes_of_mut, from_bytes};
use num_conv::prelude::*;
use spin::Mutex;

use crate::{
	cpu::hart::WhiskerHart,
	interrupts::{InterruptMessage, InterruptSource},
	mem::{Memory, mmio::MMIODevice},
	tracing::*,
	virtio::{InterruptStatus, VirtQueue, VirtioDeviceStatus, VirtioFeatures},
};

pub const VIRTIO_BLOCK_BASE: u64 = 0x10001000;

const SUPPORTED_FEATURES: u128 = VirtioFeatures::SPEC_VERSION_1.bits() | VirtioBlockDeviceFeatures::empty().bits();

#[derive(Debug)]
struct Command {
	queue: u16,
}

#[derive(Debug, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct BlockDevConfig {
	// capacity in 512 byte sectors of the device
	capacity: u64,
}

impl BlockDevConfig {
	fn new() -> Self {
		Self { capacity: 1024 }
	}
}

pub struct VirtioBlockDevice {
	interrupt_tx: Sender<InterruptMessage>,
	thread_tx: Sender<Command>,
	queues: Vec<VirtQueue>,
	driver_features: u128,
	status: VirtioDeviceStatus,
	device_features_select: u16,
	driver_features_select: u16,
	queue_select: u16,
	interrupt_status: InterruptStatus,
	/// the "generation" of the config value, changes every time the configuration changes
	/// used for "atomicity" of the config space
	config_gen: u32,
	config: BlockDevConfig,
}

impl VirtioBlockDevice {
	pub fn init(mem: Arc<Memory>, fs_img: &Path, interrupt_tx: Sender<InterruptMessage>) -> Arc<Mutex<Self>> {
		FS_IMG.get_or_init(|| OpenOptions::new().read(true).write(true).open(fs_img).unwrap());

		let (thread_tx, thread_rx) = mpsc::channel();

		let this = Arc::new(Mutex::new(Self {
			interrupt_tx,
			thread_tx,
			driver_features: 0,
			status: VirtioDeviceStatus::empty(),
			device_features_select: 0,
			driver_features_select: 0,
			queue_select: 0,
			queues: vec![VirtQueue::new(); 1],
			interrupt_status: InterruptStatus::empty(),
			config_gen: 0,
			config: BlockDevConfig::new(),
		}));

		thread::spawn({
			let virtio = Arc::clone(&this);
			move || start_block_device(&mem, &virtio, &thread_rx)
		});

		this
	}
}

impl MMIODevice for VirtioBlockDevice {
	fn read(&mut self, hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		use offsets::*;

		let Ok(out) = bytemuck::try_from_bytes_mut::<u32>(buf) else {
			error!("virtio block device only supports reads of u32");
			return;
		};

		let offset = addr.wrapping_sub(VIRTIO_BLOCK_BASE);
		match offset {
			MAGIC_VAL => *out = 0x74726976, //"virt" in little endian
			VERSION => *out = 2,
			DEVICE_ID => *out = 2,          // block device
			VENDOR_ID => *out = 0x554d4551, // "QEMU"
			DEVICE_SUPPORTED_FEATURES => *out = self.read_device_features(),
			DEVICE_FEATURES_SELECT | DRIVER_FEATURES | DRIVER_FEATURES_SELECT | VIRTQUEUE_SELECT => {} // WRITE ONLY
			QUEUE_SIZE_MAX => *out = self.selected_queue_mut().max_size().extend::<u32>(),
			QUEUE_SIZE => {} // WRITE ONLY
			QUEUE_READY => *out = u32::from(self.selected_queue_mut().ready),
			QUEUE_NOTIFY => {} // WRITE ONLY
			INTERRUPT_STATUS => *out = self.interrupt_status.bits().extend::<u32>(),
			INTERRUPT_ACK => {} // WRITE_ONLY
			STATUS => *out = self.status.bits(),
			QUEUE_DESC_LOW | QUEUE_DESC_HIGH | QUEUE_AVAIL_LOW | QUEUE_AVAIL_HIGH | QUEUE_USED_LOW
			| QUEUE_USED_HIGH | SHM_SELECT => {} // WRITE ONLY
			SHM_LENGTH_LOW | SHM_LENGTH_HIGH | SHM_BASE_LOW | SHM_BASE_HIGH => todo!("shared memory"),
			QUEUE_RESET => todo!("queue reset"),
			CONFIG_GENERATION => *out = self.config_gen,
			CONFIG_BASE.. => *out = self.read_config(offset - CONFIG_BASE),
			_ => {
				error!(
					"hart {:?} tried to reod from unknown offset {:#018X}",
					hart.hart_id(),
					offset
				);
			}
		}

		trace!("virtio r {:#018X} => {:#010X}", addr, out);
	}

	fn write(&mut self, hart: &mut WhiskerHart, addr: u64, val: &[u8]) {
		use offsets::*;

		let val = *bytemuck::from_bytes::<u32>(val);
		trace!("virtio w {:#018X} <= {:#010X}", addr, val);
		let offset = addr.wrapping_sub(VIRTIO_BLOCK_BASE);
		match offset {
			MAGIC_VAL | VERSION | DEVICE_ID | VENDOR_ID | DEVICE_SUPPORTED_FEATURES => {} // READ ONLY
			DEVICE_FEATURES_SELECT => {
				if val > 3 {
					error!("virtio device features select > 3 not supported: {}", val);
				} else {
					self.device_features_select = val.truncate::<u16>();
				}
			}
			DRIVER_FEATURES => self.set_driver_features(val),
			DRIVER_FEATURES_SELECT => {
				if val > 3 {
					error!("virtio driver features select > 3 not supported: {}", val);
				} else {
					self.driver_features_select = val.truncate::<u16>();
				}
			}
			VIRTQUEUE_SELECT => {
				if val != 0 {
					todo!("virtqueue select != 0");
				}
				self.queue_select = val.truncate::<u16>();
			}
			QUEUE_SIZE_MAX => {} // READ ONLY
			QUEUE_SIZE => self.selected_queue_mut().set_size(val.truncate::<u16>()),
			QUEUE_READY => self.selected_queue_mut().ready = val == 0x01,
			QUEUE_NOTIFY => self.notify_queue(val),
			INTERRUPT_STATUS => {} // READ ONLY
			INTERRUPT_ACK => self.interrupt_ack(val),
			STATUS => self.set_status(hart, val),
			QUEUE_DESC_LOW => self.set_desc_low(val),
			QUEUE_DESC_HIGH => self.set_desc_high(val),
			QUEUE_AVAIL_LOW => self.set_avail_low(val),
			QUEUE_AVAIL_HIGH => self.set_avail_high(val),
			QUEUE_USED_LOW => self.set_used_low(val),
			QUEUE_USED_HIGH => self.set_used_high(val),
			SHM_SELECT | SHM_LENGTH_LOW | SHM_LENGTH_HIGH | SHM_BASE_LOW | SHM_BASE_HIGH => todo!("shared memory"),
			QUEUE_RESET => todo!("queue reset"),
			CONFIG_GENERATION => {} // READ ONLY
			CONFIG_BASE => self.write_config(offset - CONFIG_BASE, val),
			_ => {
				error!(
					"hart {:?} tried to write to unknown offset {:#018X}",
					hart.hart_id(),
					offset
				);
			}
		}
	}
}

impl VirtioBlockDevice {
	fn selected_queue_mut(&mut self) -> &mut VirtQueue {
		&mut self.queues[self.queue_select.extend::<usize>()]
	}

	fn read_device_features(&self) -> u32 {
		let shift = self.device_features_select * 32;
		debug_assert!(shift < 128);

		(SUPPORTED_FEATURES >> shift).truncate::<u32>()
	}

	fn set_driver_features(&mut self, val: u32) {
		let shift = self.driver_features_select * 32;
		debug_assert!(shift < 128);

		let bits = u128::from(val) << shift;
		// dont let the driver set anything we don't support
		// FIXME: set error if the driver tried to
		let bits = SUPPORTED_FEATURES & bits;

		let mask: u128 = !(0xFF_u128 << shift);
		self.driver_features &= mask;
		self.driver_features |= bits;
	}

	fn set_status(&mut self, hart: &mut WhiskerHart, val: u32) {
		if val == 0 {
			error!("TODO: DEVICE RESET");
		}

		self.status = VirtioDeviceStatus::from_bits_retain(val);
		//		self.interrupt_status.set(InterruptStatus::CONFIG_CHANGE, true);
		//		self.interrupt_tx
		//			.send(InterruptMessage::new_high(InterruptSource::VIRTIO))
		//			.unwrap();
	}

	fn set_desc_low(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.descriptor_table & 0xFFFFFFFF_00000000 | val.extend::<u64>();
		queue.descriptor_table = val;
	}
	fn set_desc_high(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.descriptor_table & 0x00000000_FFFFFFFF | (val.extend::<u64>() << 32);
		queue.descriptor_table = val;
	}

	fn set_avail_low(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.avail_ring & 0xFFFFFFFF_00000000 | val.extend::<u64>();
		queue.avail_ring = val;
	}
	fn set_avail_high(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.avail_ring & 0x00000000_FFFFFFFF | (val.extend::<u64>() << 32);
		queue.avail_ring = val;
	}

	fn set_used_low(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.used_ring & 0xFFFFFFFF_00000000 | val.extend::<u64>();
		queue.used_ring = val;
	}
	fn set_used_high(&mut self, val: u32) {
		let queue = self.selected_queue_mut();
		let val = queue.used_ring & 0x00000000_FFFFFFFF | (val.extend::<u64>() << 32);
		queue.used_ring = val;
	}

	fn interrupt_ack(&mut self, val: u32) {
		let cleared = InterruptStatus::from_bits_truncate(val.truncate::<u8>());
		self.interrupt_status = self.interrupt_status.difference(cleared);
		self.interrupt_tx
			.send(InterruptMessage::new_low(InterruptSource::VIRTIO))
			.unwrap();
	}

	fn notify_queue(&mut self, val: u32) {
		if VirtioFeatures::from_bits_retain(self.driver_features).contains(VirtioFeatures::NOTIFICATION_DATA) {
			todo!("virtio notification data")
		} else {
			let queue = val.truncate::<u16>();
			self.thread_tx.send(Command { queue }).unwrap();
		}
	}

	fn read_config(&self, offset: u64) -> u32 {
		let bytes = bytes_of(&self.config);
		let offset = offset as usize;
		// wrapping is not allowed and would be out of bounds anyway
		let Some(end) = offset.checked_add(4) else { return 0 };
		// out of bounds reads should just return 0
		let Some(val) = bytes.get(offset..end) else { return 0 };
		// convert back to u32
		// TODO: make sure the optimizer can see that this never panics because offset..end has len 4
		*from_bytes::<u32>(val)
	}

	fn write_config(&mut self, offset: u64, val: u32) {
		let bytes = bytes_of_mut(&mut self.config);
		let offset = offset as usize;
		// wrapping is not allowed and would be out of bounds anyway
		let Some(end) = offset.checked_add(4) else { return };
		// out of bounds reads should just return
		let Some(dst) = bytes.get_mut(offset..end) else { return };
		// TODO: make sure the optimizer can see that this never panics because offset..end has len 4
		dst.copy_from_slice(&val.to_le_bytes());

		self.config_gen = self.config_gen.wrapping_add(1);
	}
}

fn start_block_device(mem: &Memory, virt_blk: &Arc<Mutex<VirtioBlockDevice>>, command_rx: &Receiver<Command>) {
	'main: loop {
		let command = command_rx.recv().unwrap();
		trace!("block device thread cmd: {:?}", command);
		let queue_idx = command.queue.extend::<usize>();
		// loop over this queue until there's no more data available
		loop {
			let mut virtio = virt_blk.lock();
			let queue = &mut virtio.queues[queue_idx];
			let Some((mut descriptors, head_idx)) = queue.next_avail(&mem) else {
				trace!("no more descriptor chains ready");
				break;
			};
			trace!("descriptor head_idx: {}", head_idx);

			let Some(first) = descriptors.next(&mem) else {
				error!("descriptor chain {:#?} missing first descriptor?", descriptors);
				continue 'main;
			};
			trace!("first: {:#?}", first);

			if !first.is_device_readable() {
				error!("first descriptor in chain not readable: {:?}", first);
				continue 'main;
			}

			// FIXME: maybe support different layouts of descriptors

			let Some(header) = BlockRequestHeader::read_from_mem(&mem, first.addr()) else {
				warn!("failed to read header from mem");
				continue 'main;
			};

			trace!("header: {:?}", header);

			let Some(buf_desc) = descriptors.next(&mem) else {
				error!("virtio block request missing buf descriptor");
				continue 'main;
			};
			trace!("next: {:#?}", buf_desc);

			let is_read = header.kind == BlockRequestKind::In;
			if is_read && !buf_desc.is_device_writeable() {
				error!("virtio blk buffer is not writeable for disk read");
				continue 'main;
			}
			if !is_read && !buf_desc.is_device_readable() {
				error!("virtio blk buffer not readable for disk write");
				continue 'main;
			}

			let buf_addr = buf_desc.addr();
			let buf_len = buf_desc.len();

			let Some(status_desc) = descriptors.next(&mem) else {
				error!("virtio blk request missing status descriptor");
				continue 'main;
			};
			trace!("status: {:#?}", status_desc);

			while let Some(desc) = descriptors.next(&mem) {
				warn!("ignored descriptor {:#?} because we only expected 3", desc);
			}

			let status_addr = status_desc.addr();

			let req = match header.kind {
				BlockRequestKind::In => BlockRequest::Read {
					sector: header.sector,
					buf_addr,
					status_addr,
					buf_len,
				},
				BlockRequestKind::Out => BlockRequest::Write {
					sector: header.sector,
					buf_addr,
					status_addr,
					buf_len,
				},
			};
			drop(virtio);

			// IT IS CRITICAL THAT THE MMIO LOCK NOT BE HELD HERE
			let used_len = if handle_request(&mem, req).is_ok() {
				trace!("handled request, used len {}", buf_len + 1);
				// wrote all of buf, plus one status byte
				buf_len + 1
			} else {
				trace!("failed to handle request");
				// conservatively say we didnt write anything
				0
			};

			let mut virtio = virt_blk.lock();
			let queue = &mut virtio.queues[queue_idx];
			queue.set_used(&mem, head_idx, used_len);
			virtio.interrupt_status.set(InterruptStatus::USED_BUFFER, true);
			virtio
				.interrupt_tx
				.send(InterruptMessage::new_high(InterruptSource::VIRTIO))
				.unwrap();
		}
	}
}

static FS_IMG: OnceLock<File> = OnceLock::new();

const SECTOR_SIZE: u64 = 512;

const STATUS_OK: u8 = 0;
const STATUS_IOERR: u8 = 1;
#[allow(dead_code, reason = "FIXME: Check status")]
const STATUS_UNSUPP: u8 = 2;

fn handle_request(mem: &Memory, req: BlockRequest) -> Result<(), ()> {
	trace!("handle_req {:#?}", req);
	match req {
		BlockRequest::Read {
			sector,
			buf_addr,
			status_addr,
			buf_len,
		} => {
			let offset = sector * SECTOR_SIZE;

			let mut data = vec![0_u8; buf_len as usize];

			let mut f = FS_IMG.wait();
			f.seek(SeekFrom::Start(offset)).unwrap();
			f.read_exact(&mut data).unwrap();

			for (idx, chunk) in data.chunks_exact(8).enumerate() {
				let val = u64::from_le_bytes(chunk.try_into().unwrap());
				let offset = (idx * 8) as u64;

				if mem.write_hw_u64(buf_addr + offset, val).is_err() {
					let _ = mem.write_hw_u8(status_addr, STATUS_IOERR);
					return Err(());
				}
			}

			if mem.write_hw_u8(status_addr, STATUS_OK).is_err() {
				error!("failed to write to status at {:#018X}", status_addr);
			}
			Ok(())
		}
		BlockRequest::Write {
			sector,
			buf_addr,
			status_addr,
			buf_len,
		} => {
			let offset = sector * SECTOR_SIZE;

			let mut f = FS_IMG.wait();
			f.seek(SeekFrom::Start(offset)).unwrap();

			for idx in 0..u64::from(buf_len / 8) {
				let Ok(val) = mem.read_hw_u64(buf_addr + idx * 8) else {
					error!("virtio blk could not read from buf at {:#018X}[{}]", buf_addr, idx * 8);
					let _ = mem.write_hw_u8(status_addr, STATUS_IOERR);
					return Err(());
				};

				f.write_all(val.to_le_bytes().as_slice()).unwrap();
			}

			let _ = mem.write_hw_u8(status_addr, STATUS_OK);
			Ok(())
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum BlockRequest {
	Read {
		sector: u64,
		buf_addr: u64,
		status_addr: u64,
		buf_len: u32,
	},
	Write {
		sector: u64,
		buf_addr: u64,
		status_addr: u64,
		buf_len: u32,
	},
}

#[derive(Debug)]
/// data in the first descriptor of the chain
struct BlockRequestHeader {
	sector: u64,
	kind: BlockRequestKind,
}

impl BlockRequestHeader {
	fn read_from_mem(mem: &Memory, addr: u64) -> Option<BlockRequestHeader> {
		let Ok(kind) = mem.read_hw_u32(addr) else {
			error!("virtio blk unable to read descriptor at {:#018X}", addr);
			return None;
		};
		let Ok(sector) = mem.read_hw_u64(addr + 8) else {
			error!("virtio blk unable to read descriptor at {:#018X}", addr);
			return None;
		};
		let kind = match kind {
			0 => BlockRequestKind::In,
			1 => BlockRequestKind::Out,
			_ => {
				error!("unsupported virtio blk request kind {}", kind);
				return None;
			}
		};
		Some(BlockRequestHeader { sector, kind })
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BlockRequestKind {
	In,
	Out,
}

bitflags! {
	#[derive(Debug)]
	pub struct VirtioBlockDeviceFeatures: u128 {
		const SIZE_MAX = 1 << 1;
		const SEGMENT_MAX = 1 << 2;
		const GEOMETRY = 1 << 4;
		const READ_ONLY = 1 << 5;
		const BLOCK_SIZE = 1 << 6;
		const FLUSH = 1 << 9;
		const TOPOLOGY = 1 << 10;
		const CONFIG_WCE = 1 << 11;
		const MULTIQUEUE = 1 << 12;
		const DISCARD = 1 << 13;
		const WRITE_ZEROS = 1 << 14;
		const LIFETIME = 1 << 15;
		const SECURE_ERASE = 1 << 16;
		const ZONED = 1 << 17;
	}
}

mod offsets {
	pub const MAGIC_VAL: u64 = 0x000;
	pub const VERSION: u64 = 0x004;
	pub const DEVICE_ID: u64 = 0x008;
	pub const VENDOR_ID: u64 = 0x00C;
	pub const DEVICE_SUPPORTED_FEATURES: u64 = 0x010;
	pub const DEVICE_FEATURES_SELECT: u64 = 0x014;
	pub const DRIVER_FEATURES: u64 = 0x020;
	pub const DRIVER_FEATURES_SELECT: u64 = 0x024;
	pub const VIRTQUEUE_SELECT: u64 = 0x030;
	pub const QUEUE_SIZE_MAX: u64 = 0x034;
	pub const QUEUE_SIZE: u64 = 0x038;
	pub const QUEUE_READY: u64 = 0x044;
	pub const QUEUE_NOTIFY: u64 = 0x050;
	pub const INTERRUPT_STATUS: u64 = 0x060;
	pub const INTERRUPT_ACK: u64 = 0x064;
	pub const STATUS: u64 = 0x070;
	pub const QUEUE_DESC_LOW: u64 = 0x080;
	pub const QUEUE_DESC_HIGH: u64 = 0x084;
	pub const QUEUE_AVAIL_LOW: u64 = 0x090;
	pub const QUEUE_AVAIL_HIGH: u64 = 0x094;
	pub const QUEUE_USED_LOW: u64 = 0x0A0;
	pub const QUEUE_USED_HIGH: u64 = 0x0A4;
	pub const SHM_SELECT: u64 = 0x0AC;
	pub const SHM_LENGTH_LOW: u64 = 0x0B0;
	pub const SHM_LENGTH_HIGH: u64 = 0x0B4;
	pub const SHM_BASE_LOW: u64 = 0x0B8;
	pub const SHM_BASE_HIGH: u64 = 0x0BC;
	pub const QUEUE_RESET: u64 = 0xC0;
	pub const CONFIG_GENERATION: u64 = 0xFC;
	pub const CONFIG_BASE: u64 = 0x100;
}
