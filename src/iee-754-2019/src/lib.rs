mod f32;

pub use f32::F32;

// These correspond to the rounding-direction attributes defined in Section 4.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingMode {
	/// RNE - Rounds to the nearest value, with ties rounded to the nearest value with an even least significant bit.
	/// Reference: Section 4.3.1
	#[default]
	RoundTiesToEven,

	/// RMM - Rounds to the nearest value, with ties rounded away from zero.
	/// Reference: Section 4.3.1
	RoundTiesToAway,

	/// RUP - Rounds towards positive infinity.
	/// Reference: Section 4.3.2
	RoundTowardPositive,

	/// RDN - Rounds towards negative infinity.
	/// Reference: Section 4.3.2
	RoundTowardNegative,

	/// RTZ - Truncates the result.
	/// Reference: Section 4.3.2
	RoundTowardZero,
}

impl RoundingMode {
	pub const fn const_eq(self, other: RoundingMode) -> bool {
		match (self, other) {
			(RoundingMode::RoundTiesToEven, RoundingMode::RoundTiesToEven) => true,
			(RoundingMode::RoundTiesToAway, RoundingMode::RoundTiesToAway) => true,
			(RoundingMode::RoundTowardPositive, RoundingMode::RoundTowardPositive) => true,
			(RoundingMode::RoundTowardNegative, RoundingMode::RoundTowardNegative) => true,
			(RoundingMode::RoundTowardZero, RoundingMode::RoundTowardZero) => true,
			_ => false,
		}
	}
}

/// These correspond to the status flags in Section 7.1
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExceptionFlags {
	/// Invalid Operation (NV) - An operation was performed on invalid operands (e.g., sqrt(-1), inf - inf).
	/// Reference: Section 7.2
	pub invalid_operation: bool,

	/// Division by Zero (DZ) - A finite non-zero number was divided by zero.
	/// Reference: Section 7.3
	pub div_by_zero: bool,

	/// Overflow (OF) - The result exceeded the format's largest finite number.
	/// Reference: Section 7.4
	pub overflow: bool,

	/// Underflow (UF) - The result is tiny and may have lost accuracy.
	/// Reference: Section 7.5
	pub underflow: bool,

	/// Inexact (NX) - The result of the operation is not exact.
	/// Reference: Section 7.6
	pub inexact: bool,
}

impl ExceptionFlags {
	pub const fn new() -> Self {
		Self {
			invalid_operation: false,
			div_by_zero: false,
			overflow: false,
			underflow: false,
			inexact: false,
		}
	}

	#[inline(always)]
	pub(crate) const fn invalid(&mut self) {
		self.invalid_operation = true;
	}

	#[inline(always)]
	pub(crate) const fn div_by_zero(&mut self) {
		self.div_by_zero = true;
	}

	#[inline(always)]
	pub(crate) const fn overflow(&mut self) {
		self.overflow = true;
	}

	#[inline(always)]
	pub(crate) const fn underflow(&mut self) {
		self.underflow = true;
	}

	#[inline(always)]
	pub(crate) const fn inexact(&mut self) {
		self.inexact = true;
	}
}
