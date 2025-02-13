mod insn;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use insn::{Instruction, IntInstruction};

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

	pub pc: usize,
}

impl Registers {
	// TODO: replace index with RegisterIndex newtype with the 0-31 constraint
	pub fn get(&self, index: usize) -> u64 {
		if index == 0 {
			0
		} else {
			self.x[index - 1]
		}
	}

	pub fn set(&mut self, index: usize, value: u64) {
		if index == 0 {
			// nop
		} else {
			self.x[index - 1] = value;
		}
	}
}

pub struct PhysicalMemory {
	inner: Box<[u8]>,
}

impl PhysicalMemory {
	pub fn new(size: usize) -> Self {
		Self {
			inner: vec![0; size].into_boxed_slice(),
		}
	}

	pub fn read_u16(&self, offset: usize) -> u16 {
		let lo = self.inner[offset];
		let hi = self.inner[offset + 1];
		u16::from_le_bytes([lo, hi])
	}

	pub fn read_u32(&self, offset: usize) -> u32 {
		let b0 = self.inner[offset];
		let b1 = self.inner[offset + 1];
		let b2 = self.inner[offset + 2];
		let b3 = self.inner[offset + 3];
		u32::from_le_bytes([b0, b1, b2, b3])
	}
}

pub struct WhiskerCpu {
	pub supported_extensions: SupportedExtensions,
	pub physmem: PhysicalMemory,
	pub registers: Registers,
}

impl WhiskerCpu {
	pub fn new(supported_extensions: SupportedExtensions, physmem: PhysicalMemory) -> Self {
		Self {
			supported_extensions,
			physmem,
			registers: Registers::default(),
		}
	}

	fn execute_i_insn(&mut self, insn: IntInstruction) {
		match insn {
			IntInstruction::AddImmediate { dst, src, val } => {
				let src_val = self.registers.get(src);
				self.registers.set(dst, src_val + val as u64);
			}
			IntInstruction::LoadUpperImmediate { dst, val } => {
				self.registers.set(dst, val as u64);
			}
			IntInstruction::StoreByte { dst, dst_offset, src } => {
				let offset = (self.registers.get(dst) + dst_offset as u64) as usize;
				self.physmem.inner[offset] = self.registers.get(src).to_le_bytes()[0];
			}
			IntInstruction::JumpAndLink { link_reg, jmp_addr } => {
				self.registers.set(link_reg, self.registers.pc as u64);
				self.registers.pc = jmp_addr as usize;
			}
		}
	}

	pub fn execute_one(&mut self) {
		dbg!(&self.registers);

		// Increments PC by instruction size
		let insn = Instruction::fetch_instruction(self);
		dbg!(&insn);
		match insn {
			Instruction::IntExtension(insn) => self.execute_i_insn(insn),
		}
		dbg!(&self.registers);
	}
}

impl Default for WhiskerCpu {
	fn default() -> Self {
		Self {
			supported_extensions: SupportedExtensions::all(),
			physmem: PhysicalMemory::new(0x10001000),
			registers: Default::default(),
		}
	}
}

fn main() {
	let prog = include_bytes!("../target/hello-uart.bin");
	let mut cpu = WhiskerCpu::default();
	cpu.physmem.inner[..prog.len()].copy_from_slice(prog);

	loop {
		cpu.execute_one();
	}
}
