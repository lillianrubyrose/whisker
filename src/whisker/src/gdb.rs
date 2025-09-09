use std::{
	fmt::Display,
	net::{TcpListener, TcpStream},
	num::NonZeroUsize,
};

use gdbstub::{
	arch::{Arch, Registers},
	common::{Signal, Tid},
	conn::{Connection, ConnectionExt},
	stub::{
		MultiThreadStopReason,
		run_blocking::{BlockingEventLoop, Event, WaitForStopReasonError},
	},
	target::{
		Target, TargetError, TargetResult,
		ext::{
			base::{
				BaseOps,
				multithread::{
					MultiThreadBase, MultiThreadResume, MultiThreadResumeOps, MultiThreadSingleStep,
					MultiThreadSingleStepOps,
				},
			},
			breakpoints::{
				Breakpoints, BreakpointsOps, HwBreakpointOps, HwWatchpoint, HwWatchpointOps, SwBreakpoint,
				SwBreakpointOps,
			},
		},
	},
};
use gdbstub_arch::riscv::reg::id::RiscvRegId;

use crate::{
	WhiskerCpu,
	cpu::{WhiskerExecState, WhiskerExecStatus},
	mem::{ReadKind, WriteKind},
	tracing::*,
};

pub fn wait_for_tcp() -> Result<TcpStream, std::io::Error> {
	let sockaddr = format!("127.0.0.1:{}", 2424);
	info!("Waiting for a GDB connection on {:?}...", sockaddr);

	let sock = TcpListener::bind(sockaddr)?;
	let (stream, addr) = sock.accept()?;
	info!("Debugger connected from {}", addr);

	Ok(stream)
}

pub struct Rv64Arch;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
pub struct Rv64Regs {
	pub x: [u64; 32],
	pub f: [f64; 32],
	pub pc: u64,
}

impl Registers for Rv64Regs {
	type ProgramCounter = u64;

	fn pc(&self) -> Self::ProgramCounter {
		self.pc
	}

	fn gdb_serialize(&self, mut write_byte: impl FnMut(Option<u8>)) {
		macro_rules! write_le_bytes {
			($value:expr) => {{
				let bytes = $value.to_le_bytes();
				for b in bytes {
					write_byte(Some(b));
				}
			}};
		}

		// Write GPRs
		for reg in self.x.iter() {
			write_le_bytes!(reg);
		}

		// Program Counter is regnum 33
		write_le_bytes!(&self.pc);

		// Write FPRs
		for reg in self.f.iter() {
			write_le_bytes!(reg);
		}
	}

	fn gdb_deserialize(&mut self, bytes: &[u8]) -> Result<(), ()> {
		let ptrsize = core::mem::size_of::<u64>();

		// ensure bytes.chunks_exact(ptrsize) won't panic
		if bytes.len() % ptrsize != 0 {
			return Err(());
		}

		let mut regs = bytes
			.chunks_exact(ptrsize)
			.map(|c| u64::from_le_bytes(c.try_into().expect("to be optimized out")));

		// Read GPRs
		for reg in self.x.iter_mut() {
			*reg = regs.next().ok_or(())?;
		}
		self.pc = regs.next().ok_or(())?;

		// Read FPRs
		for reg in self.f.iter_mut() {
			*reg = f64::from_bits(regs.next().ok_or(())?);
		}

		if regs.next().is_some() {
			return Err(());
		}

		Ok(())
	}
}

impl Arch for Rv64Arch {
	type Usize = u64;

	type Registers = Rv64Regs;

	type BreakpointKind = usize;

	type RegId = RiscvRegId<u64>;

	fn target_description_xml() -> Option<&'static str> {
		Some(include_str!("../../../assets/rv64.xml"))
	}
}

pub struct WhiskerEventLoop;

#[derive(Debug, Clone, Copy)]
pub enum WhiskerTargetError {}

impl Display for WhiskerTargetError {
	fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match *self {}
	}
}

impl Target for WhiskerCpu {
	type Arch = Rv64Arch;

	type Error = WhiskerTargetError;

	fn base_ops(&mut self) -> BaseOps<'_, Self::Arch, Self::Error> {
		BaseOps::MultiThread(self)
	}

	fn support_breakpoints(&mut self) -> Option<BreakpointsOps<'_, Self>> {
		Some(self)
	}

	fn use_target_description_xml(&self) -> bool {
		true
	}
}

