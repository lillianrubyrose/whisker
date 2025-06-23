use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::PathBuf;

use num_conv::{Extend, Truncate};
use tracing::*;

pub mod csr;

use crate::cpu::csr::ControlStatusRegisters;
use crate::insn::atomic::AtomicInstruction;
use crate::insn::compressed::CompressedInstruction;
use crate::insn::csr::CSRInstruction;
use crate::insn::float::FloatInstruction;
use crate::insn::int::IntInstruction;
use crate::insn::multiply::MultiplyInstruction;
use crate::insn::privileged::PrivilegedInstruction;
use crate::insn::Instruction;
use crate::log;
use crate::mem::Memory;
use crate::regs::{FPRegisters, GPRegisters};
use crate::soft::ExceptionFlags;
use crate::ty::{GPRegisterIndex, HartId, SupportedExtensions, TrapIdx, TrapKind};
use crate::util::*;

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

#[derive(Debug)]
pub struct WhiskerCpu {
	pub logfile: Option<File>,

	pub supported_extensions: SupportedExtensions,
	pub mem: Memory,
	pub registers: GPRegisters,
	pub fp_registers: FPRegisters,

	pub csrs: ControlStatusRegisters,

	/// the program counter for the currently executing instruction
	pub pc: u64,
	/// the value to set pc to at the end of processing the current cycle
	next_pc: u64,

	pub cycles: u64,
	pub exec_state: WhiskerExecState,

