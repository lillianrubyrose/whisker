use std::fmt::Debug;
use std::marker::PhantomData;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

// Do we want to make specific structs for this?
// I think it's fine to use the <X>Registers struct since it's only-
// used for compiler help.
pub type GPRegisterIndex = RegisterIndex<GPRegsIdx>;
pub type FPRegisterIndex = RegisterIndex<FPRegsIdx>;
pub type UnknownRegisterIndex = RegisterIndex<()>;

/// a valid register index 0..=31
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegisterIndex<T>(u8, PhantomData<T>);

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

impl<T> RegisterIndex<T> {
	pub const ZERO: RegisterIndex<T> = RegisterIndex(0, PhantomData);
}

#[allow(unused)]
impl GPRegisterIndex {
	pub const LINK_REG: GPRegisterIndex = RegisterIndex(1, PhantomData);
	pub const SP: GPRegisterIndex = RegisterIndex(2, PhantomData);
	pub const GLOBAL_PTR: GPRegisterIndex = RegisterIndex(3, PhantomData);
	pub const THREAD_PTR: GPRegisterIndex = RegisterIndex(4, PhantomData);

	pub fn display(&self) -> &'static str {
		match self.0 {
			0 => "zero",
			1 => "ra",
			2 => "sp",
			3 => "gp",
			4 => "tp",
			5 => "t0",
			6 => "t1",
			7 => "t2",
			8 => "s0",
			9 => "s1",
			10 => "a0",
			11 => "a1",
			12 => "a2",
			13 => "a3",
			14 => "a4",
			15 => "a5",
			16 => "a6",
			17 => "a7",
			18 => "s2",
			19 => "s3",
			20 => "s4",
			21 => "s5",
			22 => "s6",
			23 => "s7",
			24 => "s8",
			25 => "s9",
			26 => "s10",
			27 => "s11",
			28 => "t3",
			29 => "t4",
			30 => "t5",
			31 => "t6",
			_ => unreachable!(),
		}
	}
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
			1 => "ra",
			2 => "sp",
			3 => "gp",
			4 => "tp",
			5 => "t0",
			6 => "t1",
			7 => "t2",
			8 => "s0",
			9 => "s1",
			10 => "a0",
			11 => "a1",
			12 => "a2",
			13 => "a3",
			14 => "a4",
			15 => "a5",
			16 => "a6",
			17 => "a7",
			18 => "s2",
			19 => "s3",
			20 => "s4",
			21 => "s5",
			22 => "s6",
			23 => "s7",
			24 => "s8",
			25 => "s9",
			26 => "s10",
			27 => "s11",
			28 => "t3",
			29 => "t4",
			30 => "t5",
			31 => "t6",
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

/// these exist to allow the generic RegisterIndex to derive things without needing the underlying register
/// container type to derive things
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GPRegsIdx {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FPRegsIdx {}
