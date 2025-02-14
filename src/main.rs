mod insn;
mod mem;
mod ty;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use insn::{Instruction, IntInstruction};
use mem::Memory;
use ty::RegisterIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

impl SupportedExtensions {
	pub const INTEGER: SupportedExtensions = SupportedExtensions(0b1);

	pub const fn empty() -> Self {
		SupportedExtensions(0)
	}

	pub const fn all() -> Self {
		SupportedExtensions(u64::MAX)
	}

	pub const fn has(self, other: Self) -> bool {
		(self.0 & other.0) == other.0
	}

	pub fn insert(&mut self, other: Self) -> &mut Self {
		self.0 |= other.0;
		self
	}

	pub fn remove(&mut self, other: Self) -> &mut Self {
		self.0 &= !other.0;
		self
	}
}

impl BitOr for SupportedExtensions {
	type Output = Self;
	fn bitor(self, rhs: Self) -> Self::Output {
		SupportedExtensions(self.0 | rhs.0)
	}
}

impl BitOrAssign for SupportedExtensions {
	fn bitor_assign(&mut self, rhs: Self) {
		self.0 |= rhs.0;
	}
}

impl BitAnd for SupportedExtensions {
	type Output = Self;
	fn bitand(self, rhs: Self) -> Self::Output {
		SupportedExtensions(self.0 & rhs.0)
	}
}

impl BitAndAssign for SupportedExtensions {
	fn bitand_assign(&mut self, rhs: Self) {
		self.0 &= rhs.0;
	}
}

impl Not for SupportedExtensions {
	type Output = Self;
	fn not(self) -> Self::Output {
		SupportedExtensions(!self.0)
	}
}

#[derive(Default, Debug)]
pub struct Registers {
	x: [u64; 31],

	pub pc: u64,
}

impl Registers {
	pub fn get(&self, index: RegisterIndex) -> u64 {
		let index = usize::from(index.inner());
		if index == 0 {
			0
		} else {
			self.x[index - 1]
		}
	}

	pub fn set(&mut self, index: RegisterIndex, value: u64) {
		let index = usize::from(index.inner());
		if index == 0 {
			// writes to r0 are ignored
		} else {
			self.x[index - 1] = value;
		}
	}
}

pub struct WhiskerCpu {
	pub supported_extensions: SupportedExtensions,
	pub mem: Memory,
	pub registers: Registers,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, mem: Memory) -> Self {
		Self {
			supported_extensions,
			mem,
			registers: Registers::default(),
		}
	}

	fn execute_i_insn(&mut self, insn: IntInstruction, start_pc: u64) {
		match insn {
			IntInstruction::AddImmediate { dst, src, val } => {
				let src_val = self.registers.get(src);
				self.registers.set(dst, src_val.wrapping_add_signed(val));
			}
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val as u64);
			}
			IntInstruction::StoreByte { dst, dst_offset, src } => {
				let offset = self.registers.get(dst).wrapping_add_signed(dst_offset);
				let val = self.registers.get(src).to_le_bytes()[0];
				self.mem.write_u8(offset, val);
			}
			IntInstruction::JumpAndLink { link_reg, jmp_off } => {
				// linking sets the *new* pc to the link register, but sets the pc relative to the old pc
				self.registers.set(link_reg, self.registers.pc as u64);
				self.registers.pc = start_pc.wrapping_add_signed(jmp_off);
			}
		}
	}

	pub fn execute_one(&mut self) {
		dbg!(&self.registers);

		// some instructions (particularly jumps) need the program counter at the start of the instruction
		let start_pc = self.registers.pc as u64;
		// increments pc to past the end of the instruction
		let insn = Instruction::fetch_instruction(self);
		dbg!(&insn);
		match insn {
			Instruction::IntExtension(insn) => self.execute_i_insn(insn, start_pc),
		}
		dbg!(&self.registers);
	}
}

impl Default for WhiskerCpu {
	fn default() -> Self {
		Self {
			supported_extensions: SupportedExtensions::all(),
			mem: Memory::new(0x10001000),
			registers: Default::default(),
		}
	}
}

fn main() {
	let prog = include_bytes!("../target/hello-uart.bin");
	let mut cpu = WhiskerCpu::default();
	cpu.mem.write_slice(0, prog.as_slice());

	loop {
		cpu.execute_one();
	}
}
