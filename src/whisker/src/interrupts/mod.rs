use std::num::NonZeroU16;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use bytemuck::from_bytes_mut;
use num_conv::{Extend, Truncate};
use rustc_hash::FxHashMap;
use tracing::*;

use crate::cpu::hart::WhiskerHart;
use crate::mem::mmio::MMIODevice;
use crate::ty::{HartId, HartMode, TrapIdx};

/// INVARIANT: a valid interrupt source in range 1..=1023
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct InterruptSource(u16);

impl InterruptSource {
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

	context_info: FxHashMap<ContextId, ContextInfo>,
}

/// turns an IRQ number into an index and bit index
fn irq_to_idx(irq: u16) -> (usize, u32) {
	((irq / 32) as usize, (irq % 32) as u32)
}

impl PlatformInterruptController {
	pub fn new(num_harts: u16) -> (Sender<InterruptMessage>, Arc<Mutex<Self>>) {
		let (tx, rx) = mpsc::channel();

		let context_info = (0..num_harts)
			.flat_map(|hart_id| {
				let hart_id = HartId::new(hart_id);
				let m_context = ContextId::new_m_mode(hart_id);
				let s_context = ContextId::new_s_mode(hart_id);

				[(m_context, ContextInfo::default()), (s_context, ContextInfo::default())]
			})
			.collect();

		let this = Arc::new(Mutex::new(Self {
			interrupt_rx: rx,
			priorities: [0_u32; MAX_IRQ_SOURCES],
			pending: [0_u32; MAX_IRQ_SOURCES / 32],
			claimed: [0_u32; MAX_IRQ_SOURCES / 32],
			context_info,
		}));

		(tx, this)
	}

	pub fn poll(&mut self, harts: &mut Vec<WhiskerHart>) {
		match self.interrupt_rx.try_recv() {
			Ok(source) => {
				let (idx, bit_idx) = irq_to_idx(source.kind.inner());
				let mask = 1 << bit_idx;
				let val = u32::from(source.level) << bit_idx;

				self.pending[idx] &= !mask;
				self.pending[idx] |= val;
			}
			Err(TryRecvError::Empty) => {}
			Err(TryRecvError::Disconnected) => panic!("interrupt controller sources disconnected"),
		}

		for context in self.context_info.keys().copied() {
			let hart = &mut harts[context.hart_id().inner() as usize];
			let is_irq_avail = self.best_irq_for_context(context).is_some();
			hart.set_interrupt_pending(context.external_interrupt_trap_idx(), is_irq_avail);
		}
	}
}

impl MMIODevice for PlatformInterruptController {
	fn read(&mut self, _: &mut WhiskerHart, addr: u64, buf: &mut [u8]) {
		let out = from_bytes_mut::<u32>(buf);

		let offset = addr - PLIC_BASE;

		trace!("reading from PLIC offset {:#018X}", offset);

		use addrs::*;
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
				let offset = offset - ENABLE_REG_MIN;
				let context = ContextId::new((offset / 0x80) as u16);
				let idx = (offset % 80) as usize;
				*out = self.context_info.get_mut(&context).unwrap().enabled[idx];
			}
			CONTEXT_REG_MIN..CONTEXT_REG_MAX => {
				let offset = offset - CONTEXT_REG_MIN;
				let context_id = ContextId::new((offset / 0x1000) as u16);
				let context = self.context_info.get_mut(&context_id).unwrap();
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
		assert!(val.len() == 4);
		let val = u32::from_le_bytes(val.try_into().unwrap());
		let offset = addr - PLIC_BASE;

		trace!("writing to PLIC offset {:#018X}", offset);

		use addrs::*;
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
				let offset = offset - ENABLE_REG_MIN;
				let context = ContextId::new((offset / 0x80) as u16);
				let idx = (offset % 80) as usize;

				trace!(
					"setting context {:?} enabled idx {:#04X} to {:#032b}",
					context,
					idx,
					val
				);

				self.context_info.get_mut(&context).unwrap().enabled[idx] = val;
			}
			CONTEXT_REG_MIN..CONTEXT_REG_MAX => {
				let offset = offset - CONTEXT_REG_MIN;
				let context_id = ContextId::new((offset / 0x1000) as u16);
				let context = self.context_info.get_mut(&context_id).unwrap();
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
	fn best_irq_for_context(&self, context: ContextId) -> Option<InterruptSource> {
		let info = &self.context_info[&context];

		let mut best_irq = None;
		let mut best_prio = info.priority;

		trace!(
			"getting best irq for context {:?} prio threshold {}",
			context,
			best_prio
		);

		for idx in 0..(MAX_IRQ_SOURCES / 32) {
			let pending = self.pending[idx];
			let enabled = info.enabled[idx];
			let claimed = self.claimed[idx];
			trace!(
				"context {:?} idx {} pend {:#010X} enabled {:#010X} claimed {:#010X}",
				context,
				idx,
				pending,
				enabled,
				claimed
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

		trace!("best irq for context {:?}: {:?}", context, best_irq);
		best_irq
	}

	fn do_claim(&mut self, context: ContextId) -> u32 {
		match self.best_irq_for_context(context) {
			Some(irq) => {
				trace!("context {:?} claimed irq {:?}", context, irq);

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

	fn do_complete(&mut self, context: ContextId, irq: u16) {
		trace!("context {:?} completed irq {}", context, irq);
		let (idx, bit_idx) = irq_to_idx(irq);
		let mask = 1 << bit_idx;
		self.claimed[idx] &= !mask;
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

/// an interrupt "context", which defines a hart and mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ContextId(u16);

impl ContextId {
	pub const MAX_NUM_CONTEXTS: u64 = HartId::MAX_NUM_HARTS as u64 * 2;

	pub fn new_m_mode(hart_id: HartId) -> Self {
		Self(hart_id.inner() << 1)
	}

	pub fn new_s_mode(hart_id: HartId) -> Self {
		Self(hart_id.inner() << 1 + 1)
	}

	pub fn new(context: u16) -> Self {
		assert!(context.extend::<u64>() < Self::MAX_NUM_CONTEXTS);
		Self(context)
	}

	pub fn hart_id(self) -> HartId {
		HartId::new(self.0 >> 1)
	}

	/// get the appropriate [`TrapIdx`] for this context to signal an external interrupt
	pub fn external_interrupt_trap_idx(self) -> TrapIdx {
		match self.mode() {
			HartMode::Supervisor => TrapIdx::SUPERVISOR_EXTERNAL_INTERRUPT,
			HartMode::Machine => TrapIdx::MACHINE_EXTERNAL_INTERRUPT,
			_ => unreachable!("only M and S mode contexts supported"),
		}
	}

	fn mode(self) -> HartMode {
		if self.0 & 1 == 0 {
			HartMode::Machine
		} else {
			HartMode::Supervisor
		}
	}
}

#[derive(Debug, Default)]
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
	pub const CONTEXT_REG_MAX: u64 = CONTEXT_REG_MIN + 0x1000 * ContextId::MAX_NUM_CONTEXTS;
}