	pub breakpoints: HashSet<u64>,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, mem: Memory, logfile: Option<PathBuf>) -> Self {
		let logfile = logfile.map(|path| {
			OpenOptions::new()
				.write(true)
				.create(true)
				.truncate(true)
				.open(&path)
				.unwrap_or_else(|e| panic!("failed to create logfile {}: {:?}", path.display(), e))
		});
		Self {
			logfile,

			supported_extensions,
			mem,
			registers: GPRegisters::default(),
			fp_registers: FPRegisters::default(),

			csrs: ControlStatusRegisters::new(),

			pc: 0,
			next_pc: 0,
			cycles: 0,
			exec_state: WhiskerExecState::Paused,
			breakpoints: HashSet::default(),
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecStatus> {
		self.cycles += 1;
		log!(self, "cycle {}", self.cycles);

		// DEBUG: send timer interrupts occasionally
		// if self.cycles > 1024 && self.cycles % 100 == 0 {
		// 	// set the machine timer interrupt pending bit
		// 	let mip = self.read_csr_unchecked(csr::MIP);
		// 	self.write_csr_unchecked(csr::MIP, mip | 1 << 7);
		// }

		// if a trap happened, just update pc and return
		// next cycle will fetch
		if self.check_interrupt_trap() {
			self.pc = self.next_pc;
			self.dump();
			return Ok(());
		}

		if self.breakpoints.contains(&self.pc) {
			log!(self, "  reached breakpoint at {:#018X}", self.pc);
			return Err(WhiskerExecStatus::HitBreakpoint);
		}

		match Instruction::fetch_instruction(self) {
			Ok((inst, size)) => {
				log!(self, "  {:#018X}: fetched {:?}", self.pc, inst);
				self.next_pc = self.pc.wrapping_add(size);
				match inst {
					Instruction::IntExtension(insn) => self.execute_i_insn(insn),
					Instruction::FloatExtension(insn) => self.execute_f_insn(insn),
					Instruction::Csr(insn) => self.exec_csr(insn),
					Instruction::CompressedExtension(insn) => self.exec_compressed_insn(insn),
					Instruction::AtomicExtension(insn) => self.exec_atomic_insn(insn),
					Instruction::MultiplyInstruction(insn) => self.exec_multiply_insn(insn),
					Instruction::PrivilegedInstruction(insn) => self.exec_privileged_insn(insn),
				}
			}
			// error during instruction decoding, trap was requested
			Err(()) => {}
		}

		self.pc = self.next_pc;
		self.dump();
		Ok(())
	}

	/// requests the specified trap to happen
	/// sets `next_pc` to the appropriate handler for the trap
	pub fn request_trap(&mut self, trap: TrapIdx, mtval: u64) {
		log!(
			self,
			"  requesting trap kind cause={:#018X} mtval={:#018X}",
			trap.inner(),
			mtval,
		);

		// interrupts can be disabled or enabled by status bits
		if let TrapKind::Interrupt = trap.kind() {
			let status = self.read_csr_unchecked(csr::MSTATUS);
			if status & csr::mstatus::MIE == 0 {
				log!(
					self,
					"  skipped trap cause {:#018X}: machine interrupts were disabled",
					trap.inner()
				);
				return;
			}

			let mie = self.read_csr_unchecked(csr::MIE);
			if mie & (1 << trap.cause()) == 0 {
				log!(
					self,
					"  skipped interrupt cause {:#018X}: cause disabled in MIE CSR",
					trap.inner()
				);
				return;
			}

			// set the bit to signal that the interrupt is pending being handled
			let mip = self.read_csr_unchecked(csr::MIP);
			self.write_csr_unchecked(csr::MIP, mip | (1 << trap.cause()));
		}

		self.write_csr_unchecked(csr::MCAUSE, trap.inner());
		self.write_csr_unchecked(csr::MTVAL, mtval);

		// set MPIE to the value of MIE before the trap was taken
		// and then disable MIE
		let mstatus = self.read_csr_unchecked(csr::MSTATUS);
		let mie = extract_bit_64(mstatus, csr::mstatus::MIE_BIT).truncate::<u8>();
		let mstatus = insert_bit_64(mstatus, mie, csr::mstatus::MPIE_BIT);
		let mstatus = insert_bit_64(mstatus, 0, csr::mstatus::MIE_BIT);
		self.write_csr_unchecked(csr::MSTATUS, mstatus);

		// save interrupted PC for return
		self.write_csr_unchecked(csr::MEPC, self.pc);

		let mtvec = self.read_csr_unchecked(csr::MTVEC);
		log!(self, "  trap handler at {mtvec:#018X}");
		self.next_pc = mtvec;
	}

	/// checks whether the CPU should trap due to an interrupt
	pub fn check_interrupt_trap(&mut self) -> bool {
		log!(self, "checking interrupts");
		let mstatus = self.read_csr_unchecked(csr::MSTATUS);
		if mstatus & csr::mstatus::MIE == 0 {
			log!(self, "  interrupts globally disabled");
			return false;
		}
		// NOTE: mideleg CSR does not exist, so we do not need to check it

		let mip = self.read_csr_unchecked(csr::MIP);
		log!(self, "  mip currently pending: {:#018X}", mip);
		let mie = self.read_csr_unchecked(csr::MIE);
		let to_trap = mip & mie;
		log!(self, "  pending and enabled: {:#018X}", to_trap);
		if to_trap == 0 {
			return false;
		}

		// this subtraction cannot overflow because we know at least one bit is set
		let highest_bit = 63 - to_trap.leading_zeros();
		// implementation specific interrupts have priority from most significant bit to least
		if highest_bit >= 16 {
			self.request_trap(TrapIdx::interrupt(highest_bit as u64), 0);
			return true;
		}

		// standard interrupts have priority:
		// external interrupt
		// software interrupt
		// timer interrupt
		const EXTERNAL_INTERRUPT_MASK: u64 = 1 << 11;
		const SOFTWARE_INTERRUPT_MASK: u64 = 1 << 3;
		const TIMER_INTERRUPT_MASK: u64 = 1 << 7;
		if to_trap & EXTERNAL_INTERRUPT_MASK != 0 {
			self.request_trap(TrapIdx::MACHINE_EXTERNAL_INTERRUPT, 0);
			true
		} else if to_trap & SOFTWARE_INTERRUPT_MASK != 0 {
			self.request_trap(TrapIdx::MACHINE_SOFTWARE_INTERRUPT, 0);
			true
		} else if to_trap & TIMER_INTERRUPT_MASK != 0 {
			self.request_trap(TrapIdx::MACHINE_TIMER_INTERRUPT, 0);
			true
		} else {
			error!("unsupported trap bits {:#018X}", to_trap);
			false
		}
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

macro_rules! read_mem_u8 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_u8($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! read_mem_u16 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_u16($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! read_mem_u32 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_u32($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! read_mem_u64 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_u64($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! read_mem_float {
	($self:ident, $offset:ident) => {
		match $self.mem.read_soft_float($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

#[expect(unused, reason = "doubles NYI")]
macro_rules! read_mem_double {
	($self:ident, $offset:ident) => {
		match $self.mem.read_soft_double($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_u8 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_u8($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_u16 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_u16($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_u32 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_u32($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_u64 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_u64($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_float {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_soft_float($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

#[expect(unused, reason = "doubles NYI")]
macro_rules! write_mem_double {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_soft_double($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

impl WhiskerCpu {
	pub fn dump(&self) {
		if let Some(mut f) = self.logfile.as_ref() {
			// UNWRAPS: writing to string cannot fail

			let mut out = format!("state after cycle {}\n", self.cycles);
			writeln!(&mut out, "    pc: {:#018X}\n", self.pc).unwrap();
			let regs = self.registers.regs();
			for idx in 0..32 {
				let val = regs[idx as usize];
				// we do this for pretty display purposes
				let idx = GPRegisterIndex::new(idx).unwrap();
				writeln!(&mut out, "  {:>4}: {val:#018X} ({val:})", idx.display(),).unwrap();
			}

			writeln!(&mut out).unwrap();
			let fpregs = self.fp_registers.get_all_raw();
			for idx in 0..32 {
				let val = u64::from_le_bytes(fpregs[idx].to_le_bytes());
				writeln!(
					&mut out,
					"  {:>4}: {:#018X} (f64: {}, f32: {})",
					format!("fp{idx}"),
					val,
					f64::from_bits(val),
					f32::from_bits(val as u32)
				)
				.unwrap();
			}
			out.push_str("\n\n");

			write!(f, "{}", out).expect("unable to write to logfile");
			f.flush().expect("unable to flush logfile");
		}
	}

	fn execute_i_insn(&mut self, insn: IntInstruction) {
		match insn {
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val as u64);
			}
			IntInstruction::AddUpperImmediateToPc { dst, val } => {
				self.registers.set(dst, self.pc.wrapping_add_signed(val));
			}
			IntInstruction::StoreByte { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u8;
				write_mem_u8!(self, offset, val);
			}
			IntInstruction::StoreHalf { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u16;
				write_mem_u16!(self, offset, val);
			}
			IntInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u32;
				write_mem_u32!(self, offset, val);
			}
			IntInstruction::StoreDoubleWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src);
				write_mem_u64!(self, offset, val);
			}
			IntInstruction::LoadByte { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u8!(self, offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_FFFFFF00) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadHalf { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u16!(self, offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_FFFF0000) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u32!(self, offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_00000000) | val;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadDoubleWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u64!(self, offset);
				self.registers.set(dst, val);
			}
			IntInstruction::LoadByteZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u8!(self, offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadHalfZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u16!(self, offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::LoadWordZeroExtend { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u32!(self, offset) as u64;
				self.registers.set(dst, val);
			}
			IntInstruction::JumpAndLink { link_reg, jmp_off } => {
				// FIXME: this is not a static +4, it should be "the next instruction"
				self.registers.set(link_reg, self.pc + 4);
				// FIXME: if C extension is not enabled, check alignment
				// note: jmp_off is aligned to 2 by nature of its construction during parsing
				self.next_pc = self.pc.wrapping_add_signed(jmp_off);
			}
			IntInstruction::JumpAndLinkRegister {
				link_reg,
				jmp_reg,
				jmp_off,
			} => {
				// FIXME: this is not a static +4, it should be "the next instruction"
				self.registers.set(link_reg, self.pc + 4);
				// FIXME: if C extension is not enabled, check alignment
				self.next_pc = self.registers.get(jmp_reg).wrapping_add_signed(jmp_off) & !1;
			}
			IntInstruction::Add { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_add(rhs));
			}
			IntInstruction::Sub { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_sub(rhs));
			}
			IntInstruction::Xor { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs ^ rhs);
			}
			IntInstruction::Or { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs | rhs);
			}
			IntInstruction::And { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs & rhs);
			}
			IntInstruction::ShiftLeftLogical { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shl(rhs as u32));
			}
			IntInstruction::ShiftRightLogical { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shr(rhs as u32));
			}
			IntInstruction::ShiftRightArithmetic { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, lhs.wrapping_shr(rhs as u32) as u64);
			}
			IntInstruction::SetLessThan { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs) as i64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::SetLessThanUnsigned { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);
				self.registers.set(dst, (lhs < rhs) as u64);
			}

			IntInstruction::AddImmediate { dst, lhs, rhs } => {
				self.registers
					.set(dst, self.registers.get(lhs).wrapping_add_signed(rhs));
			}
			IntInstruction::XorImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs ^ (rhs as u64));
			}
			IntInstruction::OrImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs | (rhs as u64));
			}
			IntInstruction::AndImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs & (rhs as u64));
			}
			IntInstruction::ShiftLeftLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs.wrapping_shl(shift_amt));
			}
			IntInstruction::ShiftRightLogicalImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs);
				self.registers.set(dst, lhs.wrapping_shr(shift_amt));
			}
			IntInstruction::ShiftRightArithmeticImmediate { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as i64;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}
			IntInstruction::SetLessThanImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as i64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::SetLessThanUnsignedImmediate { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs);
				let rhs = rhs as u64;
				self.registers.set(dst, (lhs < rhs) as u64);
			}
			IntInstruction::AddImmediateWord { dst, lhs, rhs } => {
				let lhs = self.registers.get(lhs) as u32;
				let result = lhs.wrapping_add_signed(rhs);
				// sign extend
				self.registers.set(dst, (result as i32) as i64 as u64);
			}
			IntInstruction::ShiftLeftLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as u32;
				self.registers.set(dst, lhs.wrapping_shl(shift_amt) as u64);
			}
			IntInstruction::ShiftRightLogicalImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as u32;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}
			IntInstruction::ShiftRightArithmeticImmediateWord { dst, lhs, shift_amt } => {
				let lhs = self.registers.get(lhs) as i32;
				self.registers.set(dst, lhs.wrapping_shr(shift_amt) as u64);
			}

			// ============
			// BRANCH
			// ============
			IntInstruction::BranchEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) == self.registers.get(rhs) {
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchNotEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) != self.registers.get(rhs) {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThan { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) < self.registers.get(rhs) as i64 {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqual { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) >= self.registers.get(rhs) as i64 {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThanUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) < self.registers.get(rhs) {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqualUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) >= self.registers.get(rhs) {
					// FIXME: if C extension is not enabled, check alignment
					self.next_pc = self.pc.wrapping_add_signed(imm);
				}
			}

			IntInstruction::AddWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let rhs = self.registers.get(rhs) as u32;
				self.registers.set(dst, lhs.wrapping_add(rhs) as u64);
			}
			IntInstruction::SubWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let rhs = self.registers.get(rhs) as u32;
				self.registers.set(dst, lhs.wrapping_sub(rhs) as u64);
			}
			// These only use the lower 5 bits of the rhs register for shamt
			IntInstruction::ShiftLeftLogicalWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let shamt = (self.registers.get(rhs) as u32) & 0b11111;
				let result = lhs.wrapping_shl(shamt);
				// sign extension
				self.registers.set(dst, (result as i32) as i64 as u64);
			}
			IntInstruction::ShiftRightLogicalWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let shamt = (self.registers.get(rhs) as u32) & 0b11111;
				let result = lhs.wrapping_shr(shamt);
				// sign extension
				self.registers.set(dst, (result as i32) as i64 as u64);
			}
			IntInstruction::ShiftRightArithmeticWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i32;
				let shamt = (self.registers.get(rhs) as u32) & 0b11111;
				let result = lhs.wrapping_shr(shamt);
				// sign extension
				self.registers.set(dst, result as i64 as u64);
			}

			IntInstruction::Fence { .. } => {
				// we don't do reordering, fence is a no-op
			}

			// =========
			// SYSTEM
			// =========
			IntInstruction::ECall => {
				// TODO: handle different modes
				self.request_trap(TrapIdx::ECALL_MMODE, 0);
			}
			IntInstruction::EBreak => {
				// TODO: should this do anything else?
				self.request_trap(TrapIdx::BREAKPOINT, 0);
			}
		}
	}

	fn execute_f_insn(&mut self, insn: FloatInstruction) {
		match insn {
			FloatInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_float!(self, offset);
				self.fp_registers.set_float(dst, val);
			}
			FloatInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.fp_registers.get_float(src);
				write_mem_float!(self, offset, val);
			}
			FloatInstruction::AddSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.add(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SubSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.sub(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulAddSingle {
				dst,
				mul_lhs,
				mul_rhs,
				add,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let add = self.fp_registers.get_float(add);
				let result = mul_lhs.mul_add(&mul_rhs, &add, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulSubSingle {
				dst,
				mul_lhs,
				mul_rhs,
				sub,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let sub = self.fp_registers.get_float(sub);
				let result = mul_lhs.mul_sub(&mul_rhs, &sub, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::NegMulAddSingle {
				dst,
				mul_lhs,
				mul_rhs,
				add,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs).set_sign(false);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let add = self.fp_registers.get_float(add);
				let result = mul_lhs.mul_add(&mul_rhs, &add, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::NegMulSubSingle {
				dst,
				mul_lhs,
				mul_rhs,
				sub,
				rm,
			} => {
				let mul_lhs = self.fp_registers.get_float(mul_lhs).set_sign(false);
				let mul_rhs = self.fp_registers.get_float(mul_rhs);
				let sub = self.fp_registers.get_float(sub);
				let result = mul_lhs.mul_sub(&mul_rhs, &sub, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.mul(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::DivSingle { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.div(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SqrtSingle { dst, val, rm } => {
				let result = self.fp_registers.get_float(val).sqrt(rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::SignInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(rhs.sign()));
			}
			FloatInstruction::SignNotInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(!rhs.sign()));
			}
			FloatInstruction::SignXorInjectionSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				self.fp_registers.set_float(dst, lhs.set_sign(lhs.sign() ^ rhs.sign()));
			}
			FloatInstruction::MinSingle { dst, lhs, rhs } => {
				// TODO: Fix this implementation
				// PAGE: 115
				warn!("FMIN.S Implementation is incorrect");
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				if lhs.lt(&rhs) {
					self.fp_registers.set_float(dst, lhs);
				} else {
					self.fp_registers.set_float(dst, rhs);
				}
			}
			FloatInstruction::MaxSingle { dst, lhs, rhs } => {
				// TODO: Fix this implementation
				// PAGE: 115
				warn!("FMAX.S Implementation is incorrect");
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				if lhs.gt(&rhs) {
					self.fp_registers.set_float(dst, lhs);
				} else {
					self.fp_registers.set_float(dst, rhs);
				}
			}
			FloatInstruction::EqualSingle { dst, lhs, rhs } => {
				//FEQ.S performs a quiet comparison:
				//it only sets the invalid operation exception flag if either input is a signaling NaN. For all three
				//instructions, the result is 0 if either operand is NaN.

				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is NaN
				let Some(cmp) = lhs.partial_cmp(&rhs) else {
					// if any input was NaN, the output is 0
					self.registers.set(dst, 0);
					// if either input was sNaN, write invalid operation
					if lhs.is_snan() || rhs.is_snan() {
						let val = self.read_csr_unchecked(csr::FCSR);
						self.write_csr_unchecked(csr::FCSR, val | u64::from(ExceptionFlags::FLAG_INVALID));
					}
					return;
				};

				self.registers.set(dst, u64::from(cmp == Ordering::Equal));
			}
			//FLT.S and FLE.S perform what the IEEE 754-2008 standard refers to as signaling comparisons: that is,
			//they set the invalid operation exception flag if either input is NaN.
			FloatInstruction::LessThanSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				let Some(cmp) = lhs.partial_cmp(&rhs) else {
					self.registers.set(dst, 0);
					let val = self.read_csr_unchecked(csr::FCSR);
					self.write_csr_unchecked(csr::FCSR, val | u64::from(ExceptionFlags::FLAG_INVALID));
					return;
				};

				self.registers.set(dst, u64::from(cmp == Ordering::Less));
			}
			FloatInstruction::LessOrEqualSingle { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				let Some(cmp) = lhs.partial_cmp(&rhs) else {
					self.registers.set(dst, 0);
					let val = self.read_csr_unchecked(csr::FCSR);
					self.write_csr_unchecked(csr::FCSR, val | u64::from(ExceptionFlags::FLAG_INVALID));
					return;
				};

				self.registers
					.set(dst, u64::from(matches!(cmp, Ordering::Less | Ordering::Equal)));
			}
		}
	}

	fn exec_csr(&mut self, insn: CSRInstruction) {
		// FIXME: ordering of effects on registers and traps???
		match insn {
			CSRInstruction::CSRReadWrite { dst, src, csr } => {
				let Some(token) = self.csr_require_rw(csr) else {
					return;
				};
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
				let new_val = self.registers.get(src);
				self.write_csr(&token, new_val);
			}
			CSRInstruction::CSRReadAndSet { dst, mask, csr } => {
				// we must not check for writability if the mask register is x0
				if mask != GPRegisterIndex::ZERO {
					let Some(token) = self.csr_require_rw(csr) else {
						return;
					};

					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val | self.registers.get(mask));
				} else {
					let Some(token) = self.csr_require_ro(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
			CSRInstruction::CSRReadAndClear { dst, mask, csr } => {
				// we must not check for writability if the mask register is x0
				if mask != GPRegisterIndex::ZERO {
					let Some(token) = self.csr_require_rw(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val & self.registers.get(mask));
				} else {
					let Some(token) = self.csr_require_ro(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
			CSRInstruction::CSRReadWriteImm { dst, imm, csr } => {
				let Some(token) = self.csr_require_rw(csr) else {
					return;
				};
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
				self.write_csr(&token, imm);
			}
			CSRInstruction::CSRReadAndSetImm { dst, mask, csr } => {
				// we must not check for writability if the mask is 0
				if mask != 0 {
					let Some(token) = self.csr_require_rw(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val | mask);
				} else {
					let Some(token) = self.csr_require_ro(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
			CSRInstruction::CSRReadAndClearImm { dst, mask, csr } => {
				// we must not check for writability if the mask is 0
				if mask != 0 {
					let Some(token) = self.csr_require_rw(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
					self.write_csr(&token, val & mask);
				} else {
					let Some(token) = self.csr_require_ro(csr) else {
						return;
					};
					let val = self.read_csr(&token);
					self.registers.set(dst, val);
				}
			}
		}
	}

	fn exec_compressed_insn(&mut self, insn: CompressedInstruction) {
		match insn {
			// this nop is special in that it's designated as an explicit NOP for future standard use
			// so it cannot be combined into an integer instruction
			CompressedInstruction::Nop => {}
		}
	}

	fn exec_atomic_insn(&mut self, insn: AtomicInstruction) {
		// TODO(atomic): For now we'll be ignoring the aq: _ and rl: _ bits as it requires fencing logic
		// and other things we do not currently implement.
		match insn {
			AtomicInstruction::LoadReservedWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.load_reserved_word(addr, HartId::HART0) {
					Ok(val) => {
						self.registers.set(dst, val.extend());
					}
					Err(addr) => {
						self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::StoreConditionalWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, addr);
					return;
				}

				let val = self.registers.get(src2) as u32;

				match self.mem.store_conditional_word(addr, HartId::HART0, val) {
					Ok(true) => self.registers.set(dst, 0),
					Ok(false) => self.registers.set(dst, 1),
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::SwapWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// swap src2 to (src1)
					let src2_val = self.registers.get(src2);
					Some(src2_val as u32)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::AddWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// add src2 value to (src1)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = word.wrapping_add(src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::XorWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// xor src2 value with (src1)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = word ^ src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::AndWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// and src2 value with (src1)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = word & src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::OrWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// or src2 value with (src1)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = word | src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MinWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// min of src2 value and (src1) (signed)
					let src2_val = self.registers.get(src2) as i32;
					let new_val = std::cmp::min(word as i32, src2_val) as u32;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MaxWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// max of src2 value and (src1) (signed)
					let src2_val = self.registers.get(src2) as i32;
					let new_val = std::cmp::max(word as i32, src2_val) as u32;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MinUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// min of src2 value and (src1) (unsigned)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = std::cmp::min(word, src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MaxUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 4 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_word(addr, |word| {
					// put (src1) value into rd
					self.registers.set(dst, u64::from(word));

					// max of src2 value and (src1) (unsigned)
					let src2_val = self.registers.get(src2) as u32;
					let new_val = std::cmp::max(word, src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}

			AtomicInstruction::LoadReservedDoubleWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.load_reserved_dword(addr, HartId::HART0) {
					Ok(val) => {
						self.registers.set(dst, val);
					}
					Err(addr) => {
						self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::StoreConditionalDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::STORE_ADDR_MISALIGNED, addr);
					return;
				}

				let val = self.registers.get(src2);
				match self.mem.store_conditional_dword(addr, HartId::HART0, val) {
					Ok(true) => self.registers.set(dst, 0),
					Ok(false) => self.registers.set(dst, 1),
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::SwapDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// swap src2 to (src1)
					let src2_val = self.registers.get(src2);
					Some(src2_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::AddDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// add src2 value to (src1)
					let src2_val = self.registers.get(src2);
					let new_val = dword.wrapping_add(src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::XorDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// xor src2 value with (src1)
					let src2_val = self.registers.get(src2);
					let new_val = dword ^ src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::AndDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// and src2 value with (src1)
					let src2_val = self.registers.get(src2);
					let new_val = dword & src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::OrDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// or src2 value with (src1)
					let src2_val = self.registers.get(src2);
					let new_val = dword | src2_val;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MinDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// min of src2 value and (src1) (signed)
					let src2_val = self.registers.get(src2) as i64;
					let new_val = std::cmp::min(dword as i64, src2_val) as u64;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MaxDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// max of src2 value and (src1) (signed)
					let src2_val = self.registers.get(src2) as i64;
					let new_val = std::cmp::max(dword as i64, src2_val) as u64;
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MinUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// min of src2 value and (src1) (unsigned)
					let src2_val = self.registers.get(src2);
					let new_val = std::cmp::min(dword, src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
			AtomicInstruction::MaxUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				if addr % 8 != 0 {
					self.request_trap(TrapIdx::LOAD_ADDR_MISALIGNED, addr);
					return;
				}

				match self.mem.atomic_op_dword(addr, |dword| {
					// put (src1) value into rd
					self.registers.set(dst, dword);

					// max of src2 value and (src1) (unsigned)
					let src2_val = self.registers.get(src2);
					let new_val = std::cmp::max(dword, src2_val);
					Some(new_val)
				}) {
					Ok(_) => {}
					Err(addr) => {
						self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
					}
				}
			}
		}
	}

	fn exec_multiply_insn(&mut self, insn: MultiplyInstruction) {
		match insn {
			MultiplyInstruction::Multiply { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				self.registers.set(dst, lhs.wrapping_mul(rhs));
			}
			MultiplyInstruction::MultiplyHigh { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs) as i64;

				// full 128bit signed mul
				let product = (lhs as i128) * (rhs as i128);
				let hi_bits = (product >> 64) as u64;

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::MultiplyHighSignedUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs);

				// full 128bit signed x unsigned mul. i think this is right????
				let product = (lhs as i128) * (rhs as u128 as i128);
				let hi_bits = (product >> 64) as u64;

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::MultiplyHighUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				// full 128bit unsigned mul
				let product = (lhs as u128) * (rhs as u128);
				let hi_bits = (product >> 64) as u64;

				self.registers.set(dst, hi_bits);
			}
			MultiplyInstruction::Divide { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs) as i64;

				let result = if rhs == 0 {
					-1i64 // div by zero returns -1
				} else if lhs == i64::MIN && rhs == -1 {
					lhs // overflow returns dividend
				} else {
					lhs.wrapping_div(rhs)
				};

				self.registers.set(dst, result as u64);
			}
			MultiplyInstruction::DivideUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				// div by zero returns 0b1111111111...
				let result = if rhs == 0 { u64::MAX } else { lhs.wrapping_div(rhs) };

				self.registers.set(dst, result);
			}
			MultiplyInstruction::Remainder { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i64;
				let rhs = self.registers.get(rhs) as i64;

				let result = if rhs == 0 {
					lhs // rem by zero returns dividend
				} else if lhs == i64::MIN && rhs == -1 {
					0 // overflow returns nothing
				} else {
					lhs.wrapping_rem(rhs)
				};

				self.registers.set(dst, result as u64);
			}
			MultiplyInstruction::RemainderUnsigned { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs);
				let rhs = self.registers.get(rhs);

				// rem by zero returns dividend
				let result = if rhs == 0 { lhs } else { lhs.wrapping_rem(rhs) };

				self.registers.set(dst, result);
			}

			MultiplyInstruction::MultiplyWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let rhs = self.registers.get(rhs) as u32;

				// TODO: babygirl is this right? lily is only like, half sure of this.
				// ty in advance~ <3
				let result = ((lhs as i32).wrapping_mul(rhs as i32)) as i64;

				self.registers.set(dst, result as u64);
			}
			MultiplyInstruction::DivideWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i32;
				let rhs = self.registers.get(rhs) as i32;

				let result = if rhs == 0 {
					-1i32 // div by zero returns -1
				} else if lhs == i32::MIN && rhs == -1 {
					lhs // overflow returns dividend
				} else {
					lhs.wrapping_div(rhs)
				};

				self.registers.set(dst, result as i64 as u64);
			}
			MultiplyInstruction::DivideUnsignedWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let rhs = self.registers.get(rhs) as u32;

				// div by zero returns 0b111111111...
				let result = if rhs == 0 { u32::MAX } else { lhs.wrapping_div(rhs) };

				self.registers.set(dst, result as u64);
			}
			MultiplyInstruction::RemainderWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as i32;
				let rhs = self.registers.get(rhs) as i32;

				let result = if rhs == 0 {
					lhs // rem by zero returns dividend
				} else if lhs == i32::MIN && rhs == -1 {
					0 // overflow returns nothing
				} else {
					lhs.wrapping_rem(rhs)
				};

				self.registers.set(dst, result as i64 as u64);
			}
			MultiplyInstruction::RemainderUnsignedWord { lhs, rhs, dst } => {
				let lhs = self.registers.get(lhs) as u32;
				let rhs = self.registers.get(rhs) as u32;

				// rem by zero returns dividend
				let result = if rhs == 0 { lhs } else { lhs.wrapping_rem(rhs) };

				self.registers.set(dst, result as u64);
			}
		}
	}

	fn exec_privileged_insn(&mut self, insn: PrivilegedInstruction) {
		match insn {
			PrivilegedInstruction::Mret => {
				let status = self.read_csr_unchecked(csr::MSTATUS);
				let mpie = extract_bit_64(status, csr::mstatus::MPIE_BIT).truncate::<u8>();
				let new_priv = extract_bits_64(status, csr::mstatus::MPP_START, csr::mstatus::MPP_END);
				debug_assert_eq!(new_priv, 0b11, "only M mode is supported");

				// set MIE to MPIE and MPIE to 1
				let status = insert_bit_64(status, mpie, csr::mstatus::MIE_BIT);
				let status = insert_bit_64(status, 1, csr::mstatus::MPIE_BIT);
				// do not need to set MPP, that is read only 0b11 and cannot be modified

				self.write_csr_unchecked(csr::MSTATUS, status);
				self.next_pc = self.read_csr_unchecked(csr::MEPC);
			}
		}
	}

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}
}
