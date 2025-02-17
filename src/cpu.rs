use std::collections::HashSet;

use log::*;

use crate::csr::ControlStatusRegisters;
use crate::insn::int::IntInstruction;
use crate::insn::Instruction;
use crate::mem::Memory;
use crate::ty::{RegisterIndex, SupportedExtensions, TrapIdx};

#[derive(Default, Debug)]
pub struct Registers {
	x: [u64; 32],

	pub pc: u64,
}

impl Registers {
	pub fn get(&self, index: RegisterIndex) -> u64 {
		let index = index.as_usize();
		if index == 0 {
			0
		} else {
			self.x[index]
		}
	}

	pub fn set(&mut self, index: RegisterIndex, value: u64) {
		let index = index.as_usize();
		if index == 0 {
			// writes to r0 are ignored
		} else {
			self.x[index] = value;
		}
	}

	pub fn regs(&self) -> &[u64; 32] {
		&self.x
	}

	/// sets all general purpose registers
	/// NOTE: writes to zero register are ignored
	pub fn set_all(&mut self, regs: &[u64; 32]) {
		self.x[1..].copy_from_slice(&regs[1..]);
	}
}

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
	pub registers: Registers,

	should_trap: bool,

	pub csrs: ControlStatusRegisters,

	pub cycles: u64,
	pub exec_state: WhiskerExecState,

	pub breakpoints: HashSet<u64>,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, mem: Memory) -> Self {
		Self {
			supported_extensions,
			mem,
			registers: Registers::default(),

			should_trap: false,
			csrs: ControlStatusRegisters::new(),

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
		let start_pc = self.registers.pc;

		if self.breakpoints.contains(&start_pc) {
			return Err(WhiskerExecStatus::HitBreakpoint);
		}

		match Instruction::fetch_instruction(self) {
			Ok((inst, size)) => {
				trace!("fetched {:#?}", inst);
				self.registers.pc = self.registers.pc.wrapping_add(size);
				match inst {
					Instruction::IntExtension(insn) => self.execute_i_insn(insn, start_pc),
				}
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

impl WhiskerCpu {
	fn exec_trap(&mut self) -> Result<(), WhiskerExecStatus> {
		let cause = self.csrs.read_mcause();
		let mtval = self.csrs.read_mtval();
		trace!("executing trap mcause={cause:#018X} mtval={mtval:#018X}");
		let mtvec = self.csrs.read_mtvec();
		trace!("trap handler at {mtvec:#018X}");

		// TODO: there's a lot more CSRs that need to be set up properly here and in request_trap
		self.registers.pc = mtvec;
		// make it so that the next execution cycle of the cpu doesn't go here
		self.should_trap = false;
		Ok(())
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
				self.registers.pc = start_pc.wrapping_add_signed(jmp_off);
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
				self.registers.pc = self.registers.get(jmp_reg).wrapping_add_signed(jmp_off) & !1;
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
				self.registers.set(dst, lhs.wrapping_shr(shift_amt));
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
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchNotEqual { lhs, rhs, imm } => {
				if self.registers.get(lhs) != self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThan { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) < self.registers.get(rhs) as i64 {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqual { lhs, rhs, imm } => {
				if (self.registers.get(lhs) as i64) >= self.registers.get(rhs) as i64 {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchLessThanUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) < self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
				}
			}
			IntInstruction::BranchGreaterEqualUnsigned { lhs, rhs, imm } => {
				if self.registers.get(lhs) >= self.registers.get(rhs) {
					self.registers.pc = start_pc.wrapping_add_signed(imm);
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

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}
}
