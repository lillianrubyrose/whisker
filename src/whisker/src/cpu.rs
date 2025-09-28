use std::{
	fs::{File, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	sync::Arc,
};

use gdbstub::target::ext::breakpoints::WatchKind;
use rustc_hash::FxHashSet;
use spin::Mutex;

use crate::{mem::mmio::clint::Clint, riscv_tests::RiscTestCommand, tracing::*};

pub mod csr;
pub mod hart;

use crate::{
	cpu::hart::WhiskerHart,
	interrupts::PlatformInterruptController,
	mem::{self, Memory, mmio::MMIOKind},
	ty::{HartId, RiscvExtensions},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WhiskerExecState {
	Step,
	Running,
	Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiskerExecStatus {
	Stepped,
	HitBreakpoint(HartId),
	HitWatchpoint(HartId, WatchKind, u64),
	Paused,
	MMIOShutdown,
}

#[derive(Debug)]
pub struct WhiskerCpu {
	/// the number of execution steps that have happened
	pub steps: u64,
	pub tohost_addr: Option<u64>,
	pub memory: Arc<Memory>,

	/// the index into `harts` which will be executed next
	current_hart_id: HartId,
	pub harts: Vec<WhiskerHart>,

	pub breakpoints: FxHashSet<u64>,

	pub interrupt_controller: Arc<Mutex<PlatformInterruptController>>,
	pub clint: Arc<Mutex<Clint>>,

	logfile: Option<File>,
}

impl WhiskerCpu {
	pub fn new(
		supported_extensions: RiscvExtensions,
		logfile: Option<PathBuf>,
		num_harts: u16,
		initial_pc: u64,
		fs_img: Option<&Path>,
		memory: Arc<Memory>,
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
		let clint = Arc::new(Mutex::new(Clint::new()));

		mem::mmio::register_mmio(MMIOKind::PLIC, interrupt_controller.clone() as _).unwrap();
		mem::mmio::register_mmio(MMIOKind::UART, mem::mmio::UART::init(int_tx.clone()) as _).unwrap();
		mem::mmio::register_mmio(MMIOKind::Clint, clint.clone() as _).unwrap();
		mem::mmio::register_mmio(
			MMIOKind::Shutdown,
			Arc::new(Mutex::new(mem::mmio::shutdown::ShutdownDevice)) as _,
		)
		.unwrap();

		if let Some(fs_img) = fs_img {
			mem::mmio::register_mmio(
				MMIOKind::VirtioBlock,
				mem::mmio::virtio_block::VirtioBlockDevice::init(memory.clone(), fs_img, int_tx.clone()) as _,
			)
			.unwrap();
		}

		Self {
			steps: 0,
			tohost_addr: None,
			memory,

			breakpoints: FxHashSet::default(),

			current_hart_id: HartId::new(0),
			harts,

			interrupt_controller,
			clint,
			logfile,
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecStatus> {
		self.steps += 1;

		if self.steps.is_multiple_of(16) {
			self.interrupt_controller.lock().poll(&mut self.harts);
		}

		let hart_id = self.current_hart_id;
		trace!("executing {:?}", hart_id);
		self.current_hart_id = HartId::new(self.current_hart_id.inner().wrapping_add(1) % self.harts.len() as u16);

		let hart = &mut self.harts[hart_id.as_idx()];
		if hart.exec_state == WhiskerExecState::Paused {
			return Err(WhiskerExecStatus::Paused);
		}

		self.clint.lock().step(hart);

		if self.breakpoints.contains(&hart.pc()) {
			debug!("reached breakpoint at {:#018X} on {:?}", hart.pc(), hart.hart_id());
			return Err(WhiskerExecStatus::HitBreakpoint(hart_id));
		}

		hart.step(&self.memory);

		if let Some(f) = &mut self.logfile {
			let dump = hart.dump();
			f.write_all(dump.as_bytes()).unwrap();
		}

		if let Some(kind) = hart.requested_break.take() {
			warn!("hart {:?} requested break: {:?}", hart_id, kind);
			match kind {
				hart::HartBreakKind::Watchpoint(watch_kind, addr) => {
					return Err(WhiskerExecStatus::HitWatchpoint(hart_id, watch_kind, addr));
				}
				hart::HartBreakKind::DebugPause => {
					return Err(WhiskerExecStatus::Paused);
				}
				hart::HartBreakKind::MMIOShutdown => {
					return Err(WhiskerExecStatus::MMIOShutdown);
				}
			}
		}

		if hart.exec_state == WhiskerExecState::Step {
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
		self.steps.is_multiple_of(1024)
	}

	pub fn check_tohost(&mut self) -> Option<RiscTestCommand> {
		const TOHOST_POLL_RATE: u64 = 4096;

		if let Some(tohost_addr) = self.tohost_addr
			&& self.steps.is_multiple_of(TOHOST_POLL_RATE)
		{
			let bits = self
				.memory
				.read_hw_u64(tohost_addr)
				.unwrap_or_else(|()| panic!("unable to read tohost addr {:#018X}", tohost_addr));
			self.memory
				.write_hw_u64(tohost_addr, 0)
				.unwrap_or_else(|()| panic!("unable to write tohost addr {:#018X}", tohost_addr));

			let mut cmd = RiscTestCommand::new();
			cmd.set_inner(bits.to_le_bytes());
			Some(cmd)
		} else {
			None
		}
	}
}
