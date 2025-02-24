use std::collections::HashSet;

use log::*;

use crate::csr::ControlStatusRegisters;
use crate::insn::compressed::CompressedInstruction;
use crate::insn::csr::CSRInstruction;
use crate::insn::float::FloatInstruction;
use crate::insn::int::IntInstruction;
use crate::insn::Instruction;
use crate::mem::Memory;
use crate::ty::{FPRegisterIndex, GPRegisterIndex, SupportedExtensions, TrapIdx};

pub type GPRegisters = Registers<u64>;
pub type FPRegisters = Registers<f64>;

#[derive(Default, Debug)]
pub struct Registers<T>
where
	T: Copy,
{
	x: [T; 32],
}

impl<T> Registers<T>
where
	T: Copy,
{
	pub fn regs(&self) -> &[T; 32] {
		&self.x
	}
}

impl GPRegisters {
	pub fn get(&self, index: GPRegisterIndex) -> u64 {
		let index = index.as_usize();
		if index == 0 {
			0
		} else {
			self.x[index]
		}
	}

	pub fn set(&mut self, index: GPRegisterIndex, value: u64) {
		let index = index.as_usize();
		if index == 0 {
			// writes to r0 are ignored
		} else {
			self.x[index] = value;
		}
	}

	/// sets all general purpose registers
	/// NOTE: writes to zero register are ignored
	pub fn set_all(&mut self, regs: &[u64; 32]) {
		self.x[1..].copy_from_slice(&regs[1..]);
	}
}

impl FPRegisters {
	pub fn get(&self, index: FPRegisterIndex) -> f64 {
		let index = index.as_usize();
		self.x[index]
	}

	pub fn set(&mut self, index: FPRegisterIndex, value: f64) {
		let index = index.as_usize();
		self.x[index] = value;
	}

