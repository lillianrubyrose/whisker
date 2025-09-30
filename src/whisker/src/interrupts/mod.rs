use std::sync::{
	Arc,
	mpsc::{self, Receiver, Sender, TryRecvError},
};

use bytemuck::from_bytes_mut;
use num_conv::{Extend, Truncate};
use rustc_hash::FxHashMap;
use spin::Mutex;

use crate::{
	cpu::{WhiskerExecState, hart::WhiskerHart},
	mem::mmio::MMIODevice,
	tracing::*,
	ty::{HartId, TrapIdx},
};

/// INVARIANT: a valid interrupt source in range 1..=1023
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InterruptSource(u16);

impl InterruptSource {
	pub const VIRTIO: Self = Self(1);
	pub const RTC: Self = Self(2);
	pub const UART: Self = Self(10);

	pub fn inner(self) -> u16 {
		self.0
	}
}

pub const PLIC_BASE: u64 = 0x0c00_0000;
pub const PLIC_LEN: u64 = 0x0400_0000;

// this must be a multiple of 32
pub const MAX_IRQ_SOURCES: usize = 64;

#[derive(Debug)]
pub struct PlatformInterruptController {
	interrupt_rx: Receiver<InterruptMessage>,

	/// the priority of interrupt sources.
	/// 0 means never interrupt, priority increases as the numerical value increases
	priorities: [u32; MAX_IRQ_SOURCES],
	/// which interrupt sources are pending
	/// each source has one bit at index `source/32`, bit `source%32`
	pending: [u32; MAX_IRQ_SOURCES / 32],

	/// which interrupt sources have been claimed
	claimed: [u32; MAX_IRQ_SOURCES / 32],

	/// map of context id to info
	context_info: Vec<ContextInfo>,

	/// current state of each interrupt, set when a message comes in, and cleared upon completion
	interrupt_source_states: FxHashMap<InterruptSource, bool>,
}

/// turns an IRQ number into an index and bit index
fn irq_to_idx(irq: u16) -> (usize, u32) {
	((irq / 32) as usize, (irq % 32) as u32)
}

impl PlatformInterruptController {
	pub fn new(num_harts: u16) -> (Sender<InterruptMessage>, Arc<Mutex<Self>>) {
		let (tx, rx) = mpsc::channel();

		let context_info = vec![ContextInfo::default(); num_harts.extend::<usize>() * 2];

		let interrupt_source_states = (0..MAX_IRQ_SOURCES as u16)
			.map(|src_num| (InterruptSource(src_num), false))
			.collect::<FxHashMap<_, _>>();

		let this = Arc::new(Mutex::new(Self {
			interrupt_rx: rx,
			priorities: [0_u32; MAX_IRQ_SOURCES],
			pending: [0_u32; MAX_IRQ_SOURCES / 32],
			claimed: [0_u32; MAX_IRQ_SOURCES / 32],
			context_info,
			interrupt_source_states,
		}));

		(tx, this)
	}

	pub fn poll(&mut self, harts: &mut [WhiskerHart]) {
		loop {
			match self.interrupt_rx.try_recv() {
				Ok(source) => {
					debug!("recv {:?}", source);

					let current_level = *self.interrupt_source_states.get(&source.kind).unwrap();
					if !current_level && source.level {
						self.set_pending(source.kind.inner(), true);
						self.interrupt_source_states.insert(source.kind, true);
					}
				}
				Err(TryRecvError::Empty) => break,
				Err(TryRecvError::Disconnected) => panic!("interrupt controller sources disconnected"),
			}
		}

		for context_id in 0..self.context_info.len() {
			let hart = &mut harts[context_as_hart_idx(context_id)];
			// skip interrupts when in single step mode
			if hart.exec_state == WhiskerExecState::Step {
				continue;
			}

			let is_irq_avail = self.best_irq_for_context(context_id).is_some();
			hart.set_interrupt_pending(context_external_interrupt_trap_idx(context_id), is_irq_avail);
		}
	}
}