impl MultiThreadBase for WhiskerCpu {
	fn read_registers(&mut self, regs: &mut <Self::Arch as Arch>::Registers, tid: Tid) -> TargetResult<(), Self> {
		let hart = &self.harts[tid.get() - 1];
		regs.x.copy_from_slice(hart.registers.regs());
		regs.f = hart.fp_registers.get_all_raw().map(f64::from_bits);
		regs.pc = hart.pc();
		Ok(())
	}

	fn write_registers(&mut self, regs: &<Self::Arch as Arch>::Registers, tid: Tid) -> TargetResult<(), Self> {
		let hart = &mut self.harts[tid.get() - 1];
		hart.registers.set_all(&regs.x);
		hart.fp_registers.set_all_raw(&regs.f.map(f64::to_bits));
		hart.set_pc_debug(regs.pc);
		Ok(())
	}

	fn read_addrs(
		&mut self,
		start_addr: <Self::Arch as Arch>::Usize,
		data: &mut [u8],
		tid: Tid,
	) -> TargetResult<usize, Self> {
		let hart = &mut self.harts[tid.get() - 1];
		hart.debug = true;

		for (idx, addr) in (start_addr..(start_addr + data.len() as u64)).enumerate() {
			if let Ok(val) = self.memory.read_u8(hart, addr, ReadKind::Normal) {
				data[idx] = val;
			} else {
				if idx == 0 {
					hart.debug = false;
					// if no bytes were read, report a fault error
					return Err(TargetError::Errno(0x0E));
				}
				hart.debug = false;
				// if an access errors, return the length that has been sucessfully read so far
				return Ok(idx);
			}
		}

		hart.debug = false;
		Ok(data.len())
	}

	fn write_addrs(
		&mut self,
		start_addr: <Self::Arch as Arch>::Usize,
		data: &[u8],
		tid: Tid,
	) -> TargetResult<(), Self> {
		let hart = &mut self.harts[tid.get() - 1];
		hart.debug = true;

		for (idx, addr) in (start_addr..(start_addr + data.len() as u64)).enumerate() {
			let val = data[idx];
			if self.memory.write_u8(hart, addr, WriteKind::Normal, val).is_err() {
				hart.debug = false;
				// if writing failed for any reason, return an error
				return Err(TargetError::Errno(0x0E));
			}
		}

		hart.debug = false;
		Ok(())
	}

	#[allow(clippy::inline_always, reason = "improves perf for calling `thread_is_active`")]
	#[inline(always)]
	fn list_active_threads(&mut self, thread_is_active: &mut dyn FnMut(Tid)) -> Result<(), Self::Error> {
		for idx in 0..self.harts.len() {
			let tid = NonZeroUsize::new(idx + 1).unwrap();
			thread_is_active(tid);
		}
		Ok(())
	}

	fn support_resume(&mut self) -> Option<MultiThreadResumeOps<'_, Self>> {
		Some(self)
	}
}

// FIXME: i think a lot of these are subtly wrong
impl MultiThreadResume for WhiskerCpu {
	fn resume(&mut self) -> Result<(), Self::Error> {
		self.hart_states.iter_mut().for_each(|s| {
			if *s == WhiskerExecState::Paused {
				*s = WhiskerExecState::Running;
			}
		});
		Ok(())
	}

	fn clear_resume_actions(&mut self) -> Result<(), Self::Error> {
		self.hart_states.fill(WhiskerExecState::Paused);
		Ok(())
	}

	fn set_resume_action_continue(&mut self, tid: Tid, _signal: Option<Signal>) -> Result<(), Self::Error> {
		let hart_idx = tid.get() - 1;
		self.hart_states[hart_idx] = WhiskerExecState::Running;
		Ok(())
	}

	fn support_single_step(&mut self) -> Option<MultiThreadSingleStepOps<'_, Self>> {
		Some(self)
	}
}

impl MultiThreadSingleStep for WhiskerCpu {
	fn set_resume_action_step(&mut self, tid: Tid, _signal: Option<Signal>) -> Result<(), Self::Error> {
		let hart_idx = tid.get() - 1;
		self.hart_states[hart_idx] = WhiskerExecState::Step;
		Ok(())
	}
}

