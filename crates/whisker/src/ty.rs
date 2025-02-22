use std::fmt::Debug;
use std::marker::{PhantomData, StructuralPartialEq};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

use crate::cpu::{FPRegisters, GPRegisters};

// Do we want to make specific structs for this?
// I think it's fine to use the <X>Registers struct since it's only-
// used for compiler help.
pub type GPRegisterIndex = RegisterIndex<GPRegisters>;
pub type FPRegisterIndex = RegisterIndex<FPRegisters>;
pub type UnknownRegisterIndex = RegisterIndex<()>;

/// a valid register index 0..=31
pub struct RegisterIndex<T>(u8, PhantomData<T>);

impl<T> Clone for RegisterIndex<T> {
	fn clone(&self) -> Self {
		*self
	}
}
impl<T> Copy for RegisterIndex<T> {}

impl<T> StructuralPartialEq for RegisterIndex<T> {}
impl<T> PartialEq for RegisterIndex<T> {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl<T> Eq for RegisterIndex<T> {}

impl<T> RegisterIndex<T> {
	pub fn new(idx: u8) -> Option<Self> {
		if idx <= 31 {
			Some(Self(idx, PhantomData))
		} else {
			None
		}
	}

	pub fn as_usize(&self) -> usize {
		// TODO: tell this to the optimizer better?
		debug_assert!(self.0 <= 31);
		usize::from(self.0 & 0b11111)
	}
}

impl UnknownRegisterIndex {
	pub fn to_gp(self) -> GPRegisterIndex {
		RegisterIndex(self.0, PhantomData)
	}

	pub fn to_fp(self) -> FPRegisterIndex {
		RegisterIndex(self.0, PhantomData)
	}
}

impl From<UnknownRegisterIndex> for GPRegisterIndex {
	fn from(value: UnknownRegisterIndex) -> Self {
		value.to_gp()
	}
}

impl From<UnknownRegisterIndex> for FPRegisterIndex {
	fn from(value: UnknownRegisterIndex) -> Self {
		value.to_fp()
	}
}

#[allow(unused)]
impl GPRegisterIndex {
	pub const ZERO: GPRegisterIndex = RegisterIndex(0, PhantomData);
	pub const LINK_REG: GPRegisterIndex = RegisterIndex(1, PhantomData);
	pub const SP: GPRegisterIndex = RegisterIndex(2, PhantomData);
	pub const GLOBAL_PTR: GPRegisterIndex = RegisterIndex(3, PhantomData);
	pub const THREAD_PTR: GPRegisterIndex = RegisterIndex(4, PhantomData);
}

impl Debug for UnknownRegisterIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Reg(")?;
		write!(f, "{})", self.0.to_string())
	}
}

impl Debug for GPRegisterIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Reg(")?;
		let r = match self.0 {
			0 => "zero",
			1 => "x1",
			2 => "x2",
			3 => "x3",
			4 => "x4",
			5 => "x5",
			6 => "x6",
			7 => "x7",
			8 => "x8",
			9 => "x9",
			10 => "x10",
			11 => "x11",
			12 => "x12",
			13 => "x13",
			14 => "x14",
			15 => "x15",
			16 => "x16",
			17 => "x17",
			18 => "x18",
			19 => "x19",
			20 => "x20",
			21 => "x21",
			22 => "x22",
			23 => "x23",
			24 => "x24",
			25 => "x25",
			26 => "x26",
			27 => "x27",
			28 => "x28",
			29 => "x29",
			30 => "x30",
			31 => "x31",
			_ => unreachable!(),
		};
		write!(f, "{})", r)
	}
}

