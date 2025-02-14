use std::fmt::Debug;

/// a valid register index 0..=31
pub struct RegisterIndex(u8);

impl RegisterIndex {
	pub fn new(idx: u8) -> Option<Self> {
		if idx <= 31 {
			Some(Self(idx))
		} else {
			None
		}
	}

	pub fn inner(&self) -> u8 {
		self.0
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