impl Breakpoints for WhiskerCpu {
	fn support_sw_breakpoint(&mut self) -> Option<SwBreakpointOps<'_, Self>> {
		Some(self)
	}

	fn support_hw_breakpoint(&mut self) -> Option<HwBreakpointOps<'_, Self>> {
		None
	}

	fn support_hw_watchpoint(&mut self) -> Option<HwWatchpointOps<'_, Self>> {
		Some(self)
	}
}

impl SwBreakpoint for WhiskerCpu {
	fn add_sw_breakpoint(
		&mut self,
		addr: <Self::Arch as Arch>::Usize,
		_kind: <Self::Arch as Arch>::BreakpointKind,
	) -> TargetResult<bool, Self> {
		self.breakpoints.insert(addr);
		Ok(true)
	}

	fn remove_sw_breakpoint(
		&mut self,
		addr: <Self::Arch as Arch>::Usize,
		_kind: <Self::Arch as Arch>::BreakpointKind,
	) -> TargetResult<bool, Self> {
		self.breakpoints.remove(&addr);
		Ok(true)
	}
}

impl HwWatchpoint for WhiskerCpu {
	fn add_hw_watchpoint(
		&mut self,
		addr: <Self::Arch as Arch>::Usize,
		len: <Self::Arch as Arch>::Usize,
		kind: gdbstub::target::ext::breakpoints::WatchKind,
	) -> TargetResult<bool, Self> {
		warn!("adding watchpoint for {:#018X} len {}", addr, len);
		let mut watchpoints = self.memory.watchpoints.write();

		watchpoints.push((addr, len, kind));
		watchpoints.sort_by_key(|(addr, _, _)| *addr);
		Ok(true)
	}

	fn remove_hw_watchpoint(
		&mut self,
		addr: <Self::Arch as Arch>::Usize,
		len: <Self::Arch as Arch>::Usize,
		kind: gdbstub::target::ext::breakpoints::WatchKind,
	) -> TargetResult<bool, Self> {
		warn!("removing watchpoint for {:#018X} len {}", addr, len);
		let mut watchpoints = self.memory.watchpoints.write();
		match watchpoints.iter().position(|e| e == &(addr, len, kind)) {
			Some(idx) => {
				watchpoints.remove(idx);
				Ok(true)
			}
			None => Ok(false),
		}
	}
}

impl BlockingEventLoop for WhiskerEventLoop {
	type Target = WhiskerCpu;

	type Connection = Box<dyn ConnectionExt<Error = std::io::Error>>;

	type StopReason = MultiThreadStopReason<u64>;

	fn wait_for_stop_reason(
		target: &mut Self::Target,
		conn: &mut Self::Connection,
	) -> Result<
		Event<Self::StopReason>,
		WaitForStopReasonError<<Self::Target as Target>::Error, <Self::Connection as Connection>::Error>,
	> {
		let poll_incoming_data = || conn.peek().map(|b| b.is_some()).unwrap_or(true);
		match target.exec_gdb(poll_incoming_data) {
			None => {
				let data = conn.read().map_err(WaitForStopReasonError::Connection)?;
				Ok(Event::IncomingData(data))
			}
			Some(res) => {
				let reason = match res {
					WhiskerExecStatus::Stepped => MultiThreadStopReason::DoneStep,
					WhiskerExecStatus::Paused => MultiThreadStopReason::Signal(Signal::SIGINT),
					WhiskerExecStatus::HitBreakpoint(hart_id) => MultiThreadStopReason::SwBreak(hart_id.as_tid()),
					WhiskerExecStatus::HitWatchpoint(hart_id, kind, addr) => MultiThreadStopReason::Watch {
						tid: hart_id.as_tid(),
						kind,
						addr,
					},
				};
				Ok(Event::TargetStopped(reason))
			}
		}
	}

	fn on_interrupt(target: &mut Self::Target) -> Result<Option<Self::StopReason>, <Self::Target as Target>::Error> {
		target.hart_states.fill(WhiskerExecState::Paused);
		Ok(Some(MultiThreadStopReason::Signal(Signal::SIGINT)))
	}
}
