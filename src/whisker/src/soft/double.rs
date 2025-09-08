use std::cmp::Ordering;

use super::{FClass, RoundingMode};
use crate::cpu::hart::WhiskerHart;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct SoftDouble(u64);

impl SoftDouble {
	pub const fn from_f64(value: f64) -> Self {
		Self(value.to_bits())
	}

	pub const fn to_f64(self) -> f64 {
		f64::from_bits(self.0)
	}

	pub fn from_u64(value: u64) -> Self {
		Self(value)
	}

	pub fn to_u64(self) -> u64 {
		self.0
	}

	pub fn to_le_bytes(self) -> [u8; 8] {
		self.0.to_le_bytes()
	}

	pub fn from_le_bytes(bytes: [u8; 8]) -> Self {
		Self(u64::from_le_bytes(bytes))
	}

	pub fn is_nan(self) -> bool {
		Self::get_exponent(self.0) == Self::EXPONENT_BITS && Self::get_mantissa(self.0) != 0u64
	}

	pub fn fclass(self) -> FClass {
		let sign = Self::get_sign(self.0);
		let exponent = Self::get_exponent(self.0);
		let mantissa = Self::get_mantissa(self.0);

		if exponent == Self::EXPONENT_MASK {
			if mantissa == 0 {
				if sign == 0 {
					FClass::PositiveInfinity
				} else {
					FClass::NegativeInfinity
				}
			} else if (mantissa & Self::QUIET_NAN_MASK) == 0 {
				FClass::SignalingNaN
			} else {
				FClass::QuietNaN
			}
		} else if exponent == 0 {
			if mantissa == 0 {
				if sign == 0 {
					FClass::PositiveZero
				} else {
					FClass::NegativeZero
				}
			} else if sign == 0 {
				FClass::PositiveSubNormal
			} else {
				FClass::NegativeSubnormal
			}
		} else if sign == 0 {
			FClass::PositiveNormal
		} else {
			FClass::NegativeNormal
		}
	}
}

#[allow(dead_code, reason = "FIXME: Finish FP instruction implementations")]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl SoftDouble {
	pub fn add(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_add(self.0.into(), other.0.into()) }.v)
	}

	pub fn sub(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_sub(self.0.into(), other.0.into()) }.v)
	}

	pub fn mul(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_mul(self.0.into(), other.0.into()) }.v)
	}

	pub fn div(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_div(self.0.into(), other.0.into()) }.v)
	}

	pub fn rem(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_rem(self.0.into(), other.0.into()) }.v)
	}

	pub fn mul_add(self, mul: Self, add: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_mulAdd(self.0.into(), mul.0.into(), add.0.into()) }.v)
	}

	pub fn sqrt(self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		rm.write_thread_local(hart);
		Self(unsafe { softfloat_sys::f64_sqrt(self.0.into()) }.v)
	}
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
impl SoftDouble {
	pub fn add(self, other: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() + other.to_f64())
	}

	pub fn sub(self, other: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() - other.to_f64())
	}

	pub fn mul(self, other: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() * other.to_f64())
	}

	pub fn div(self, other: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() / other.to_f64())
	}

	pub fn rem(self, other: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() % other.to_f64())
	}

	pub fn mul_add(self, mul: Self, add: Self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64() * mul.to_f64() + add.to_f64())
	}

	pub fn sqrt(self, _rm: RoundingMode, _hart: &mut WhiskerHart) -> Self {
		Self::from_f64(self.to_f64().sqrt())
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
		#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
		unsafe {
			softfloat_sys::f64_eq(self.0.into(), other.0.into())
		}
		#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
		self.to_f64().eq(&other.to_f64())
	}
}

impl PartialOrd for SoftDouble {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.is_nan() || other.is_nan() {
			None
		} else if self.eq(other) {
			Some(Ordering::Equal)
		} else if {
			#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
			unsafe {
				softfloat_sys::f64_lt(self.0.into(), other.0.into())
			}
			#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
			{
				self.to_f64() < other.to_f64()
			}
		} {
			Some(Ordering::Less)
		} else {
			Some(Ordering::Greater)
		}
	}
}
