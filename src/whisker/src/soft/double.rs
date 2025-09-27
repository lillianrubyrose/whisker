use std::cmp::Ordering;

use softfloat_pure::{FPU, float64_t};

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

	pub fn is_snan(self) -> bool {
		softfloat_pure::softfloat::softfloat_isSigNaNF64UI(self.0)
	}
	pub fn is_qnan(self) -> bool {
		self.is_nan() && (Self::get_mantissa(self.0) & Self::QUIET_NAN_MASK == 0)
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

	pub fn mul_sub(self, mul: Self, sub: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		self.mul_add(mul, sub.neg(hart), rm, hart)
	}

	pub fn is_positive(self) -> bool {
		Self::get_sign(self.to_u64()) == 0
	}

	pub fn sign(self) -> u64 {
		Self::get_sign(self.to_u64())
	}

	pub fn set_sign(self, sign: u64) -> Self {
		Self::from_u64(
			(self.to_u64() & !(1 << (Self::EXPONENT_BITS + Self::MANTISSA_BITS)))
				| ((sign & 1) << (Self::EXPONENT_BITS + Self::MANTISSA_BITS)),
		)
	}

	pub fn neg(self, hart: &mut WhiskerHart) -> Self {
		if self.is_nan() {
			if self.is_snan() {
				hart.float_status_control.set_invalid_operation(true);
			}
			return self;
		}
		Self(self.0 ^ (1 << (u32::BITS - 1)))
	}

	fn min_max_handle_nan(lhs: SoftDouble, rhs: SoftDouble, hart: &mut WhiskerHart) -> SoftDouble {
		if lhs.is_snan() || rhs.is_snan() {
			hart.float_status_control.set_invalid_operation(true);
		}

		if lhs.is_nan() {
			if rhs.is_nan() {
				SoftDouble((SoftDouble::EXPONENT_MASK << SoftDouble::MANTISSA_BITS) | SoftDouble::QUIET_NAN_MASK)
			} else {
				rhs
			}
		} else {
			lhs
		}
	}

	pub fn max(self, other: Self, hart: &mut WhiskerHart) -> Self {
		if self.is_nan() || other.is_nan() {
			return Self::min_max_handle_nan(self, other, hart);
		}

		let lhs_positive = self.is_positive();
		let rhs_positive = other.is_positive();

		if lhs_positive != rhs_positive {
			if !lhs_positive {
				return other;
			}
			return self;
		}

		if self.gt(&other) { self } else { other }
	}

	pub fn min(self, other: Self, hart: &mut WhiskerHart) -> Self {
		if self.is_nan() || other.is_nan() {
			return Self::min_max_handle_nan(self, other, hart);
		}

		let lhs_positive = self.is_positive();
		let rhs_positive = other.is_positive();

		if lhs_positive != rhs_positive {
			if !lhs_positive {
				return self;
			}
			return other;
		}

		if self.lt(&other) { self } else { other }
	}
}

impl SoftDouble {
	pub fn add(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		let result = fpu.add(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn sub(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		let result = fpu.sub(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn mul(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		let result = fpu.mul(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn div(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		let result = fpu.div(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn rem(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		let result = fpu.rem(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn mul_add(self, mul: Self, add: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let this = float64_t::from_bits(self.0);
		let mul = float64_t::from_bits(mul.0);
		let add = float64_t::from_bits(add.0);
		let result = fpu.mul_add(this, mul, add, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
	}

	pub fn sqrt(self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let result = fpu.sqrt(lhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u64(result.v)
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
		let mut fpu = FPU::default();
		let lhs = float64_t::from_bits(self.0);
		let rhs = float64_t::from_bits(other.0);
		fpu.eq(lhs, rhs)
	}
}

impl PartialOrd for SoftDouble {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		if self.is_nan() || other.is_nan() {
			None
		} else if self.eq(other) {
			Some(Ordering::Equal)
		} else if {
			let mut fpu = FPU::default();
			let lhs = float64_t::from_bits(self.0);
			let rhs = float64_t::from_bits(other.0);
			fpu.lt(lhs, rhs)
		} {
			Some(Ordering::Less)
		} else {
			Some(Ordering::Greater)
		}
	}
}
