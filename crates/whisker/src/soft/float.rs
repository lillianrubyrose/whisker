use std::cmp::Ordering;

use softfloat_sys::float32_t;

use super::{FClass, RoundingMode};

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
#[allow(unused)]
pub struct SoftFloat(float32_t);

#[allow(unused)]
impl SoftFloat {
	pub const fn from_f32(value: f32) -> Self {
		Self(float32_t { v: value.to_bits() })
	}

	pub const fn to_f32(self) -> f32 {
		f32::from_bits(self.0.v)
	}

	pub fn from_u32(value: u32) -> Self {
		Self(float32_t { v: value })
	}

	pub fn to_u32(self) -> u32 {
		self.0.v
	}

	pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
		Self(float32_t {
			v: u32::from_le_bytes(bytes),
		})
	}

	pub fn to_le_bytes(self) -> [u8; 4] {
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
		Self::get_exponent(self.0.v) == Self::EXPONENT_BITS && Self::get_mantissa(self.0.v) != 0u32
	}

	pub fn add(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_add(self.0, other.0) })
	}

	pub fn sub(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_sub(self.0, other.0) })
	}

	pub fn mul(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_mul(self.0, other.0) })
	}

	pub fn div(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_div(self.0, other.0) })
	}

	pub fn rem(&self, other: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_rem(self.0, other.0) })
	}

	pub fn mul_add(&self, mul: &Self, add: &Self, rm: RoundingMode) -> Self {
		unsafe { rm.write_thread_local() };
		Self(unsafe { softfloat_sys::f32_mulAdd(self.0, mul.0, add.0) })
	}
}

#[allow(unused)]
impl SoftFloat {
	const BITS: u32 = 32;
	const MANTISSA_BITS: u32 = 23;
	const EXPONENT_BITS: u32 = Self::BITS - Self::MANTISSA_BITS - 1;

	const MANTISSA_MASK: u32 = (1 << Self::MANTISSA_BITS) - 1;
	const EXPONENT_MASK: u32 = (1 << Self::EXPONENT_BITS) - 1;

	const QUIET_NAN_MASK: u32 = 1 << (Self::MANTISSA_BITS - 1);

	const fn get_sign(value: u32) -> u32 {
		value >> (Self::EXPONENT_BITS + Self::MANTISSA_BITS)
	}

	const fn get_exponent(value: u32) -> u32 {
		(value >> Self::MANTISSA_BITS) & Self::EXPONENT_MASK
	}

	const fn get_mantissa(value: u32) -> u32 {
		value & Self::MANTISSA_MASK
	}
}

impl Default for SoftFloat {
	fn default() -> Self {
		Self::from_f32(0_f32)
	}
}

impl PartialEq for SoftFloat {
	fn eq(&self, other: &Self) -> bool {
		unsafe { softfloat_sys::f32_eq(self.0, other.0) }
	}
}

impl PartialOrd for SoftFloat {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.is_nan() || other.is_nan() {
			None
		} else if unsafe { softfloat_sys::f32_eq(self.0, other.0) } {
			Some(Ordering::Equal)
		} else if unsafe { softfloat_sys::f32_lt(self.0, other.0) } {
			Some(Ordering::Less)
		} else {
			Some(Ordering::Greater)
		}
	}
}
