use std::cmp::Ordering;

use softfloat_sys::float64_t;

use super::{FClass, RoundingMode};

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct SoftDouble(float64_t);

#[allow(unused)]
impl SoftDouble {
	pub const fn from_f64(value: f64) -> Self {
		Self(float64_t { v: value.to_bits() })
	}

	pub const fn to_f64(self) -> f64 {
		f64::from_bits(self.0.v)
	}

	pub fn from_u64(value: u64) -> Self {
		Self(float64_t { v: value })
	}

	pub fn to_u64(self) -> u64 {
		self.0.v
	}

	pub fn from_le_bytes(bytes: [u8; 8]) -> Self {
		Self(float64_t {
			v: u64::from_le_bytes(bytes),
		})
	}

	pub fn to_le_bytes(self) -> [u8; 8] {
		self.0.v.to_le_bytes()
	}

	pub fn fclass(self) -> FClass {
		let sign = Self::get_sign(self.0.v);
		let exponent = Self::get_exponent(self.0.v);
		let mantissa = Self::get_mantissa(self.0.v);

		if exponent == Self::EXPONENT_MASK {
			if mantissa == 0 {
				if sign == 0 {
					FClass::PositiveInfinity
				} else {
					FClass::NegativeInfinity
				}
			} else {
				if (mantissa & Self::QUIET_NAN_MASK) == 0 {
					FClass::SignalingNaN
				} else {
					FClass::QuietNaN
				}
			}
		} else if exponent == 0 {
			if mantissa == 0 {
				if sign == 0 {
					FClass::PositiveZero
				} else {
					FClass::NegativeZero
				}
			} else {
				if sign == 0 {
					FClass::PositiveSubNormal
				} else {
					FClass::NegativeSubnormal
				}
			}
		} else {
			if sign == 0 {
				FClass::PositiveNormal
			} else {
				FClass::NegativeNormal
			}
		}
	}

	pub fn is_nan(&self) -> bool {
		Self::get_exponent(self.0.v) == Self::EXPONENT_BITS && Self::get_mantissa(self.0.v) != 0u64
	}

	pub fn add(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_add(self.0, other.0) })
	}

	pub fn sub(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_sub(self.0, other.0) })
	}

	pub fn mul(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_mul(self.0, other.0) })
	}

	pub fn div(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_div(self.0, other.0) })
	}

	pub fn rem(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_rem(self.0, other.0) })
	}

	pub fn mul_add(&self, mul: &Self, add: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f64_mulAdd(self.0, mul.0, add.0) })
	}
}

#[allow(unused)]
impl SoftDouble {
	const BITS: u64 = 64;
	const MANTISSA_BITS: u64 = 52;
	const EXPONENT_BITS: u64 = Self::BITS - Self::MANTISSA_BITS - 1;

	const MANTISSA_MASK: u64 = (1 << Self::MANTISSA_BITS) - 1;
	const EXPONENT_MASK: u64 = (1 << Self::EXPONENT_BITS) - 1;

	const QUIET_NAN_MASK: u64 = 1 << (Self::MANTISSA_BITS - 1);

	const fn get_sign(value: u64) -> u64 {
		value >> (Self::EXPONENT_BITS + Self::MANTISSA_BITS)
	}

	const fn get_exponent(value: u64) -> u64 {
		(value >> Self::MANTISSA_BITS) & Self::EXPONENT_MASK
	}

	const fn get_mantissa(value: u64) -> u64 {
		value & Self::MANTISSA_MASK
	}
}

impl Default for SoftDouble {
	fn default() -> Self {
		Self::from_f64(0_f64)
	}
}

impl PartialEq for SoftDouble {
	fn eq(&self, other: &Self) -> bool {
		unsafe { softfloat_sys::f64_eq(self.0, other.0) }
	}
}

impl PartialOrd for SoftDouble {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.is_nan() || other.is_nan() {
			None
		} else if unsafe { softfloat_sys::f64_eq(self.0, other.0) } {
			Some(Ordering::Equal)
		} else if unsafe { softfloat_sys::f64_lt(self.0, other.0) } {
			Some(Ordering::Less)
		} else {
			Some(Ordering::Greater)
		}
	}
}