impl MMIODevice for PlatformInterruptController {
	fn read(&mut self, _hart: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		use addrs::*;

		let out = from_bytes_mut::<u32>(buf);
		let offset = addr - PLIC_BASE;

		trace!("reading from PLIC offset {:#018X}", offset);

		match offset {
			PRIORITY_REG_MIN..PRIORITY_REG_MAX => {
				let source_idx = ((offset - PRIORITY_REG_MIN) as usize) / 4;
				*out = self.priorities[source_idx];
			}
			PENDING_REG_MIN..PENDING_REG_MAX => {
				let source_idx = (offset - PENDING_REG_MIN) as usize;
				*out = self.pending[source_idx];
			}
			ENABLE_REG_MIN..ENABLE_REG_MAX => {
				let offset = (offset - ENABLE_REG_MIN) as usize;
				let context = offset / 0x80;
				let idx = offset % 0x80;
				let ctx = self.context_info.get(context).unwrap();
				*out = ctx.enabled[idx];
			}
			CONTEXT_REG_MIN..CONTEXT_REG_MAX => {
				let offset = (offset - CONTEXT_REG_MIN) as usize;
				let context_id = offset / 0x1000;
				let context = self.context_info.get_mut(context_id).unwrap();
				match offset % 0x1000 {
					0 => *out = context.priority,
					4 => *out = self.do_claim(context_id),
					off @ ..0x1000 => warn!("write to unknown context field {:?}:{:#06X}", context_id, off),
					_ => unreachable!(),
				}
			}
			_ => warn!("read from unknown PLIC offset {:#018X}", offset),
		}
	}

	fn write(&mut self, _: &mut WhiskerHart, addr: u64, val: &[u8]) {
		use addrs::*;

		let val = u32::from_le_bytes(val.try_into().unwrap());
		let offset = addr - PLIC_BASE;

		trace!("writing to PLIC offset {:#018X}", offset);

		match offset {
			PRIORITY_REG_MIN..PRIORITY_REG_MAX => {
				let source_idx = ((offset - PRIORITY_REG_MIN) as usize) / 4;
				trace!("setting source {:#06X} priority to {}", source_idx, val);
				// source 0 is no source, and is hardwired to 0
				if source_idx != 0 {
					self.priorities[source_idx] = val;
				}
			}
			// interrupt pending bits are not writeable
			PENDING_REG_MIN..PENDING_REG_MAX => todo!("should we trap on trying to write to IP bits?"),
			ENABLE_REG_MIN..ENABLE_REG_MAX => {
				let offset = (offset - ENABLE_REG_MIN) as usize;
				let context = offset / 0x80;
				let idx = offset % 0x80;

				trace!(
					"setting context {:?} enabled idx {:#04X} to {:#032b}",
					context, idx, val
				);

				self.context_info.get_mut(context).unwrap().enabled[idx] = val;
			}
			CONTEXT_REG_MIN..CONTEXT_REG_MAX => {
				let offset = (offset - CONTEXT_REG_MIN) as usize;
				let context_id = offset / 0x1000;
				let context = self.context_info.get_mut(context_id).unwrap();
				match offset % 0x1000 {
					0 => {
						trace!("setting context {:?} priority threshold to {}", context_id, val);
						context.priority = val;
					}
					4 => self.do_complete(context_id, val.truncate()),
					off @ ..0x1000 => warn!("write to unknown context field {:?}:{:#06X}", context_id, off),
					_ => unreachable!(),
				}
			}

			_ => warn!("write to unknown offset {:#018X}", offset),
		}
	}
}

impl PlatformInterruptController {
	fn best_irq_for_context(&self, context: usize) -> Option<InterruptSource> {
		let info = &self.context_info[context];

		let mut best_irq = None;
		let mut best_prio = info.priority;

		trace!(
			"getting best irq for context {:?} prio threshold {}",
			context, best_prio
		);

		for idx in 0..(MAX_IRQ_SOURCES / 32) {
			let pending = self.pending[idx];
			let enabled = info.enabled[idx];
			let claimed = self.claimed[idx];
			trace!(
				"context {:?} idx {} pend {:#010X} enabled {:#010X} claimed {:#010X}",
				context, idx, pending, enabled, claimed
			);

			let avail = pending & enabled & !claimed;
			if avail == 0 {
				continue;
			}

			// because we iterate from lowest irq source to highest, irqs with lower IDs get priority
			// this is correct by the spec
			// https://github.com/riscv/riscv-plic-spec/blob/master/riscv-plic.adoc#3-interrupt-priorities
			for bit_idx in 0..32 {
				let irq_num = idx * 32 + bit_idx;
				let irq_prio = self.priorities[irq_num];
				trace!("irq {} has prio {}", irq_num, irq_prio);
				// whether this specific IRQ is enabled globally
				let irq_enabled = avail & (1 << irq_num) != 0;
				if irq_enabled && irq_prio > best_prio {
					trace!("new best irq {} with prio {}", irq_num, irq_prio);
					best_prio = irq_prio;
					// INVARIANT: irq 0 always has priority 0, so it can never happen
					best_irq = Some(InterruptSource(irq_num as u16));
				}
			}
		}

		debug!("best irq for context {:?}: {:?}", context, best_irq);
		best_irq
	}

