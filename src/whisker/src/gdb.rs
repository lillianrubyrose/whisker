use std::net::{TcpListener, TcpStream};

use gdbstub::arch::{Arch, Registers};
use gdbstub::target::TargetError;
use gdbstub::{
	common::Signal,
	conn::ConnectionExt,
	stub::{
		run_blocking::{BlockingEventLoop, Event, WaitForStopReasonError},
		SingleThreadStopReason,
	},
	target::{
		ext::{
			base::singlethread::{SingleThreadBase, SingleThreadResume, SingleThreadSingleStep},
			breakpoints::{Breakpoints, SwBreakpoint},
		},
		Target,
	},
};
use gdbstub_arch::riscv::reg::id::RiscvRegId;

use crate::cpu::{WhiskerExecState, WhiskerExecStatus};
use crate::WhiskerCpu;

pub fn wait_for_tcp() -> Result<TcpStream, std::io::Error> {
	let sockaddr = format!("127.0.0.1:{}", 2424);
	eprintln!("Waiting for a GDB connection on {:?}...", sockaddr);

	let sock = TcpListener::bind(sockaddr)?;
	let (stream, addr) = sock.accept()?;
	eprintln!("Debugger connected from {}", addr);

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
			($value:expr) => {
				let bytes = $value.to_le_bytes();
				for b in bytes {
					write_byte(Some(b));
				}
			};
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
			*reg = regs.next().ok_or(())?
		}
		self.pc = regs.next().ok_or(())?;

		// Read FPRs
		for reg in self.f.iter_mut() {
			*reg = f64::from_bits(regs.next().ok_or(())?)
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

impl Target for WhiskerCpu {
	type Arch = Rv64Arch;

	type Error = ();

	fn base_ops(&mut self) -> gdbstub::target::ext::base::BaseOps<'_, Self::Arch, Self::Error> {
		gdbstub::target::ext::base::BaseOps::SingleThread(self)
	}

	fn support_breakpoints(&mut self) -> Option<gdbstub::target::ext::breakpoints::BreakpointsOps<'_, Self>> {
		Some(self)
	}

	fn use_target_description_xml(&self) -> bool {
		true
	}
}

impl SingleThreadBase for WhiskerCpu {
	fn read_registers(
		&mut self,
		regs: &mut <Self::Arch as gdbstub::arch::Arch>::Registers,
	) -> gdbstub::target::TargetResult<(), Self> {
		regs.x.copy_from_slice(self.registers.regs());
		regs.f = self.fp_registers.get_all_raw().map(f64::from_bits);
		regs.pc = self.pc;
		Ok(())
	}

	fn write_registers(
		&mut self,
		regs: &<Self::Arch as gdbstub::arch::Arch>::Registers,
	) -> gdbstub::target::TargetResult<(), Self> {
		assert_eq!(regs.x[0], 0, "tried to write non-zero to x0(zero) register");
		self.registers.set_all(&regs.x);
		self.fp_registers.set_all_raw(&regs.f.map(f64::to_bits));
		self.pc = regs.pc;
		Ok(())
	}

	fn read_addrs(
		&mut self,
		start_addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
		data: &mut [u8],
	) -> gdbstub::target::TargetResult<usize, Self> {
		match self.mem.read_slice(start_addr, data) {
			Ok(()) => Ok(data.len()),
			// FIXME: does this do what we want
			Err(addr) => Ok((addr - start_addr) as usize),
		}
	}

	fn write_addrs(
		&mut self,
		start_addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
		data: &[u8],
	) -> gdbstub::target::TargetResult<(), Self> {
		match self.mem.write_slice(start_addr, data) {
			Ok(()) => Ok(()),
			// EREMOTEIO - causes gdb to report "cannot access memory at <start_addr>"
			Err(_addr) => Err(TargetError::Errno(121)),
		}
	}

	fn support_resume(&mut self) -> Option<gdbstub::target::ext::base::singlethread::SingleThreadResumeOps<'_, Self>> {
		Some(self)
	}
}

impl SingleThreadResume for WhiskerCpu {
	fn resume(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
		self.exec_state = WhiskerExecState::Running;
		Ok(())
	}

	fn support_single_step(
		&mut self,
	) -> Option<gdbstub::target::ext::base::singlethread::SingleThreadSingleStepOps<'_, Self>> {
		Some(self)
	}
}

impl SingleThreadSingleStep for WhiskerCpu {
	fn step(&mut self, _signal: Option<Signal>) -> Result<(), Self::Error> {
		self.exec_state = WhiskerExecState::Step;
		Ok(())
	}
}

impl Breakpoints for WhiskerCpu {
	fn support_sw_breakpoint(&mut self) -> Option<gdbstub::target::ext::breakpoints::SwBreakpointOps<'_, Self>> {
		Some(self)
	}

	fn support_hw_breakpoint(&mut self) -> Option<gdbstub::target::ext::breakpoints::HwBreakpointOps<'_, Self>> {
		None
	}

	fn support_hw_watchpoint(&mut self) -> Option<gdbstub::target::ext::breakpoints::HwWatchpointOps<'_, Self>> {
		None
	}
}

impl SwBreakpoint for WhiskerCpu {
	fn add_sw_breakpoint(
		&mut self,
		addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
		_kind: <Self::Arch as gdbstub::arch::Arch>::BreakpointKind,
	) -> gdbstub::target::TargetResult<bool, Self> {
		self.breakpoints.insert(addr);
		Ok(true)
	}

	fn remove_sw_breakpoint(
		&mut self,
		addr: <Self::Arch as gdbstub::arch::Arch>::Usize,
		_kind: <Self::Arch as gdbstub::arch::Arch>::BreakpointKind,
	) -> gdbstub::target::TargetResult<bool, Self> {
		self.breakpoints.remove(&addr);
		Ok(true)
	}
}

impl BlockingEventLoop for WhiskerEventLoop {
	type Target = WhiskerCpu;

	type Connection = Box<dyn ConnectionExt<Error = std::io::Error>>;

	type StopReason = SingleThreadStopReason<u64>;

	fn wait_for_stop_reason(
		target: &mut Self::Target,
		conn: &mut Self::Connection,
	) -> Result<
		gdbstub::stub::run_blocking::Event<Self::StopReason>,
		gdbstub::stub::run_blocking::WaitForStopReasonError<
			<Self::Target as gdbstub::target::Target>::Error,
			<Self::Connection as gdbstub::conn::Connection>::Error,
		>,
	> {
		let poll_incoming_data = || conn.peek().map(|b| b.is_some()).unwrap_or(true);
		match target.exec_gdb(poll_incoming_data) {
			None => {
				let data = conn.read().map_err(WaitForStopReasonError::Connection)?;
				Ok(Event::IncomingData(data))
			}
			Some(res) => {
				let reason = match res {
					WhiskerExecStatus::Stepped => SingleThreadStopReason::DoneStep,
					WhiskerExecStatus::Paused => SingleThreadStopReason::Signal(Signal::SIGINT),
					WhiskerExecStatus::HitBreakpoint => SingleThreadStopReason::SwBreak(()),
				};
				Ok(Event::TargetStopped(reason))
			}
		}
	}

	fn on_interrupt(
		target: &mut Self::Target,
	) -> Result<Option<Self::StopReason>, <Self::Target as gdbstub::target::Target>::Error> {
		target.exec_state = WhiskerExecState::Paused;
		Ok(Some(SingleThreadStopReason::Signal(Signal::SIGINT)))
	}
}