	/// sets all fp registers
	pub fn set_all(&mut self, regs: &[f64; 32]) {
		self.x.copy_from_slice(regs);
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
			registers: Registers::default(),
			fp_registers: Registers::default(),

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

macro_rules! read_mem_f32 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_f32($offset) {
			Ok(val) => val,
			Err(addr) => {
				$self.request_trap(TrapIdx::LOAD_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! read_mem_f64 {
	($self:ident, $offset:ident) => {
		match $self.mem.read_f64($offset) {
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

macro_rules! write_mem_f32 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_f32($offset, $val) {
			Ok(()) => (),
			Err(addr) => {
				$self.request_trap(TrapIdx::STORE_PAGE_FAULT, addr);
				return;
			}
		}
	};
}

macro_rules! write_mem_f64 {
	($self:ident, $offset:ident, $val:ident) => {
		match $self.mem.write_f64($offset, $val) {
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
	fn exec_trap(&mut self) -> Result<(), WhiskerExecStatus> {
		let cause = self.csrs.read_mcause();
		let mtval = self.csrs.read_mtval();
		trace!("executing trap mcause={cause:#018X} mtval={mtval:#018X}");
		let mtvec = self.csrs.read_mtvec();
		trace!("trap handler at {mtvec:#018X}");

		// TODO: there's a lot more CSRs that need to be set up properly here and in request_trap
		self.pc = mtvec;
		// make it so that the next execution cycle of the cpu doesn't go here
		self.should_trap = false;
		panic!();
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
				let val = read_mem_f32!(self, offset);
				self.fp_registers.set(dst, val as f64);
			}
			FloatInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.fp_registers.get(src) as f32;
				write_mem_f32!(self, offset, val);
			}
			FloatInstruction::AddSinglePrecision { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get(lhs) as f32;
				let rhs = self.fp_registers.get(rhs) as f32;
				self.fp_registers.set(dst, f64::from(lhs + rhs));
			}
			FloatInstruction::SubSinglePrecision { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get(lhs) as f32;
				let rhs = self.fp_registers.get(rhs) as f32;
				self.fp_registers.set(dst, f64::from(lhs - rhs));
			}
			FloatInstruction::MulAddSinglePrecision {
				dst,
				mul_lhs,
				mul_rhs,
				add,
			} => {
				let mul_lhs = self.fp_registers.get(mul_lhs) as f32;
				let mul_rhs = self.fp_registers.get(mul_rhs) as f32;
				let add = self.fp_registers.get(add) as f32;

				let result = mul_lhs * mul_rhs + add;
				self.fp_registers.set(dst, f64::from(result));
			}
			FloatInstruction::EqSinglePrecision { dst, lhs, rhs } => {
				let lhs = self.fp_registers.get(lhs) as f32;
				let rhs = self.fp_registers.get(rhs) as f32;
				self.registers.set(dst, lhs.eq(&rhs) as u8 as _);
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

	fn exec_compressed_insn(&mut self, insn: CompressedInstruction, start_pc: u64) {
		match insn {
			CompressedInstruction::AddImmediate16ToSP { imm } => {
				self.registers.set(
					GPRegisterIndex::SP,
					self.registers.get(GPRegisterIndex::SP).wrapping_add_signed(imm),
				);
			}
			CompressedInstruction::LoadUpperImmediate { dst, imm } => {
				self.registers.set(dst, imm as u64);
			}
			CompressedInstruction::LoadWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u32!(self, offset) as u64;

				let reg_val = self.registers.get(dst);
				let val = (reg_val & 0xFFFFFFFF_00000000) | val;
				self.registers.set(dst, val);
			}
			CompressedInstruction::StoreWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src) as u32;
				write_mem_u32!(self, offset, val);
			}
			CompressedInstruction::StoreDoubleWordToSP { src2, offset } => {
				let offset = self.registers.get(GPRegisterIndex::SP).wrapping_add_signed(offset);
				let val = self.registers.get(src2);
				write_mem_u64!(self, offset, val);
			}
			CompressedInstruction::ADDI4SPN { dst, imm } => {
				self.registers
					.set(dst, self.registers.get(GPRegisterIndex::SP).wrapping_add_signed(imm));
			}
			CompressedInstruction::Add { src, dst } => {
				let value = self.registers.get(src).wrapping_add(self.registers.get(dst));
				self.registers.set(dst, value);
			}
			CompressedInstruction::BranchIfZero { src, offset } => {
				if self.registers.get(src) == 0 {
					self.pc = start_pc.wrapping_add_signed(offset);
				}
			}
			CompressedInstruction::AddImmediateWord { dst, rhs } => {
				let lhs = self.registers.get(dst) as u32;
				self.registers.set(dst, lhs.wrapping_add_signed(rhs) as u64);
			}
			CompressedInstruction::Jump { offset } => {
				self.pc = start_pc.wrapping_add_signed(offset);
			}
			CompressedInstruction::Nop => {}
			CompressedInstruction::Move { src, dst } => {
				self.registers.set(dst, self.registers.get(src));
			}
			CompressedInstruction::LoadDoubleWordFromSP { dst, offset } => {
				let offset = self.registers.get(GPRegisterIndex::SP).wrapping_add_signed(offset);
				let val = read_mem_u64!(self, offset);
				self.registers.set(dst, val);
			}
			CompressedInstruction::LoadImmediate { dst, imm } => {
				self.registers.set(dst, imm as u64);
			}
			CompressedInstruction::JumpToRegister { src } => {
				self.pc = self.registers.get(src);
			}
			CompressedInstruction::LoadDoubleWord { dst, src, src_offset } => {
				let offset = self.registers.get(src).wrapping_add_signed(src_offset);
				let val = read_mem_u64!(self, offset);
				self.registers.set(dst, val);
			}
			CompressedInstruction::StoreDoubleWord { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src);
				write_mem_u64!(self, offset, val);
			}
		}
	}

	fn should_poll(&self) -> bool {
		self.cycles % 1024 == 0
	}
}