	fn do_claim(&mut self, context: usize) -> u32 {
		match self.best_irq_for_context(context) {
			Some(irq) => {
				debug!("context {:?} claimed irq {:?}", context, irq);

				let irq = irq.inner();
				let (idx, bit_idx) = irq_to_idx(irq);
				let mask = 1 << bit_idx;
				self.pending[idx] &= !mask;
				self.claimed[idx] |= mask;

				irq.extend::<u32>()
			}
			None => 0,
		}
	}

	fn do_complete(&mut self, context: usize, irq: u16) {
		debug!("context {:?} completed irq {}", context, irq);
		let (idx, bit_idx) = irq_to_idx(irq);
		let mask = 1 << bit_idx;
		self.claimed[idx] &= !mask;
		self.interrupt_source_states.insert(InterruptSource(irq), false);
	}

	fn set_pending(&mut self, irq: u16, level: bool) {
		let (idx, bit_idx) = irq_to_idx(irq);
		let mask = 1 << bit_idx;
		let val = u32::from(level) << bit_idx;

		self.pending[idx] &= !mask;
		self.pending[idx] |= val;
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterruptMessage {
	kind: InterruptSource,
	level: bool,
}

impl InterruptMessage {
	pub fn new(kind: InterruptSource, level: bool) -> Self {
		Self { kind, level }
	}

	pub fn new_high(kind: InterruptSource) -> Self {
		Self::new(kind, true)
	}

	pub fn new_low(kind: InterruptSource) -> Self {
		Self { kind, level: false }
	}
}

pub const MAX_NUM_CONTEXTS: u64 = HartId::MAX_NUM_HARTS as u64 * 2;

fn context_as_hart_idx(context_id: usize) -> usize {
	context_id >> 1
}

fn context_external_interrupt_trap_idx(context_id: usize) -> TrapIdx {
	match context_id & 1 {
		0 => TrapIdx::MACHINE_EXTERNAL_INTERRUPT,
		1 => TrapIdx::SUPERVISOR_EXTERNAL_INTERRUPT,
		_ => unreachable!("only M and S mode contexts supported"),
	}
}

#[derive(Debug, Default, Clone)]
struct ContextInfo {
	/// which sources are enabled for this context
	/// each source has one bit at index `source/32`, bit `source%32`
	enabled: [u32; MAX_IRQ_SOURCES / 32],
	/// the priority threshold for this context
	/// if an interrupt has a priority less than or equal to this threshold,
	/// it will not be sent to this context
	priority: u32,
}

mod addrs {
	use super::*;

	pub const PRIORITY_REG_MIN: u64 = 0;
	pub const PRIORITY_REG_MAX: u64 = PRIORITY_REG_MIN + 4 * MAX_IRQ_SOURCES as u64;
	pub const PENDING_REG_MIN: u64 = 0x1000;
	pub const PENDING_REG_MAX: u64 = PENDING_REG_MIN + MAX_IRQ_SOURCES as u64 / 32;
	pub const ENABLE_REG_MIN: u64 = 0x2000;
	pub const ENABLE_REG_MAX: u64 = ENABLE_REG_MIN + 0x1F0000;
	pub const CONTEXT_REG_MIN: u64 = 0x200000;
	pub const CONTEXT_REG_MAX: u64 = CONTEXT_REG_MIN + 0x1000 * MAX_NUM_CONTEXTS;
}
