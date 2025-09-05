use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use spin::Mutex;

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
	HitBreakpoint(HartId),
	Paused,
}

pub static MEMORY: OnceLock<Arc<Memory>> = OnceLock::new();

#[derive(Debug)]
pub struct WhiskerCpu {
	/// the number of execution steps that have happened
	steps: u64,

	/// the index into `harts` which will be executed next
	current_hart_id: HartId,
	pub harts: Vec<WhiskerHart>,
	pub hart_states: Vec<WhiskerExecState>,

	pub breakpoints: FxHashSet<u64>,

	pub interrupt_controller: Arc<Mutex<PlatformInterruptController>>,

	logfile: Option<File>,
}

impl WhiskerCpu {
	pub fn new(
		supported_extensions: RiscvExtensions,
		logfile: Option<PathBuf>,
		num_harts: u16,
		initial_pc: u64,
		fs_img: Option<&Path>,
	) -> Self {
		assert!(0 < num_harts && num_harts <= HartId::MAX_NUM_HARTS);

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

		mem::mmio::register_mmio(MMIOKind::PLIC, interrupt_controller.clone() as _).unwrap();
		mem::mmio::register_mmio(MMIOKind::UART, mem::mmio::UART::init(int_tx.clone()) as _).unwrap();

		if let Some(fs_img) = fs_img {
			mem::mmio::register_mmio(
				MMIOKind::VirtioBlock,
				mem::mmio::virtio_block::VirtioBlockDevice::init(fs_img, int_tx.clone()) as _,
			)
			.unwrap();
		}

		Self {
			steps: 0,
			breakpoints: FxHashSet::default(),

			current_hart_id: HartId::new(0),
			harts,
			hart_states: vec![WhiskerExecState::Paused; num_harts as usize],

			interrupt_controller,
			logfile,
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecStatus> {
		self.steps += 1;

		if self.steps % 16 == 0 {
			self.interrupt_controller.lock().poll(&mut self.harts);
		}

		trace!("executing {:?}", self.current_hart_id);

		if self.hart_states[self.current_hart_id.as_idx()] == WhiskerExecState::Paused {
			return Err(WhiskerExecStatus::Paused);
		}

		let hart = &mut self.harts[self.current_hart_id.as_idx()];

		if self.breakpoints.contains(&hart.pc()) {
			debug!("reached breakpoint at {:#018X} on {:?}", hart.pc(), hart.hart_id());
			return Err(WhiskerExecStatus::HitBreakpoint(self.current_hart_id));
		}

		hart.step();

		if let Some(f) = &mut self.logfile {
			let dump = hart.dump();
			f.write_all(dump.as_bytes()).unwrap();
		}

		if self.hart_states[self.current_hart_id.as_idx()] == WhiskerExecState::Step {
			return Err(WhiskerExecStatus::Stepped);
		}

		Ok(())
	}

	/// If this routine returns [None] then there's incoming GDB data
	/// otherwise it returns the status of executing the cpu.
	/// this function may block until data comes from GDB
	pub fn exec_gdb<F: FnMut() -> bool>(&mut self, mut poll_incoming_data: F) -> Option<WhiskerExecStatus> {
		loop {
			if self.should_poll() && poll_incoming_data() {
				return None;
			}

			if let Err(e) = self.execute_one() {
				return Some(e);
			}
		}
	}
}

impl WhiskerCpu {
	fn should_poll(&self) -> bool {
		self.steps % 1024 == 0
	}
}
