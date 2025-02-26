pub mod double;
pub mod float;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoundingMode {
	/// Round to Nearest, ties to Even
	///
	/// Mnemonic: RNE
	RoundToNearestTieEven,
	/// Round towards Zero
	///
	/// Mnemonic: RTZ
	RoundTowardsZero,
	/// Round Down (towards neg infinity)
	///
	/// Mnemonic: RDN
	RoundDown,
	/// Round Up (towards pos infinity)
	///
	/// Mnemonic: RUP
	RoundUp,
	/// Round to Nearest, ties to Max Magnitude
	///
	/// Mnemonic: RMM
	RoundToNearestTiesMaxMagnitude,

	#[allow(unused)]
	Reserved,
	#[allow(unused)]
	Reserved2,

	/// In instruction’s rm field, selects dynamic rounding mode; In Rounding Mode register, reserved
	#[allow(unused)]
	ReservedDynamic,
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
			// Definitely shouldn't parse the reserved ones
			_ => None,
		}
	}

	pub const fn to_u8(self) -> u8 {
		match self {
			RoundingMode::RoundToNearestTieEven => 0b000,
			RoundingMode::RoundTowardsZero => 0b001,
			RoundingMode::RoundDown => 0b010,
			RoundingMode::RoundUp => 0b011,
			RoundingMode::RoundToNearestTiesMaxMagnitude => 0b100,
			// Should these even be defined?
			RoundingMode::Reserved => 0b101,
			RoundingMode::Reserved2 => 0b110,
			RoundingMode::ReservedDynamic => 0b110,
		}
	}

	const fn to_sf_u8(self) -> u8 {
		match self {
			RoundingMode::RoundToNearestTieEven => softfloat_sys::softfloat_round_near_even,
			RoundingMode::RoundTowardsZero => softfloat_sys::softfloat_round_minMag,
			RoundingMode::RoundDown => softfloat_sys::softfloat_round_min,
			RoundingMode::RoundUp => softfloat_sys::softfloat_round_max,
			RoundingMode::RoundToNearestTiesMaxMagnitude => softfloat_sys::softfloat_round_near_maxMag,
			_ => unreachable!(),
		}
	}

	/// This should NEVER be used outside of the `soft` module, hence marked as unsafe
	pub unsafe fn write_thread_local(self) {
		unsafe {
			softfloat_sys::softfloat_roundingMode_write_helper(self.to_sf_u8());
		}
	}
}

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub struct ExceptionFlags(u8);

#[allow(unused)]
impl ExceptionFlags {
	const FLAG_INEXACT: u8 = softfloat_sys::softfloat_flag_inexact;
	const FLAG_INFINITE: u8 = softfloat_sys::softfloat_flag_infinite;
	const FLAG_OVERFLOW: u8 = softfloat_sys::softfloat_flag_overflow;
	const FLAG_UNDERFLOW: u8 = softfloat_sys::softfloat_flag_underflow;
	const FLAG_INVALID: u8 = softfloat_sys::softfloat_flag_invalid;

	pub fn is_inexact(&self) -> bool {
		self.0 & Self::FLAG_INEXACT != 0
	}

	pub fn is_infinite(&self) -> bool {
		self.0 & Self::FLAG_INFINITE != 0
	}

	pub fn is_overflow(&self) -> bool {
		self.0 & Self::FLAG_OVERFLOW != 0
	}

	pub fn is_underflow(&self) -> bool {
		self.0 & Self::FLAG_UNDERFLOW != 0
	}

	pub fn is_invalid(&self) -> bool {
		self.0 & Self::FLAG_INVALID != 0
	}

	pub fn fetch(&mut self) {
		let val = unsafe { softfloat_sys::softfloat_exceptionFlags_read_helper() };
		self.0 = val;
	}
}
