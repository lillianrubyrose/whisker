use softfloat_pure::{FPU, float32_t};

use super::{FClass, RoundingMode};
use crate::cpu::hart::WhiskerHart;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
#[allow(unused)]
pub struct SoftFloat(u32);

impl SoftFloat {
	const BITS: u32 = 32;
	const MANTISSA_BITS: u32 = 23;
	const EXPONENT_BITS: u32 = Self::BITS - Self::MANTISSA_BITS - 1;

	const MANTISSA_MASK: u32 = (1 << Self::MANTISSA_BITS) - 1;
	const EXPONENT_MASK: u32 = (1 << Self::EXPONENT_BITS) - 1;

	const QUIET_NAN_MASK: u32 = 1 << (Self::MANTISSA_BITS - 1);

	pub const fn from_f32(value: f32) -> Self {
		Self(value.to_bits())
	}

	pub const fn from_u32(value: u32) -> Self {
		Self(value)
	}

	pub const fn qnan() -> Self {
		Self(0x7fc00000)
	}

	pub const fn to_u32(self) -> u32 {
		self.0
	}

	pub const fn from_le_bytes(bytes: [u8; 4]) -> Self {
		Self(u32::from_le_bytes(bytes))
	}

	pub const fn to_le_bytes(self) -> [u8; 4] {
		self.0.to_le_bytes()
	}

	pub const fn fclass(self) -> FClass {
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

	pub const fn is_nan(self) -> bool {
		Self::get_exponent(self.0) == Self::EXPONENT_MASK && Self::get_mantissa(self.0) != 0u32
	}

	pub const fn is_snan(self) -> bool {
		softfloat_pure::softfloat::softfloat_isSigNaNF32UI(self.0)
	}

	#[allow(unused)]
	pub const fn is_qnan(self) -> bool {
		self.is_nan() && (Self::get_mantissa(self.0) & Self::QUIET_NAN_MASK == 0)
	}

	pub const fn sign(self) -> u32 {
		Self::get_sign(self.to_u32())
	}

	pub const fn set_sign(self, sign: u32) -> Self {
		Self::from_u32(
			(self.to_u32() & !(1 << (Self::EXPONENT_BITS + Self::MANTISSA_BITS)))
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

	pub fn max(self, other: Self, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let res = fpu.max(float32_t { v: self.0 }, float32_t { v: other.0 });
		hart.float_status_control.set_from_fpu(fpu.flags);
		return Self::from_u32(res.v);
	}

	pub fn min(self, other: Self, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let res = fpu.min(float32_t { v: self.0 }, float32_t { v: other.0 });
		hart.float_status_control.set_from_fpu(fpu.flags);
		return Self::from_u32(res.v);
	}

	pub fn add(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float32_t::from_bits(self.0);
		let rhs = float32_t::from_bits(other.0);
		let result = fpu.add(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn sub(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float32_t::from_bits(self.0);
		let rhs = float32_t::from_bits(other.0);
		let result = fpu.sub(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn mul(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float32_t::from_bits(self.0);
		let rhs = float32_t::from_bits(other.0);
		let result = fpu.mul(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn div(self, other: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float32_t::from_bits(self.0);
		let rhs = float32_t::from_bits(other.0);
		let result = fpu.div(lhs, rhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn sqrt(self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let lhs = float32_t::from_bits(self.0);
		let result = fpu.sqrt(lhs, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn mul_add(self, mul: Self, add: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let this = float32_t::from_bits(self.0);
		let mul = float32_t::from_bits(mul.0);
		let add = float32_t::from_bits(add.0);
		let result = fpu.mul_add(this, mul, add, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v)
	}

	pub fn mul_sub(self, mul: Self, sub: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		self.mul_add(mul, sub.neg(hart), rm, hart)
	}

	pub fn neg_mul_add(self, mul: Self, add: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		let mut fpu = FPU::default();
		let this = float32_t::from_bits(self.0);
		let mul = float32_t::from_bits(mul.0);
		let add = float32_t::from_bits(add.0);
		let result = fpu.mul_add(this, mul, add, rm.to_sf(hart));
		hart.float_status_control.set_from_fpu(fpu.flags);
		Self::from_u32(result.v).neg(hart)
	}

	pub fn neg_mul_sub(self, mul: Self, sub: Self, rm: RoundingMode, hart: &mut WhiskerHart) -> Self {
		self.neg_mul_add(mul, sub.neg(hart), rm, hart)
	}

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
