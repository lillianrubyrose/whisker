use std::fmt::Debug;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

/// a valid register index 0..=31
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegisterIndex(u8);

impl RegisterIndex {
	pub fn new(idx: u8) -> Option<Self> {
		if idx <= 31 {
			Some(Self(idx))
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

impl Debug for RegisterIndex {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedExtensions(u64);

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

impl Default for SupportedExtensions {
	fn default() -> Self {
		Self::INTEGER
	}
}
