use bitfield::prelude::*;

use crate::cpu::{csr::CSRReadToken, hart::WhiskerHart};

pub mod double;
pub mod float;

#[bitfields]
#[derive(Debug, Clone, Copy)]
pub struct FloatStatusControl {
	pub inexact: bool,
	pub underflow: bool,
	pub overflow: bool,
	pub div_by_zero: bool,
	pub invalid_operation: bool,
	pub rounding_mode: RoundingMode,
}

impl FloatStatusControl {
	pub fn set_from_fpu(&mut self, eflags: softfloat_pure::ExceptionFlags) {
		self.set_inexact(eflags.is_inexact());
		self.set_underflow(eflags.is_underflow());
		self.set_overflow(eflags.is_overflow());
		self.set_invalid_operation(eflags.is_invalid());
	}
}

/// Defined on unpriv isa page 119
#[derive(Debug, Clone, Copy)]
pub enum FClass {
	NegativeInfinity,
	NegativeNormal,
	NegativeSubnormal,
	NegativeZero,

	PositiveZero,
	PositiveSubNormal,
	PositiveNormal,
	PositiveInfinity,

	SignalingNaN,
	QuietNaN,
}

impl FClass {
	#[allow(unused)]
	pub const fn to_shift(self) -> u16 {
		match self {
			Self::NegativeInfinity => 1 << 0,
			Self::NegativeNormal => 1 << 1,
			Self::NegativeSubnormal => 1 << 2,
			Self::NegativeZero => 1 << 3,

			Self::PositiveZero => 1 << 4,
			Self::PositiveSubNormal => 1 << 5,
			Self::PositiveNormal => 1 << 6,
			Self::PositiveInfinity => 1 << 7,

			Self::SignalingNaN => 1 << 8,
			Self::QuietNaN => 1 << 9,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, BitFieldRepr)]
pub enum RoundingMode {
	/// Round to Nearest, ties to Even
	///
	/// Mnemonic: RNE
	RoundToNearestTieEven = 0,
	/// Round towards Zero
	///
	/// Mnemonic: RTZ
	RoundTowardsZero = 1,
	/// Round Down (towards neg infinity)
	///
	/// Mnemonic: RDN
	RoundDown = 2,
	/// Round Up (towards pos infinity)
	///
	/// Mnemonic: RUP
	RoundUp = 3,
	/// Round to Nearest, ties to Max Magnitude
	///
	/// Mnemonic: RMM
	RoundToNearestTiesMaxMagnitude = 4,

	/// In instruction’s rm field, selects dynamic rounding mode; In Rounding Mode register
	Dynamic = 7,
}

#[allow(unused)]
impl RoundingMode {
	pub const fn from_u8(value: u8) -> Option<Self> {
		match value {
			0b000 => Some(Self::RoundToNearestTieEven),
			0b001 => Some(Self::RoundTowardsZero),
			0b010 => Some(Self::RoundDown),
			0b011 => Some(Self::RoundUp),
			0b100 => Some(Self::RoundToNearestTiesMaxMagnitude),
			0b111 => Some(Self::Dynamic),
			_ => None,
		}
	}

	pub const fn as_u8(self) -> u8 {
		match self {
			RoundingMode::RoundToNearestTieEven => 0b000,
			RoundingMode::RoundTowardsZero => 0b001,
			RoundingMode::RoundDown => 0b010,
			RoundingMode::RoundUp => 0b011,
			RoundingMode::RoundToNearestTiesMaxMagnitude => 0b100,
			RoundingMode::Dynamic => 0b111,
		}
	}

	pub fn to_sf(self, hart: &WhiskerHart) -> softfloat_pure::RoundingMode {
		match self {
			RoundingMode::Dynamic => hart.float_status_control.get_rounding_mode().to_sf(hart),
			RoundingMode::RoundToNearestTieEven => softfloat_pure::RoundingMode::RneTiesToEven,
			RoundingMode::RoundTowardsZero => softfloat_pure::RoundingMode::RtzTowardZero,
			RoundingMode::RoundDown => softfloat_pure::RoundingMode::RdnTowardNegative,
			RoundingMode::RoundUp => softfloat_pure::RoundingMode::RupTowardPositive,
			RoundingMode::RoundToNearestTiesMaxMagnitude => softfloat_pure::RoundingMode::RmmTiesToAway,
		}
	}
}

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub struct ExceptionFlags(u8);

#[allow(unused)]
impl ExceptionFlags {
	pub const FLAG_INEXACT: u8 = 1;
	pub const FLAG_UNDERFLOW: u8 = 2;
	pub const FLAG_OVERFLOW: u8 = 4;
	pub const FLAG_INFINITE: u8 = 8;
	pub const FLAG_INVALID: u8 = 16;

	pub fn is_inexact(self) -> bool {
		self.0 & Self::FLAG_INEXACT != 0
	}

	pub fn is_infinite(self) -> bool {
		self.0 & Self::FLAG_INFINITE != 0
	}

	pub fn is_overflow(self) -> bool {
		self.0 & Self::FLAG_OVERFLOW != 0
	}

	pub fn is_underflow(self) -> bool {
		self.0 & Self::FLAG_UNDERFLOW != 0
	}

	pub fn is_invalid(self) -> bool {
		self.0 & Self::FLAG_INVALID != 0
	}
}
