use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::tracing::*;

use rustc_hash::FxHashSet;

pub mod csr;
pub mod hart;

use crate::cpu::hart::WhiskerHart;
use crate::interrupts::PlatformInterruptController;
use crate::mem::mmio::MMIOKind;
use crate::mem::{self, Memory};
use crate::ty::{HartId, RiscvExtensions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WhiskerExecState {
	Step,
	Running,
	Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WhiskerExecStatus {
	Stepped,
	HitBreakpoint,
	Paused,
}

pub static MEMORY: OnceLock<Mutex<Memory>> = OnceLock::new();

#[derive(Debug)]
pub struct WhiskerCpu {
	/// the number of execution steps that have happened
	steps: u64,

	/// the index into `harts` which will be executed next
	/// INVARIANT: always in range of harts.len()
	current_hart_id: usize,
	pub harts: Vec<WhiskerHart>,

	pub exec_state: WhiskerExecState,

	pub breakpoints: FxHashSet<u64>,

	pub interrupt_controller: Arc<Mutex<PlatformInterruptController>>,
}

impl WhiskerCpu {
	pub fn new(
		supported_extensions: RiscvExtensions,
		logfile: Option<PathBuf>,
		num_harts: u16,
		initial_pc: u64,
	) -> Self {
		assert!(0 < num_harts && num_harts <= HartId::MAX_NUM_HARTS);

		// FIXME: logfile
		#[expect(unused)]
		let logfile = logfile.map(|path| {
			OpenOptions::new()
				.write(true)
				.create(true)
				.truncate(true)
				.open(&path)
				.unwrap_or_else(|e| panic!("failed to create logfile {}: {:?}", path.display(), e))
		});

		let harts = (0..num_harts)
			.map(|id| WhiskerHart::new(HartId::new(id), supported_extensions, initial_pc))
			.collect();

		// FIXME: interrupt controller refactor
		let (int_tx, interrupt_controller) = PlatformInterruptController::new(num_harts);

		mem::mmio::register_mmio(MMIOKind::PLIC, interrupt_controller.clone() as Arc<Mutex<_>>).unwrap();
		mem::mmio::register_mmio(MMIOKind::UART, mem::mmio::UART::init(int_tx.clone()) as Arc<Mutex<_>>).unwrap();
		mem::mmio::register_mmio(
			MMIOKind::VirtioBlock,
			mem::mmio::virtio_block::VirtioBlockDevice::init(int_tx.clone()) as Arc<Mutex<_>>,
		)
		.unwrap();

		Self {
			steps: 0,
			exec_state: WhiskerExecState::Paused,
			breakpoints: FxHashSet::default(),

			current_hart_id: 0,
			harts,

			interrupt_controller,
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecStatus> {
		self.steps += 1;

		self.interrupt_controller.lock().unwrap().poll(&mut self.harts);

		trace!("executing hart {}", self.current_hart_id);
		let hart = &mut self.harts[self.current_hart_id];

		if self.breakpoints.contains(&hart.pc()) {
			debug!("reached breakpoint at {:#018X} on {:?}", hart.pc(), hart.hart_id());
			return Err(WhiskerExecStatus::HitBreakpoint);
		}

		hart.step();

		Ok(())
	}

	/// If this routine returns [None] then there's incoming GDB data
	/// otherwise it returns the status of executing the cpu.
	/// this function may block until data comes from GDB
	pub fn exec_gdb<F: FnMut() -> bool>(&mut self, mut poll_incoming_data: F) -> Option<WhiskerExecStatus> {
		match self.exec_state {
			WhiskerExecState::Step => match self.execute_one() {
				Ok(()) => Some(WhiskerExecStatus::Stepped),
				Err(e) => Some(e),
			},
			WhiskerExecState::Running => loop {
				if self.should_poll() && poll_incoming_data() {
					return None;
				}

				if let Err(e) = self.execute_one() {
					return Some(e);
				}
			},
			WhiskerExecState::Paused => Some(WhiskerExecStatus::Paused),
		}
	}
}

impl WhiskerCpu {
	fn should_poll(&self) -> bool {
		self.steps % 1024 == 0
	}
}
