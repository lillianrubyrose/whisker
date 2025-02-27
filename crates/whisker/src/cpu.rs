use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt::Write as _;

use tracing::*;

use crate::csr::ControlStatusRegisters;
use crate::insn::atomic::AtomicInstruction;
use crate::insn::compressed::CompressedInstruction;
use crate::insn::csr::CSRInstruction;
use crate::insn::float::FloatInstruction;
use crate::insn::int::IntInstruction;
use crate::insn::Instruction;
use crate::mem::Memory;
use crate::regs::{FPRegisters, GPRegisters};
use crate::soft::ExceptionFlags;
use crate::ty::{GPRegisterIndex, SupportedExtensions, TrapIdx};

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
	pub supported_extensions: SupportedExtensions,
	pub mem: Memory,
	pub registers: GPRegisters,
	pub fp_registers: FPRegisters,

	should_trap: bool,

	pub csrs: ControlStatusRegisters,

	pub pc: u64,
	pub cycles: u64,
	pub exec_state: WhiskerExecState,

	pub breakpoints: HashSet<u64>,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, mem: Memory) -> Self {
		Self {
			supported_extensions,
			mem,
			registers: GPRegisters::default(),
			fp_registers: FPRegisters::default(),

			should_trap: false,
			csrs: ControlStatusRegisters::new(),

			pc: 0,
			cycles: 0,
			exec_state: WhiskerExecState::Paused,
			breakpoints: HashSet::default(),
		}
	}

	pub fn execute_one(&mut self) -> Result<(), WhiskerExecStatus> {
		self.cycles += 1;
		if self.should_trap {
			return self.exec_trap();
		}

		// some instructions (particularly jumps) need the program counter at the start of the instruction
		let start_pc = self.pc;

		if self.breakpoints.contains(&start_pc) {
			return Err(WhiskerExecStatus::HitBreakpoint);
		}

		match Instruction::fetch_instruction(self) {
			Ok((inst, size)) => {
				trace!("fetched {:#?}", inst);
				self.pc = self.pc.wrapping_add(size);
				match inst {
					Instruction::IntExtension(insn) => self.execute_i_insn(insn, start_pc),
					Instruction::FloatExtension(insn) => self.execute_f_insn(insn, start_pc),
					Instruction::Csr(insn) => self.exec_csr(insn, start_pc),
					Instruction::CompressedExtension(insn) => self.exec_compressed_insn(insn, start_pc),
					Instruction::AtomicExtension(insn) => self.exec_atomic_insn(insn, start_pc),
				}
				self.dump();

				Ok(())
			}
			Err(()) => {
				// error during instruction decoding, trap was requested
				Ok(())
			}
		}
	}

	pub fn request_trap(&mut self, trap: TrapIdx, mtval: u64) {
		trace!("requesting trap kind cause={:#018X} mtval={mtval:#018X}", trap.inner());
		// trap causes have the high bit set if they are an interrupt, or unset for exceptions
		self.csrs.write_mcause(trap.inner());
		self.csrs.write_mtval(mtval);
		self.should_trap = true;
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

#[allow(unused)]
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

/// gets a reference to the CSR specified by $addr
/// raises an illegal instruction exception if the CSR does not exist
macro_rules! get_csr {
	($self:ident, $addr:ident) => {
		match $self.csrs.get($addr) {
			// FIXME: check privilege
			Some(info) => info,
			None => {
				$self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				return;
			}
		}
	};
}

/// gets a mutable reference to the CSR specified by $addr
/// raises an illegal instruction exception if the CSR does not exist, is not writable,
/// or could not be read or written at the current privilege
macro_rules! get_csr_mut {
	($self:ident, $addr:ident) => {
		match $self.csrs.get_mut($addr) {
			// FIXME: check privilege
			Some(info) => {
				if !info.is_rw() {
					$self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
					return;
				}
				info
			}
			None => {
				$self.request_trap(TrapIdx::ILLEGAL_INSTRUCTION, 0);
				return;
			}
		}
	};
}

impl WhiskerCpu {
	pub fn dump(&self) {
		// UNWRAPS: writing to string cannot fail

		let mut out = String::from("CPU State:\n");
		writeln!(&mut out, "     pc: {:#018X}\n", self.pc).unwrap();
		let regs = self.registers.regs();
		for idx in 0..32 {
			writeln!(&mut out, "    {:>3}: {:#018X}", format!("x{idx}"), regs[idx]).unwrap();
		}

		writeln!(&mut out).unwrap();
		let fpregs = self.fp_registers.get_all_raw();
		for idx in 0..32 {
			writeln!(
				&mut out,
				"   {:>4}: {:#018X}",
				format!("fp{idx}"),
				u64::from_le_bytes(fpregs[idx].to_le_bytes())
			)
			.unwrap();
		}

		trace!("{}", out);
	}

	fn exec_trap(&mut self) -> Result<(), WhiskerExecStatus> {
		let cause = self.csrs.read_mcause();
		let mtval = self.csrs.read_mtval();
		trace!("executing trap mcause={cause:#018X} mtval={mtval:#018X}");
		let mtvec = self.csrs.read_mtvec();
		trace!("trap handler at {mtvec:#018X}");

		let start_pc = self.pc;
		// TODO: there's a lot more CSRs that need to be set up properly here and in request_trap
		self.pc = mtvec;
		// make it so that the next execution cycle of the cpu doesn't go here
		self.should_trap = false;
		panic!("pc={:#08X}", start_pc);
	}

	fn execute_i_insn(&mut self, insn: IntInstruction, start_pc: u64) {
		match insn {
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val as u64);
			}
			IntInstruction::AddUpperImmediateToPc { dst, val } => {
				self.registers.set(dst, start_pc.wrapping_add_signed(val));
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
				self.registers.set(link_reg, start_pc + 4);
				self.pc = start_pc.wrapping_add_signed(jmp_off);
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
			IntInstruction::JumpAndLinkRegister {
				link_reg,
				jmp_reg,
				jmp_off,
			} => {
				self.registers.set(link_reg, start_pc + 4);
				self.pc = self.registers.get(jmp_reg).wrapping_add_signed(jmp_off) & !1;
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
				self.registers.set(dst, lhs.wrapping_add_signed(rhs) as u64);
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
					self.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchNotEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) != self.registers.get(rhs) {
					self.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThan { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) < self.registers.get(rhs) as i64 {
					self.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqual { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) >= self.registers.get(rhs) as i64 {
					self.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThanUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) < self.registers.get(rhs) {
					self.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqualUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) >= self.registers.get(rhs) {
					self.pc = start_pc.wrapping_add_signed(imm);
				}
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

	fn execute_f_insn(&mut self, insn: FloatInstruction, _start_pc: u64) {
		match insn {
			FloatInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_float!(self, offset);
				self.fp_registers.set_float(dst, val);
			}
			FloatInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.fp_registers.get_float(src).to_u32();
				write_mem_u32!(self, offset, val);
			}
			FloatInstruction::Add { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.add(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::Sub { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.sub(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::MulAdd {
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
			FloatInstruction::Mul { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.mul(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::Div { dst, lhs, rhs, rm } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);
				let result = lhs.div(&rhs, rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::Sqrt { dst, val, rm } => {
				let result = self.fp_registers.get_float(val).sqrt(rm, self);
				self.fp_registers.set_float(dst, result);
			}
			FloatInstruction::Min { dst, lhs, rhs } => {
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
			FloatInstruction::Max { dst, lhs, rhs } => {
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
			FloatInstruction::Equal { dst, lhs, rhs } => {
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
						let val = self.csrs.read_fcsr() | u64::from(ExceptionFlags::FLAG_INVALID);
						self.csrs.write_fcsr(val);
					}
					return;
				};

				self.registers.set(dst, u64::from(cmp == Ordering::Equal));
			}
			//FLT.S and FLE.S perform what the IEEE 754-2008 standard refers to as signaling comparisons: that is,
			//they set the invalid operation exception flag if either input is NaN.
			FloatInstruction::LessThan { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				let Some(cmp) = lhs.partial_cmp(&rhs) else {
					self.registers.set(dst, 0);
					let val = self.csrs.read_fcsr() | u64::from(ExceptionFlags::FLAG_INVALID);
					self.csrs.write_fcsr(val);
					return;
				};

				self.registers.set(dst, u64::from(cmp == Ordering::Less));
			}
			FloatInstruction::LessOrEqual { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get_float(lhs);
				let rhs = self.fp_registers.get_float(rhs);

				// the partial_cmp here returns None if either lhs or rhs is nan
				let Some(cmp) = lhs.partial_cmp(&rhs) else {
					self.registers.set(dst, 0);
					let val = self.csrs.read_fcsr() | u64::from(ExceptionFlags::FLAG_INVALID);
					self.csrs.write_fcsr(val);
					return;
				};

				self.registers
					.set(dst, u64::from(matches!(cmp, Ordering::Less | Ordering::Equal)));
			}
		}
	}

	fn exec_csr(&mut self, insn: CSRInstruction, _start_pc: u64) {
		// FIXME: ordering of effects on registers and traps???
		match insn {
			CSRInstruction::CSRReadWrite { dst, src, csr } => {
				let csr_info = get_csr_mut!(self, csr);
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					self.registers.set(dst, csr_info.val);
				}
				csr_info.val = self.registers.get(src);
			}
			CSRInstruction::CSRReadAndSet { dst, mask, csr } => {
				// we must not check for writability if the mask register is x0
				if mask != GPRegisterIndex::ZERO {
					let csr = get_csr_mut!(self, csr);
					self.registers.set(dst, csr.val);
					csr.val |= self.registers.get(mask);
				} else {
					let csr = get_csr!(self, csr);
					self.registers.set(dst, csr.val);
				}
			}
			CSRInstruction::CSRReadAndClear { dst, mask, csr } => {
				// we must not check for writability if the mask register is x0
				if mask != GPRegisterIndex::ZERO {
					let csr = get_csr_mut!(self, csr);
					self.registers.set(dst, csr.val);
					csr.val &= self.registers.get(mask);
				} else {
					let csr = get_csr!(self, csr);
					self.registers.set(dst, csr.val);
				}
			}
			CSRInstruction::CSRReadWriteImm { dst, src, csr } => {
				let csr_info = get_csr_mut!(self, csr);
				// reads dont happen when dst is zero
				if dst != GPRegisterIndex::ZERO {
					self.registers.set(dst, csr_info.val);
				}
				csr_info.val = src;
			}
			CSRInstruction::CSRReadAndSetImm { dst, mask, csr } => {
				// we must not check for writability if the mask is 0
				if mask != 0 {
					let csr = get_csr_mut!(self, csr);
					self.registers.set(dst, csr.val);
					csr.val |= mask;
				} else {
					let csr = get_csr!(self, csr);
					self.registers.set(dst, csr.val);
				}
			}
			CSRInstruction::CSRReadAndClearImm { dst, mask, csr } => {
				// we must not check for writability if the mask is 0
				if mask != 0 {
					let csr = get_csr_mut!(self, csr);
					self.registers.set(dst, csr.val);
					csr.val &= mask;
				} else {
					let csr = get_csr!(self, csr);
					self.registers.set(dst, csr.val);
				}
			}
		}
	}

	fn exec_compressed_insn(&mut self, insn: CompressedInstruction, _start_pc: u64) {
		match insn {
			// this nop is special in that it's designated as an explicit NOP for future standard use
			// so it cannot be combined into an integer instruction
			CompressedInstruction::Nop => {}
		}
	}

	fn exec_atomic_insn(&mut self, insn: AtomicInstruction, _start_pc: u64) {
		// TODO: For now we'll be ignoring the aq: _ and rl: _ bits as it requires fencing logic and other things we do-
		// not currently implement.
		const HART_ID: usize = 0;
		match insn {
			AtomicInstruction::LoadReservedWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);

				let val = self
					.mem
					.load_reserved_word(addr, HART_ID)
					.expect("addr to be in physmem");

				self.registers.set(dst, val as u64);
			}
			AtomicInstruction::StoreConditionalWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				let val = self.registers.get(src2) as u32;
				let success = self
					.mem
					.store_conditional_word(addr, HART_ID, val)
					.expect("addr to be in physmem");
				if success {
					self.registers.set(dst, 0);
				} else {
					// TODO: From the little bit I read it says "nonzero code on failure"
					// We should figure out what that value should be, unless it's arbitrary
					self.registers.set(dst, 1);
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
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// swap src2 to (src1)
						let src2_val = self.registers.get(src2);
						Some(src2_val as u32)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::AddWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// add src2 value to (src1)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = word.wrapping_add(src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::XorWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// xor src2 value with (src1)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = word ^ src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::AndWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// and src2 value with (src1)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = word & src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::OrWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// or src2 value with (src1)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = word | src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MinWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// min of src2 value and (src1) (signed)
						let src2_val = self.registers.get(src2) as i32;
						let new_val = std::cmp::min(word as i32, src2_val) as u32;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MaxWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// max of src2 value and (src1) (signed)
						let src2_val = self.registers.get(src2) as i32;
						let new_val = std::cmp::max(word as i32, src2_val) as u32;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MinUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// min of src2 value and (src1) (unsigned)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = std::cmp::min(word, src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MaxUnsignedWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_word(addr, |word| {
						// put (src1) value into rd
						self.registers.set(dst, u64::from(word));

						// max of src2 value and (src1) (unsigned)
						let src2_val = self.registers.get(src2) as u32;
						let new_val = std::cmp::max(word, src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}

			AtomicInstruction::LoadReservedDoubleWord { src, dst, _aq, _rl } => {
				let addr = self.registers.get(src);

				let val = self
					.mem
					.load_reserved_dword(addr, HART_ID)
					.expect("addr to be in physmem");

				self.registers.set(dst, val);
			}
			AtomicInstruction::StoreConditionalDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				let val = self.registers.get(src2);
				let success = self
					.mem
					.store_conditional_dword(addr, HART_ID, val)
					.expect("addr to be in physmem");
				if success {
					self.registers.set(dst, 0);
				} else {
					// TODO: From the little bit I read it says "nonzero code on failure"
					// We should figure out what that value should be, unless it's arbitrary
					self.registers.set(dst, 1);
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
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// swap src2 to (src1)
						let src2_val = self.registers.get(src2);
						Some(src2_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::AddDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// add src2 value to (src1)
						let src2_val = self.registers.get(src2);
						let new_val = dword.wrapping_add(src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::XorDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// xor src2 value with (src1)
						let src2_val = self.registers.get(src2);
						let new_val = dword ^ src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::AndDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// and src2 value with (src1)
						let src2_val = self.registers.get(src2);
						let new_val = dword & src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::OrDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// or src2 value with (src1)
						let src2_val = self.registers.get(src2);
						let new_val = dword | src2_val;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MinDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// min of src2 value and (src1) (signed)
						let src2_val = self.registers.get(src2) as i64;
						let new_val = std::cmp::min(dword as i64, src2_val) as u64;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MaxDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// max of src2 value and (src1) (signed)
						let src2_val = self.registers.get(src2) as i64;
						let new_val = std::cmp::max(dword as i64, src2_val) as u64;
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MinUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// min of src2 value and (src1) (unsigned)
						let src2_val = self.registers.get(src2);
						let new_val = std::cmp::min(dword, src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
			AtomicInstruction::MaxUnsignedDoubleWord {
				src1,
				src2,
				dst,
				_aq,
				_rl,
			} => {
				let addr = self.registers.get(src1);
				self.mem
					.atomic_op_dword(addr, |dword| {
						// put (src1) value into rd
						self.registers.set(dst, dword);

						// max of src2 value and (src1) (unsigned)
						let src2_val = self.registers.get(src2);
						let new_val = std::cmp::max(dword, src2_val);
						Some(new_val)
					})
					.expect("addr to be in physmem");
			}
		}
	}

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}
}