impl Debug for FPRegisterIndex {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Reg(")?;
		write!(f, "f{})", self.0.to_string())
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

#[expect(unused, reason = "several of these extensions are reserved in the ISA")]
impl SupportedExtensions {
	pub const ATOMIC: Self = Self(1 << 0);
	pub const B: Self = Self(1 << 1);
	pub const COMPRESSED: Self = Self(1 << 2);
	pub const DOUBLE: Self = Self(1 << 3);
	pub const E: Self = Self(1 << 4);
	pub const FLOAT: Self = Self(1 << 5);
	pub const G_RESERVED: Self = Self(1 << 6);
	pub const HYPERVISOR: Self = Self(1 << 7);
	pub const INTEGER: Self = Self(1 << 8);
	pub const J_RESERVED: Self = Self(1 << 9);
	pub const K_RESERVED: Self = Self(1 << 10);
	pub const L_RESERVED: Self = Self(1 << 11);
	pub const MULTIPLY: Self = Self(1 << 12);
	pub const N_RESERVED: Self = Self(1 << 13);
	pub const O_RESERVED: Self = Self(1 << 14);
	pub const P_RESERVED: Self = Self(1 << 15);
	pub const QUAD_FLOAT: Self = Self(1 << 16);
	pub const R_RESERVED: Self = Self(1 << 17);
	pub const SUPERVISOR: Self = Self(1 << 18);
	pub const T_RESERVED: Self = Self(1 << 19);
	pub const USER_MODE: Self = Self(1 << 20);
	pub const VECTOR: Self = Self(1 << 21);
	pub const W_RESERVED: Self = Self(1 << 22);
	pub const NON_STANDARD: Self = Self(1 << 23);
	pub const Y_RESERVED: Self = Self(1 << 24);
	pub const Z_RESERVED: Self = Self(1 << 25);

	pub const fn empty() -> Self {
		SupportedExtensions(0)
	}

	pub const fn has(self, other: Self) -> bool {
		(self.0 & other.0) == other.0
	}

	pub const fn insert(&mut self, other: Self) -> &mut Self {
		self.0 |= other.0;
		self
	}

	pub const fn remove(&mut self, other: Self) -> &mut Self {
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

impl Default for SupportedExtensions {
	fn default() -> Self {
		Self::INTEGER & Self::FLOAT
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrapKind {
	Interrupt,
	Exception,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TrapIdx(u64);

#[allow(unused)]
impl TrapIdx {
	pub fn kind(&self) -> TrapKind {
		if self.0 & Self::INTERRUPT_MASK != 0 {
			TrapKind::Interrupt
		} else {
			TrapKind::Exception
		}
	}

	pub fn code(&self) -> u64 {
		self.0 & Self::CODE_MASK
	}

	pub fn inner(&self) -> u64 {
		self.0
	}
}

#[allow(unused)]
impl TrapIdx {
	pub const INTERRUPT_MASK: u64 = 0x80000000_00000000;
	pub const CODE_MASK: u64 = 0x7FFFFFFF_FFFFFFFF;

	pub const INSTRUCTION_ADDR_MISALIGNED: Self = Self(0);
	pub const INSTRUCTION_ACCESS_FAULT: Self = Self(1);
	pub const ILLEGAL_INSTRUCTION: Self = Self(2);
	pub const BREAKPOINT: Self = Self(3);
	pub const LOAD_ADDR_MISALIGNED: Self = Self(4);
	pub const STORE_ADDR_MISALIGNED: Self = Self(5);
	pub const STORE_ACCESS_FAULT: Self = Self(6);
	pub const ECALL_UMODE: Self = Self(7);
	pub const ECALL_SMODE: Self = Self(8);
	pub const ECALL_MMODE: Self = Self(10);
	pub const INSTRUCTION_PAGE_FAULT: Self = Self(12);
	pub const LOAD_PAGE_FAULT: Self = Self(13);
	pub const STORE_PAGE_FAULT: Self = Self(15);
	pub const SOFTWARE_CHECK: Self = Self(18);
	pub const HARDWARE_CHECK: Self = Self(19);
	pub const MEOW_ERR: Self = Self(31);
}
