use bitfield::prelude::*;

use crate::cpu::hart::WhiskerHart;

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

	fn to_sf_u8(self) -> u8 {
		match self {
			RoundingMode::RoundToNearestTieEven => softfloat_sys::softfloat_round_near_even,
			RoundingMode::RoundTowardsZero => softfloat_sys::softfloat_round_minMag,
			RoundingMode::RoundDown => softfloat_sys::softfloat_round_min,
			RoundingMode::RoundUp => softfloat_sys::softfloat_round_max,
			RoundingMode::RoundToNearestTiesMaxMagnitude => softfloat_sys::softfloat_round_near_maxMag,
			RoundingMode::Dynamic => unreachable!("dynamic should read from a CSR"),
			_ => unreachable!(),
		}
	}

	fn write_thread_local(self, hart: &mut WhiskerHart) {
		let rm = match self {
			RoundingMode::Dynamic => hart.float_status_control.get_rounding_mode(),
			rm => rm,
		};
		unsafe {
			softfloat_sys::softfloat_roundingMode_write_helper(rm.to_sf_u8());
		}
	}
}

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub struct ExceptionFlags(u8);

#[allow(unused)]
impl ExceptionFlags {
	pub const FLAG_INEXACT: u8 = softfloat_sys::softfloat_flag_inexact;
	pub const FLAG_UNDERFLOW: u8 = softfloat_sys::softfloat_flag_underflow;
	pub const FLAG_OVERFLOW: u8 = softfloat_sys::softfloat_flag_overflow;
	pub const FLAG_INFINITE: u8 = softfloat_sys::softfloat_flag_infinite;
	pub const FLAG_INVALID: u8 = softfloat_sys::softfloat_flag_invalid;

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

	pub fn update_hart(self, hart: &mut WhiskerHart) {
		todo!()
	}

	pub fn get_from_softfloat() -> Self {
		let val = unsafe { softfloat_sys::softfloat_exceptionFlags_read_helper() };
		Self(val)
	}
}
