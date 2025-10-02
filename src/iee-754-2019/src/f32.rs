use crate::{
	ExceptionFlags, RoundingMode,
	RoundingMode::{RoundTiesToAway, RoundTiesToEven, RoundTowardNegative, RoundTowardPositive, RoundTowardZero},
};

pub struct F32 {
	sign: u32,
	exponent: u32,
	fraction: u32,
}

impl From<u32> for F32 {
	fn from(value: u32) -> Self {
		Self {
			sign: (value >> 31) & 1,
			exponent: (value >> 23) & 0xFF,
			fraction: value & 0x7FFFFF,
		}
	}
}

impl Into<u32> for F32 {
	fn into(self) -> u32 {
		(self.sign << 31) | (self.exponent << 23) | self.fraction
	}
}

impl F32 {
	const Q_NAN: u32 = 0x7FC00000;

	pub const fn from_u32(value: u32) -> Self {
		Self {
			sign: (value >> 31) & 1,
			exponent: (value >> 23) & 0xFF,
			fraction: value & 0x7FFFFF,
		}
	}

	pub const fn into_u32(self) -> u32 {
		(self.sign << 31) | (self.exponent << 23) | self.fraction
	}

	const fn is_nan(&self) -> bool {
		self.exponent == 0xFF && self.fraction != 0
	}

	const fn is_signaling_nan(&self) -> bool {
		self.is_nan() && (self.fraction & 0x400000) == 0
	}

	const fn is_infinity(&self) -> bool {
		self.exponent == 0xFF && self.fraction == 0
	}

	const fn is_zero(&self) -> bool {
		self.exponent == 0 && self.fraction == 0
	}

	const fn is_subnormal(&self) -> bool {
		self.exponent == 0 && self.fraction != 0
	}
}

impl F32 {
	pub const fn add(lhs_bits: u32, rhs_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);

		let mut flags = ExceptionFlags::new();

		// =============
		// Special Cases
		// Reference: Section 6
		// =============

		// Either sNaN: invalid operation and return qNaN
		// Section 7.2
		if lhs.is_signaling_nan() {
			flags.invalid();
			return (lhs_bits | 0x400000, flags);
		}

		if rhs.is_signaling_nan() {
			flags.invalid();
			return (rhs_bits | 0x400000, flags);
		}

		// Either NaN: return qNaN
		// Section 6.2
		if lhs.is_nan() || rhs.is_nan() {
			return (Self::Q_NAN, flags);
		}

		if lhs.is_infinity() && rhs.is_infinity() && lhs.sign == rhs.sign {
			return (lhs_bits, flags);
		}

		if lhs.is_infinity() && rhs.is_infinity() {
			return if lhs.sign == rhs.sign {
				// Both infinity with same sign: return the infinity
				// Section 6.1
				(lhs_bits, flags)
			} else {
				// Both infinity with different signs: invalid operation and return qNaN
				// Section 7.2
				flags.invalid();
				(Self::Q_NAN, flags)
			};
		}

		// Either infinity but not both: return the infinity
		if lhs.is_infinity() {
			return (lhs_bits, flags);
		}

		if rhs.is_infinity() {
			return (rhs_bits, flags);
		}

		// Section 6.3
		if lhs.is_zero() && rhs.is_zero() {
			// Signs are different: +0 unless RoundTowardNegative
			if lhs.sign != rhs.sign {
				return (
					if rm.const_eq(RoundTowardNegative) {
						0x80000000
					} else {
						0
					},
					flags,
				);
			}

			// Signs are same: return +0
			return (lhs_bits, flags);
		}

		// Either zero but not both: return the opposite operand
		if lhs.is_zero() {
			return (rhs_bits, flags);
		}

		if rhs.is_zero() {
			return (lhs_bits, flags);
		}

		// =============
		// Extract components and add the implicit leading bit for normal numbers
		// Reference: 3.4
		// =============

		let mut exponent_lhs = lhs.exponent.cast_signed();
		let mut exponent_rhs = rhs.exponent.cast_signed();

		// Significand is 24 bits, extend temporarily so we don't overflow when rounding/shifting
		let mut significand_lhs = lhs.fraction as u64;
		let mut significand_rhs = rhs.fraction as u64;

		if lhs.is_subnormal() {
			exponent_lhs = 1;
		} else {
			significand_lhs |= 1 << 23;
		}

		if rhs.is_subnormal() {
			exponent_rhs = 1;
		} else {
			significand_rhs |= 1 << 23;
		}

		// guard, round, and sticky bits
		significand_lhs <<= 3;
		significand_rhs <<= 3;

		// =============
		// Align exponents
		// In addition the exponents must be the same.
		// Shift the significand of the smaller number right and increment its exponent until they match.
		// =============

		let exponent_difference = (exponent_lhs - exponent_rhs).abs().cast_unsigned();

		if exponent_lhs < exponent_rhs {
			let sticky = if exponent_difference >= 64 {
				significand_lhs != 0
			} else {
				(significand_lhs & ((1 << exponent_difference) - 1)) != 0
			};
			significand_lhs = (significand_lhs >> exponent_difference) | if sticky { 1 } else { 0 };
		} else if exponent_rhs < exponent_lhs {
			let sticky = if exponent_difference >= 64 {
				significand_rhs != 0
			} else {
				(significand_rhs & ((1 << exponent_difference) - 1)) != 0
			};
			significand_rhs = (significand_rhs >> exponent_difference) | if sticky { 1 } else { 0 };
		}

		// ========================
		// Add/subtract significand
		// ========================

		let result_sign;
		let mut result_significand;

		if lhs.sign == rhs.sign {
			result_sign = lhs.sign;
			result_significand = significand_lhs + significand_rhs;
		} else {
			if significand_lhs >= significand_rhs {
				result_significand = significand_lhs - significand_rhs;
				result_sign = lhs.sign;
			} else {
				result_significand = significand_rhs - significand_lhs;
				result_sign = rhs.sign;
			}

			// Section 6.3
			if result_significand == 0 {
				let res = if rm.const_eq(RoundTowardNegative) {
					0x80000000
				} else {
					0
				};
				return (res, flags);
			}
		}

		// =========================================================================
		// Shift the result significand left or right until it is in the form 1.XXXX
		// and adjust the exponent
		// =========================================================================

		let mut result_exponent = if exponent_rhs < exponent_lhs {
			exponent_lhs
		} else {
			exponent_rhs
		};

		// Check if it's 2 bits or more past the implicit bit
		if result_significand >= (2 << 26) {
			let sticky = (result_significand & 1) != 0;
			result_significand = (result_significand >> 1) | if sticky { 1 } else { 0 };
			result_exponent += 1;
		}

		while result_significand > 0 && result_significand < (1 << 26) {
			result_significand <<= 1;
			result_exponent -= 1;
		}

		// ========
		// Overflow
		// ========

		if result_exponent >= 0xFF {
			flags.overflow();
			flags.inexact();
			let result_fraction = match rm {
				RoundTowardZero => 0x7FFFFF,
				RoundTowardNegative if result_sign == 1 => 0,
				RoundTowardNegative if result_sign == 0 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 1 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 0 => 0,
				_ => 0,
			};
			let result_exponent = if result_fraction == 0 { 0xFF } else { 0xFE };
			return ((result_sign << 31) | (result_exponent << 23) | result_fraction, flags);
		}

		// =========
		// Underflow
		// =========

		if result_exponent <= 0 {
			// Check tininess before rounding
			// Section 7.5
			let shift = (1 - result_exponent) as u32;

			let guard_shift = shift + 2;
			let guard = (result_significand >> guard_shift) & 1;

			let round_sticky_mask = (1_u64 << guard_shift) - 1;
			let round_sticky = (result_significand & round_sticky_mask) != 0;

			let lost_bits = guard != 0 || round_sticky;

			if lost_bits {
				flags.inexact();
				// Underflow flag is raised if result is inexact
				// Section 7.5
				flags.underflow();
			}

			let mut result_fraction = result_significand >> (shift + 3);
			let lsb = result_fraction & 1;

			let needs_round = match rm {
				RoundTiesToEven => guard == 1 && (round_sticky || lsb == 1),
				RoundTiesToAway => guard == 1,
				RoundTowardPositive => result_sign == 0 && lost_bits,
				RoundTowardNegative => result_sign == 1 && lost_bits,
				RoundTowardZero => false,
			};

			if needs_round {
				result_fraction += 1;
			}

			// Did rounding a subnormal make it normal?
			if result_fraction == (1 << 23) {
				return ((result_sign << 31) | (1 << 23) | 0, flags);
			}

			return (
				(result_sign << 31) | (0 << 23) | (result_fraction as u32 & 0x7FFFFF),
				flags,
			);
		}

		// ========
		// Rounding
		// ========

		let guard = (result_significand >> 2) & 1;
		let round = (result_significand >> 1) & 1;
		let sticky = result_significand & 1;
		let pre_round_inexact = guard != 0 || round != 0 || sticky != 0;
		let pre_round_result_significand_lsb = (result_significand >> 3) & 1;
		let mut result_significand = result_significand >> 3;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => result_sign == 0 && pre_round_inexact,
			RoundTowardNegative => result_sign == 1 && pre_round_inexact,
			RoundTowardZero => false,
		};

		if should_round {
			result_significand += 1;

			// Did rounding cause an overflow to the next exponent?
			if result_significand >= (2 << 23) {
				result_significand >>= 1;
				result_exponent += 1;

				// This could overflow to infinity
				if result_exponent >= 0xFF {
					flags.overflow();
					flags.inexact();
					return ((result_sign << 31) | (0xFF << 23) | 0, flags);
				}
			}
		}

		if pre_round_inexact {
			flags.inexact();
		}

		// =======
		// Packing
		// =======

		let result_fraction = result_significand as u32 & 0x7FFFFF;
		let result_exponent = result_exponent.cast_unsigned();
		((result_sign << 31) | (result_exponent << 23) | result_fraction, flags)
	}

	pub const fn sub(lhs_bits: u32, rhs_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let negated_rhs_bits = rhs_bits ^ 0x80000000;
		Self::add(lhs_bits, negated_rhs_bits, rm)
	}

	pub const fn mul(lhs_bits: u32, rhs_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);

		let mut flags = ExceptionFlags::new();

		// Reference: Section 6.3
		// The sign of a product is the XOR of the signs of the operands.
		let result_sign = lhs.sign ^ rhs.sign;

		// =======================
		// Special Cases
		// Reference: Section 6, 7
		// =======================

		// Either sNaN: invalid operation and return qNaN
		if lhs.is_signaling_nan() {
			flags.invalid();
			return (lhs_bits | 0x400000, flags);
		}

		if rhs.is_signaling_nan() {
			flags.invalid();
			return (rhs_bits | 0x400000, flags);
		}

		if lhs.is_nan() {
			return (lhs_bits, flags);
		}

		if rhs.is_nan() {
			return (rhs_bits, flags);
		}

		if (lhs.is_infinity() && rhs.is_zero()) || (rhs.is_infinity() && lhs.is_zero()) {
			flags.invalid();
			return (Self::Q_NAN, flags);
		}

		if lhs.is_infinity() || rhs.is_infinity() {
			return ((result_sign << 31) | (0xFF << 23), flags);
		}

		if lhs.is_zero() || rhs.is_zero() {
			return ((result_sign << 31), flags);
		}

		// ==================
		// Extract components
		// ==================

		let mut exponent_lhs = lhs.exponent as i32;
		let mut significand_lhs = lhs.fraction as u64;

		if lhs.is_subnormal() {
			let shift = significand_lhs.leading_zeros() as i32 - (64 - 24);
			significand_lhs <<= shift;
			exponent_lhs = 1 - shift;
		} else {
			significand_lhs |= 1 << 23;
		}

		let mut exponent_rhs = rhs.exponent as i32;
		let mut significand_rhs = rhs.fraction as u64;

		if rhs.is_subnormal() {
			let shift = significand_rhs.leading_zeros() as i32 - (64 - 24);
			significand_rhs <<= shift;
			exponent_rhs = 1 - shift;
		} else {
			significand_rhs |= 1 << 23;
		}

		let mut result_exponent = exponent_lhs + exponent_rhs - 127;
		let mut result_significand = significand_lhs * significand_rhs;

		// =====================================================
		// Normalize the 48bit product and align it for rounding
		// =====================================================

		// The product of two 24bit significands is either 47 or 48 bits.
		// Align it to a 24-bit value, the implicit bit at 23 with 3 GRS bits below.
		if (result_significand & (1u64 << 47)) != 0 {
			// Product is 48 bits, normalize.
			// Increment exponent and shift to align.
			result_exponent += 1;
			let shift = 47 - 26;
			let sticky_mask = (1u64 << shift) - 1;
			let sticky = (result_significand & sticky_mask) != 0;
			result_significand = (result_significand >> shift) | if sticky { 1 } else { 0 };
		} else {
			// Product is 47 bits, already normalized.
			// Shift to align.
			let shift = 46 - 26;
			let sticky_mask = (1u64 << shift) - 1;
			let sticky = (result_significand & sticky_mask) != 0;
			result_significand = (result_significand >> shift) | if sticky { 1 } else { 0 };
		}

		// ========
		// Overflow
		// ========

		if result_exponent >= 0xFF {
			flags.overflow();
			flags.inexact();
			let result_fraction = match rm {
				RoundTowardZero => 0x7FFFFF,
				RoundTowardNegative if result_sign == 1 => 0,
				RoundTowardNegative if result_sign == 0 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 1 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 0 => 0,
				_ => 0,
			};
			let result_exponent = if result_fraction == 0 { 0xFF } else { 0xFE };
			return ((result_sign << 31) | (result_exponent << 23) | result_fraction, flags);
		}

		// =========
		// Underflow
		// =========

		if result_exponent <= 0 {
			// Check tininess before rounding
			let shift = (1 - result_exponent) as u32;

			let guard_shift = shift + 2;
			let guard = (result_significand >> guard_shift) & 1;

			let round_sticky_mask = (1u64 << guard_shift) - 1;
			let round_sticky = (result_significand & round_sticky_mask) != 0;

			let lost_bits = guard != 0 || round_sticky;
			if lost_bits {
				flags.inexact();
				flags.underflow();
			}

			let mut result_fraction = result_significand >> (shift + 3);
			let lsb = result_fraction & 1;

			let needs_round = match rm {
				RoundTiesToEven => guard == 1 && (round_sticky || lsb == 1),
				RoundTiesToAway => guard == 1,
				RoundTowardPositive => result_sign == 0 && lost_bits,
				RoundTowardNegative => result_sign == 1 && lost_bits,
				RoundTowardZero => false,
			};

			if needs_round {
				result_fraction += 1;
			}

			if result_fraction == (1 << 23) {
				return ((result_sign << 31) | (1 << 23) | 0, flags);
			}

			return (
				(result_sign << 31) | (0 << 23) | (result_fraction as u32 & 0x7FFFFF),
				flags,
			);
		}

		// ========
		// Rounding
		// ========

		let guard = (result_significand >> 2) & 1;
		let round = (result_significand >> 1) & 1;
		let sticky = result_significand & 1;

		let pre_round_inexact = guard != 0 || round != 0 || sticky != 0;
		let pre_round_result_significand_lsb = (result_significand >> 3) & 1;

		let mut result_significand = result_significand >> 3;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => result_sign == 0 && pre_round_inexact,
			RoundTowardNegative => result_sign == 1 && pre_round_inexact,
			RoundTowardZero => false,
		};

		if should_round {
			result_significand += 1;

			if result_significand >= (2 << 23) {
				result_significand >>= 1;
				result_exponent += 1;

				if result_exponent >= 0xFF {
					flags.overflow();
					flags.inexact();
					return ((result_sign << 31) | (0xFF << 23) | 0, flags);
				}
			}
		}

		if pre_round_inexact {
			flags.inexact();
		}

		// =======
		// Packing
		// =======

		let result_fraction = result_significand as u32 & 0x7FFFFF;
		let result_exponent = result_exponent as u32;

		((result_sign << 31) | (result_exponent << 23) | result_fraction, flags)
	}

	pub const fn div(lhs_bits: u32, rhs_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);

		let mut flags = ExceptionFlags::new();

		// The sign of a quotient is the XOR of the signs of the operands.
		// Reference: Section 6.3
		let result_sign = lhs.sign ^ rhs.sign;

		// =======================
		// Special Cases
		// Reference: Section 6, 7
		// =======================

		// Either sNaN: invalid operation and return qNaN
		// Reference: Section 7.2
		if lhs.is_signaling_nan() {
			flags.invalid();
			return (lhs_bits | 0x400000, flags);
		}

		if rhs.is_signaling_nan() {
			flags.invalid();
			return (rhs_bits | 0x400000, flags);
		}

		// Either qNaN: return qNaN
		if lhs.is_nan() {
			return (lhs_bits, flags);
		}

		if rhs.is_nan() {
			return (rhs_bits, flags);
		}

		// Division by zero
		// Reference: Section 7.3
		if rhs.is_zero() {
			return if lhs.is_zero() {
				// 0/0 is an invalid operation
				// Reference: Section 7.2
				flags.invalid();
				(Self::Q_NAN, flags)
			} else {
				// Finite/0 is division by zero
				flags.div_by_zero();
				((result_sign << 31) | (0xFF << 23), flags)
			};
		}

		// Infinity handling
		if lhs.is_infinity() {
			return if rhs.is_infinity() {
				// inf/inf is an invalid operation
				// Reference: Section 7.2
				flags.invalid();
				(Self::Q_NAN, flags)
			} else {
				// inf/finite is infinity
				((result_sign << 31) | (0xFF << 23), flags)
			};
		}

		if rhs.is_infinity() {
			// finite/inf is zero
			return ((result_sign << 31), flags);
		}

		if lhs.is_zero() {
			// 0/finite is zero
			return ((result_sign << 31), flags);
		}

		// ==================
		// Extract components and normalize subnormals
		// ==================

		let mut exponent_lhs = lhs.exponent as i32;
		let mut significand_lhs = lhs.fraction as u64;

		if lhs.is_subnormal() {
			let shift = significand_lhs.leading_zeros() as i32 - (64 - 24);
			significand_lhs <<= shift;
			exponent_lhs = 1 - shift;
		} else {
			significand_lhs |= 1 << 23;
		}

		let mut exponent_rhs = rhs.exponent as i32;
		let mut significand_rhs = rhs.fraction as u64;

		if rhs.is_subnormal() {
			let shift = significand_rhs.leading_zeros() as i32 - (64 - 24);
			significand_rhs <<= shift;
			exponent_rhs = 1 - shift;
		} else {
			significand_rhs |= 1 << 23;
		}

		// ===================================
		// Perform division on significands
		// ===================================

		// The exponent of a quotient.
		// Reference: Section 5.4.1
		let mut result_exponent = exponent_lhs - exponent_rhs + 127;

		// Pre-normalize the dividend so the quotient is always of the form 1.xxxx...
		let mut dividend = significand_lhs;
		if dividend < significand_rhs {
			dividend <<= 1;
			result_exponent -= 1;
		}

		// We need p-1 fraction bits (23) and 3 GRS bits for rounding.
		dividend <<= 26;

		let quotient = dividend / significand_rhs;
		let remainder = dividend % significand_rhs;

		let mut result_significand = quotient;
		if remainder != 0 {
			result_significand |= 1;
		}

		// ========
		// Overflow
		// ========

		if result_exponent >= 0xFF {
			flags.overflow();
			flags.inexact();
			let result_fraction = match rm {
				RoundTowardZero => 0x7FFFFF,
				RoundTowardNegative if result_sign == 1 => 0,
				RoundTowardNegative if result_sign == 0 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 1 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 0 => 0,
				_ => 0,
			};
			let result_exponent = if result_fraction == 0 { 0xFF } else { 0xFE };
			return ((result_sign << 31) | (result_exponent << 23) | result_fraction, flags);
		}

		// =========
		// Underflow
		// =========

		if result_exponent <= 0 {
			// Check tininess before rounding
			let shift = (1 - result_exponent) as u32;

			let guard_shift = shift + 2;
			let guard = (result_significand >> guard_shift) & 1;

			let round_sticky_mask = (1u64 << guard_shift) - 1;
			let round_sticky = (result_significand & round_sticky_mask) != 0;

			let lost_bits = guard != 0 || round_sticky;
			if lost_bits {
				flags.inexact();
				flags.underflow();
			}

			let mut result_fraction = result_significand >> (shift + 3);
			let lsb = result_fraction & 1;

			let needs_round = match rm {
				RoundTiesToEven => guard == 1 && (round_sticky || lsb == 1),
				RoundTiesToAway => guard == 1,
				RoundTowardPositive => result_sign == 0 && lost_bits,
				RoundTowardNegative => result_sign == 1 && lost_bits,
				RoundTowardZero => false,
			};

			if needs_round {
				result_fraction += 1;
			}

			if result_fraction == (1 << 23) {
				return ((result_sign << 31) | (1 << 23) | 0, flags);
			}

			return (
				(result_sign << 31) | (0 << 23) | (result_fraction as u32 & 0x7FFFFF),
				flags,
			);
		}

		// ========
		// Rounding
		// ========

		let guard = (result_significand >> 2) & 1;
		let round = (result_significand >> 1) & 1;
		let sticky = result_significand & 1;

		let pre_round_inexact = guard != 0 || round != 0 || sticky != 0;
		let pre_round_result_significand_lsb = (result_significand >> 3) & 1;

		let mut result_significand = result_significand >> 3;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => result_sign == 0 && pre_round_inexact,
			RoundTowardNegative => result_sign == 1 && pre_round_inexact,
			RoundTowardZero => false,
		};

		if should_round {
			result_significand += 1;

			if result_significand >= (2 << 23) {
				result_significand >>= 1;
				result_exponent += 1;

				if result_exponent >= 0xFF {
					flags.overflow();
					flags.inexact();
					return ((result_sign << 31) | (0xFF << 23) | 0, flags);
				}
			}
		}

		if pre_round_inexact {
			flags.inexact();
		}

		// =======
		// Packing
		// =======

		let result_fraction = result_significand as u32 & 0x7FFFFF;
		let result_exponent = result_exponent as u32;

		((result_sign << 31) | (result_exponent << 23) | result_fraction, flags)
	}

	pub const fn sqrt(bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let this = F32::from_u32(bits);

		let mut flags = ExceptionFlags::new();

		// =======================
		// Special Cases
		// Reference: Section 5, 6, 7
		// =======================

		// sNaN: invalid operation and return qNaN
		if this.is_signaling_nan() {
			flags.invalid();
			return (bits | 0x400000, flags);
		}

		// qNaN: return qNaN
		if this.is_nan() {
			return (bits, flags);
		}

		// Negative numbers (excl. -ZERO), invalid operation and return qNaN
		if this.sign == 1 && !this.is_zero() {
			flags.invalid();
			return (Self::Q_NAN, flags);
		}

		// sqrt(-0) = -0
		if this.sign == 1 && this.is_zero() {
			return (0x80000000, flags);
		}

		// sqrt(+inf) = +inf
		if this.is_infinity() {
			return (bits, flags);
		}

		// sqrt(+0) = +0
		if this.is_zero() {
			return (bits, flags);
		}

		// ===========================================
		// Extract components and normalize subnormals
		// ===========================================

		let mut exponent = this.exponent as i32;
		let mut significand = this.fraction as u64;

		if this.is_subnormal() {
			let shift = significand.leading_zeros() as i32 - (64 - 24);
			significand <<= shift;
			exponent = 1 - shift;
		} else {
			significand |= 1 << 23;
		}

		let mut unbiased_exponent = exponent - 127;

		// We need to be able to divide the exponent (E) by 2.
		// If E is odd then subtract 1 from E and double the significand to compensate.
		// sqrt(x * 2^E) = sqrt(2x * 2^(E-1))
		if (unbiased_exponent & 1) != 0 {
			significand <<= 1;
			unbiased_exponent -= 1;
		}

		let result_exponent = (unbiased_exponent / 2) + 127;

		// ============================================================================
		// We want the sqrt of the significand (S) which is a 24bit integer (Si)
		// with an implicit leading 1 bit.
		//
		// Si represents the value (Sv) of `Si / 2^23`.
		//
		// We want `R = sqrt(Sv)`, but to work with integers we:
		// `Ri = R * 2^23 = sqrt(Sv) * 2^23 = sqrt(Si / 2^23) * 2^23 = sqrt(Si * 2^23)`
		//
		// For rounding we use 3 GRS bits, just left-shift the result by 3.
		// We can achieve this by multiplying the value by `2^6`.
		//
		// `Rgrs = sqrt(Si * 2^23 * 2^6) = sqrt(Si * 2^29)`
		// ============================================================================

		let radicand = significand << 29;

		let root = radicand.isqrt();
		let remainder = radicand - root * root;

		let mut result_significand = root;

		if remainder != 0 {
			result_significand |= 1;
		}

		// ========
		// Rounding
		// ========

		let guard = (result_significand >> 2) & 1;
		let round = (result_significand >> 1) & 1;
		let sticky = result_significand & 1;

		let pre_round_inexact = guard != 0 || round != 0 || sticky != 0;

		let mut result_significand = result_significand >> 3;
		let pre_round_result_significand_lsb = result_significand & 1;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => pre_round_inexact,
			RoundTowardNegative => false,
			RoundTowardZero => false,
		};

		if should_round {
			result_significand += 1;
		}

		if pre_round_inexact {
			flags.inexact();
		}

		// =======
		// Packing
		// =======

		let result_fraction = result_significand as u32 & 0x7FFFFF;
		let result_exponent = result_exponent as u32;
		((result_exponent << 23) | result_fraction, flags)
	}

	pub const fn fma(lhs_bits: u32, rhs_bits: u32, addend_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);
		let addend = F32::from_u32(addend_bits);

		let mut flags = ExceptionFlags::new();

		// =============
		// Special Cases
		// =============

		if lhs.is_signaling_nan() {
			flags.invalid();
			return (lhs_bits | 0x400000, flags);
		}

		if rhs.is_signaling_nan() {
			flags.invalid();
			return (rhs_bits | 0x400000, flags);
		}

		if addend.is_signaling_nan() {
			flags.invalid();
			return (addend_bits | 0x400000, flags);
		}

		if lhs.is_nan() {
			return (lhs_bits, flags);
		}

		if rhs.is_nan() {
			return (rhs_bits, flags);
		}

		if addend.is_nan() {
			return (addend_bits, flags);
		}

		let product_sign = lhs.sign ^ rhs.sign;

		if (lhs.is_infinity() && rhs.is_zero()) || (rhs.is_infinity() && lhs.is_zero()) {
			flags.invalid();
			return (Self::Q_NAN, flags);
		}

		if lhs.is_infinity() || rhs.is_infinity() {
			if addend.is_infinity() && product_sign != addend.sign {
				flags.invalid();
				return (Self::Q_NAN, flags);
			}
			return ((product_sign << 31) | (0xFF << 23), flags);
		}

		if addend.is_infinity() {
			return (addend_bits, flags);
		}

		if lhs.is_zero() || rhs.is_zero() {
			if addend.is_zero() {
				let result_sign = if product_sign != addend.sign {
					if rm.const_eq(RoundTowardNegative) { 1 } else { 0 }
				} else {
					product_sign
				};
				return ((result_sign << 31), flags);
			}
			return (addend_bits, flags);
		}

		if addend.is_zero() {
			return Self::mul(lhs_bits, rhs_bits, rm);
		}

		// ==================
		// Extract components
		// ==================

		let mut exponent_lhs = lhs.exponent as i32;
		let mut significand_lhs = lhs.fraction as u64;

		if lhs.is_subnormal() {
			let shift = significand_lhs.leading_zeros() as i32 - (64 - 24);
			significand_lhs <<= shift;
			exponent_lhs = 1 - shift;
		} else {
			significand_lhs |= 1 << 23;
		}

		let mut exponent_rhs = rhs.exponent as i32;
		let mut significand_rhs = rhs.fraction as u64;

		if rhs.is_subnormal() {
			let shift = significand_rhs.leading_zeros() as i32 - (64 - 24);
			significand_rhs <<= shift;
			exponent_rhs = 1 - shift;
		} else {
			significand_rhs |= 1 << 23;
		}

		let mut exponent_addend = addend.exponent as i32;
		let mut significand_addend = addend.fraction as u64;

		if addend.is_subnormal() {
			let shift = significand_addend.leading_zeros() as i32 - (64 - 24);
			significand_addend <<= shift;
			exponent_addend = 1 - shift;
		} else {
			significand_addend |= 1 << 23;
		}

		// =================================================
		// Compute product
		// Product is 47 or 48 bits, align with the GRS bits
		// =================================================

		let mut product_exponent = exponent_lhs + exponent_rhs - 127;
		let mut product_significand = significand_lhs * significand_rhs;

		if (product_significand & (1u64 << 47)) != 0 {
			product_exponent += 1;
			let shift = 47 - 26;
			let sticky_mask = (1u64 << shift) - 1;
			let sticky = (product_significand & sticky_mask) != 0;
			product_significand = (product_significand >> shift) | if sticky { 1 } else { 0 };
		} else {
			let shift = 46 - 26;
			let sticky_mask = (1u64 << shift) - 1;
			let sticky = (product_significand & sticky_mask) != 0;
			product_significand = (product_significand >> shift) | if sticky { 1 } else { 0 };
		}

		significand_addend <<= 3;

		// ========================
		// Align and add/subtract
		// ========================

		let mut result_significand;
		let mut result_exponent;
		let result_sign;

		if product_exponent == exponent_addend {
			if product_sign == addend.sign {
				result_significand = product_significand + significand_addend;
				result_exponent = product_exponent;
				result_sign = product_sign;
			} else {
				if product_significand >= significand_addend {
					result_significand = product_significand - significand_addend;
					result_sign = product_sign;
				} else {
					result_significand = significand_addend - product_significand;
					result_sign = addend.sign;
				}
				result_exponent = product_exponent;

				if result_significand == 0 {
					let res = if rm.const_eq(RoundTowardNegative) {
						0x80000000
					} else {
						0
					};
					return (res, flags);
				}
			}
		} else if product_exponent > exponent_addend {
			let exp_diff = (product_exponent - exponent_addend) as u32;
			let sticky = if exp_diff >= 64 {
				significand_addend != 0
			} else {
				(significand_addend & ((1 << exp_diff) - 1)) != 0
			};
			let shifted_addend = if exp_diff >= 64 {
				0
			} else {
				significand_addend >> exp_diff
			} | if sticky { 1 } else { 0 };

			if product_sign == addend.sign {
				result_significand = product_significand + shifted_addend;
				result_sign = product_sign;
			} else {
				result_significand = product_significand - shifted_addend;
				result_sign = product_sign;
			}
			result_exponent = product_exponent;
		} else {
			let exponent_difference = (exponent_addend - product_exponent) as u32;
			let sticky = if exponent_difference >= 64 {
				product_significand != 0
			} else {
				(product_significand & ((1 << exponent_difference) - 1)) != 0
			};
			let shifted_product = if exponent_difference >= 64 {
				0
			} else {
				product_significand >> exponent_difference
			} | if sticky { 1 } else { 0 };

			if product_sign == addend.sign {
				result_significand = significand_addend + shifted_product;
				result_sign = addend.sign;
			} else {
				if significand_addend >= shifted_product {
					result_significand = significand_addend - shifted_product;
					result_sign = addend.sign;
				} else {
					result_significand = shifted_product - significand_addend;
					result_sign = product_sign;
				}

				if result_significand == 0 {
					let res = if rm.const_eq(RoundTowardNegative) {
						0x80000000
					} else {
						0
					};
					return (res, flags);
				}
			}
			result_exponent = exponent_addend;
		}

		// Normalize
		if result_significand >= (2 << 26) {
			let sticky = (result_significand & 1) != 0;
			result_significand = (result_significand >> 1) | if sticky { 1 } else { 0 };
			result_exponent += 1;
		}

		while result_significand > 0 && result_significand < (1 << 26) {
			result_significand <<= 1;
			result_exponent -= 1;
		}

		if result_exponent >= 0xFF {
			flags.overflow();
			flags.inexact();

			let result_fraction = match rm {
				RoundTowardZero => 0x7FFFFF,
				RoundTowardNegative if result_sign == 1 => 0,
				RoundTowardNegative if result_sign == 0 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 1 => 0x7FFFFF,
				RoundTowardPositive if result_sign == 0 => 0,
				_ => 0,
			};

			let result_exponent = if result_fraction == 0 { 0xFF } else { 0xFE };
			return ((result_sign << 31) | (result_exponent << 23) | result_fraction, flags);
		}

		if result_exponent <= 0 {
			let shift = (1 - result_exponent) as u32;
			let guard_shift = shift + 2;
			let guard = (result_significand >> guard_shift) & 1;
			let round_sticky_mask = (1u64 << guard_shift) - 1;
			let round_sticky = (result_significand & round_sticky_mask) != 0;
			let lost_bits = guard != 0 || round_sticky;

			if lost_bits {
				flags.inexact();
				flags.underflow();
			}

			let mut result_fraction = result_significand >> (shift + 3);
			let lsb = result_fraction & 1;

			let needs_round = match rm {
				RoundTiesToEven => guard == 1 && (round_sticky || lsb == 1),
				RoundTiesToAway => guard == 1,
				RoundTowardPositive => result_sign == 0 && lost_bits,
				RoundTowardNegative => result_sign == 1 && lost_bits,
				RoundTowardZero => false,
			};

			if needs_round {
				result_fraction += 1;
			}

			if result_fraction == (1 << 23) {
				return ((result_sign << 31) | (1 << 23) | 0, flags);
			}

			return (
				(result_sign << 31) | (0 << 23) | (result_fraction as u32 & 0x7FFFFF),
				flags,
			);
		}

		let guard = (result_significand >> 2) & 1;
		let round = (result_significand >> 1) & 1;
		let sticky = result_significand & 1;
		let pre_round_inexact = guard != 0 || round != 0 || sticky != 0;
		let pre_round_result_significand_lsb = (result_significand >> 3) & 1;
		let mut result_significand = result_significand >> 3;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => result_sign == 0 && pre_round_inexact,
			RoundTowardNegative => result_sign == 1 && pre_round_inexact,
			RoundTowardZero => false,
		};

		if should_round {
			result_significand += 1;

			if result_significand >= (2 << 23) {
				result_significand >>= 1;
				result_exponent += 1;

				if result_exponent >= 0xFF {
					flags.overflow();
					flags.inexact();
					return ((result_sign << 31) | (0xFF << 23) | 0, flags);
				}
			}
		}

		if pre_round_inexact {
			flags.inexact();
		}

		let result_fraction = result_significand as u32 & 0x7FFFFF;
		let result_exponent = result_exponent as u32;
		((result_sign << 31) | (result_exponent << 23) | result_fraction, flags)
	}

	pub const fn min(lhs_bits: u32, rhs_bits: u32) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);

		let mut flags = ExceptionFlags::new();

		// sNaN = invalid
		if lhs.is_signaling_nan() || rhs.is_signaling_nan() {
			flags.invalid();
		}

		let lhs_is_nan = lhs.is_nan();
		let rhs_is_nan = rhs.is_nan();

		// If both are NaN return canonical qNaN
		// If one is NaN then return the other
		if lhs_is_nan {
			return if rhs_is_nan {
				(Self::Q_NAN, flags)
			} else {
				(rhs_bits, flags)
			};
		}
		if rhs_is_nan {
			return (lhs_bits, flags);
		}

		if lhs.sign != rhs.sign {
			// neg < pos
			return if lhs.sign == 1 {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			};
		}

		if lhs.sign == 0 {
			if lhs_bits < rhs_bits {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			}
		} else {
			if lhs_bits > rhs_bits {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			}
		}
	}

	pub const fn max(lhs_bits: u32, rhs_bits: u32) -> (u32, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);

		let mut flags = ExceptionFlags::new();

		// sNaN = invalid
		if lhs.is_signaling_nan() || rhs.is_signaling_nan() {
			flags.invalid();
		}

		let lhs_is_nan = lhs.is_nan();
		let rhs_is_nan = rhs.is_nan();

		// If both are NaN return canonical qNaN
		// If one is NaN then return the other
		if lhs_is_nan {
			return if rhs_is_nan {
				(Self::Q_NAN, flags)
			} else {
				(rhs_bits, flags)
			};
		}
		if rhs_is_nan {
			return (lhs_bits, flags);
		}

		if lhs.sign != rhs.sign {
			// pos > neg
			return if lhs.sign == 0 {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			};
		}

		if lhs.sign == 0 {
			if lhs_bits > rhs_bits {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			}
		} else {
			if lhs_bits < rhs_bits {
				(lhs_bits, flags)
			} else {
				(rhs_bits, flags)
			}
		}
	}

	pub const fn eq(lhs_bits: u32, rhs_bits: u32) -> (bool, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);
		let mut flags = ExceptionFlags::new();

		// RISC-V only signals for sNaNs, equivalent to `compareQuietEqual`
		// Reference: Section 5.11
		if lhs.is_signaling_nan() || rhs.is_signaling_nan() {
			flags.invalid();
			return (false, flags);
		}

		if lhs.is_nan() || rhs.is_nan() {
			return (false, flags);
		}

		if lhs.is_zero() && rhs.is_zero() {
			return (true, flags);
		}

		(lhs_bits == rhs_bits, flags)
	}

	pub const fn lt(lhs_bits: u32, rhs_bits: u32) -> (bool, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);
		let mut flags = ExceptionFlags::new();

		// RISC-V signals for any NaN, equivalent to `compareSignalingLess`
		// Reference: Section 5.11
		if lhs.is_nan() || rhs.is_nan() {
			flags.invalid();
			return (false, flags);
		}

		if lhs.is_zero() && rhs.is_zero() {
			return (false, flags);
		}

		if lhs.sign != rhs.sign {
			return (lhs.sign == 1, flags);
		}

		if lhs.sign == 0 {
			(lhs_bits < rhs_bits, flags)
		} else {
			(lhs_bits > rhs_bits, flags)
		}
	}

	pub const fn le(lhs_bits: u32, rhs_bits: u32) -> (bool, ExceptionFlags) {
		let lhs = F32::from_u32(lhs_bits);
		let rhs = F32::from_u32(rhs_bits);
		let mut flags = ExceptionFlags::new();

		// RISC-V signals for any NaN, equivalent to `compareSignalingLessEqual`
		// Reference: Section 5.11
		if lhs.is_nan() || rhs.is_nan() {
			flags.invalid();
			return (false, flags);
		}

		if (lhs.is_zero() && rhs.is_zero()) || (lhs_bits == rhs_bits) {
			return (true, flags);
		}

		if lhs.sign != rhs.sign {
			return (lhs.sign == 1, flags);
		}

		if lhs.sign == 0 {
			(lhs_bits < rhs_bits, flags)
		} else {
			(lhs_bits > rhs_bits, flags)
		}
	}
}

#[cfg(test)]
mod add_tests {
	use crate::{F32, RoundingMode};

	macro_rules! define_test {
		($(($name:ident, $lhs:expr, $rhs:expr, $rm:expr, $expected_result:expr)),+) => {
			$(
			#[test]
			fn $name() {
				let lhs_bits = $lhs.to_bits();
				let rhs_bits = $rhs.to_bits();
				let expected_bits = $expected_result.to_bits();

				let (result_bits, _eflags) = F32::add(lhs_bits, rhs_bits, $rm);
				let result_f32 = f32::from_bits(result_bits);

				if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nExpected NaN, got: {:?} ({:#010x})\n", result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                     panic!("\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $expected_result, expected_bits, result_f32, result_bits);
                }
			}
			)+
		};
	}

	define_test!(
		(one_plus_two, 1.0f32, 2.0f32, RoundingMode::RoundTiesToEven, 3.0f32),
		(
			neg_one_point_five_plus_neg_two_point_five,
			-1.5f32,
			-2.5f32,
			RoundingMode::RoundTiesToEven,
			-4.0f32
		),
		(
			ten_plus_neg_three_point_five,
			10.0f32,
			-3.5f32,
			RoundingMode::RoundTiesToEven,
			6.5f32
		),
		(
			cancellation,
			1.2345e10f32,
			-1.2345e10f32,
			RoundingMode::RoundTiesToEven,
			0.0f32
		),
		(
			rounding_rne,
			0.1f32,
			0.2f32,
			RoundingMode::RoundTiesToEven,
			(0.1f32 + 0.2f32)
		),
		(
			inf_plus_one,
			f32::INFINITY,
			1.0f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			neg_inf_plus_neg_one,
			f32::NEG_INFINITY,
			-1.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NEG_INFINITY
		),
		(
			inf_plus_neg_inf_is_nan,
			f32::INFINITY,
			f32::NEG_INFINITY,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			nan_plus_one_is_nan,
			f32::NAN,
			1.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			zero_plus_neg_zero_rne,
			0.0f32,
			-0.0f32,
			RoundingMode::RoundTiesToEven,
			0.0f32
		),
		(
			zero_plus_neg_zero_rtn,
			0.0f32,
			-0.0f32,
			RoundingMode::RoundTowardNegative,
			-0.0f32
		),
		(
			overflow_to_inf,
			f32::MAX,
			f32::MAX,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			subnormal_addition,
			f32::from_bits(0x000116c3),
			f32::from_bits(0x000116c3),
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x00022d86)
		)
	);
}

#[cfg(test)]
mod mul_tests {
	use crate::{F32, RoundingMode};

	macro_rules! define_test {
        ($(($name:ident, $lhs:expr, $rhs:expr, $rm:expr, $expected_result:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let lhs_bits = $lhs.to_bits();
                let rhs_bits = $rhs.to_bits();
                let expected_bits = $expected_result.to_bits();

                let (result_bits, _eflags) = F32::mul(lhs_bits, rhs_bits, $rm);
                let result_f32 = f32::from_bits(result_bits);

                if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nExpected NaN, got: {:?} ({:#010x})\n", result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                    panic!("\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $expected_result, expected_bits, result_f32, result_bits);
                }
            }
            )+
        };
    }

	define_test!(
		(two_times_three, 2.0f32, 3.0f32, RoundingMode::RoundTiesToEven, 6.0f32),
		(
			neg_point_five_times_ten,
			-0.5f32,
			10.0f32,
			RoundingMode::RoundTiesToEven,
			-5.0f32
		),
		(
			neg_two_times_neg_three,
			-2.0f32,
			-3.0f32,
			RoundingMode::RoundTiesToEven,
			6.0f32
		),
		(
			anything_times_zero,
			123.45f32,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			0.0f32
		),
		(
			anything_times_neg_zero,
			123.45f32,
			-0.0f32,
			RoundingMode::RoundTiesToEven,
			-0.0f32
		),
		(
			inf_times_two,
			f32::INFINITY,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			neg_inf_times_two,
			f32::NEG_INFINITY,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NEG_INFINITY
		),
		(
			inf_times_zero_is_nan,
			f32::INFINITY,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			nan_times_two_is_nan,
			f32::NAN,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			overflow_to_inf,
			f32::MAX,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			subnormal_times_two,
			f32::from_bits(0x00000001),
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x00000002)
		),
		(
			subnormal_underflow_to_zero,
			f32::from_bits(0x00000001),
			0.1f32,
			RoundingMode::RoundTiesToEven,
			0.0f32
		)
	);
}

#[cfg(test)]
mod div_tests {
	use crate::{F32, RoundingMode};

	macro_rules! define_test {
        ($(($name:ident, $lhs:expr, $rhs:expr, $rm:expr, $expected_result:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let lhs_bits = $lhs.to_bits();
                let rhs_bits = $rhs.to_bits();
                let expected_bits = $expected_result.to_bits();

                let (result_bits, _eflags) = F32::div(lhs_bits, rhs_bits, $rm);
                let result_f32 = f32::from_bits(result_bits);

                if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nExpected NaN, got: {:?} ({:#010x})\n", result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                     panic!("\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $expected_result, expected_bits, result_f32, result_bits);
                }
            }
            )+
        };
    }

	define_test!(
		(six_div_three, 6.0f32, 3.0f32, RoundingMode::RoundTiesToEven, 2.0f32),
		(
			one_div_three,
			1.0f32,
			3.0f32,
			RoundingMode::RoundTiesToEven,
			0.33333334f32
		),
		(
			neg_five_div_two,
			-5.0f32,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			-2.5f32
		),
		(
			one_div_zero_is_inf,
			1.0f32,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			neg_one_div_zero_is_neg_inf,
			-1.0f32,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NEG_INFINITY
		),
		(
			zero_div_zero_is_nan,
			0.0f32,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			inf_div_inf_is_nan,
			f32::INFINITY,
			f32::INFINITY,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			one_div_inf_is_zero,
			1.0f32,
			f32::INFINITY,
			RoundingMode::RoundTiesToEven,
			0.0f32
		),
		(
			inf_div_one_is_inf,
			f32::INFINITY,
			1.0f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			max_div_point_one_is_overflow,
			f32::MAX,
			0.1f32,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			min_positive_div_two_is_underflow,
			f32::MIN_POSITIVE,
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x00400000)
		),
		(
			subnormal_div,
			f32::from_bits(0x00400000),
			2.0f32,
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x00200000)
		)
	);
}

#[cfg(test)]
mod sqrt_tests {
	use crate::{F32, RoundingMode};

	macro_rules! define_test {
        ($(($name:ident, $input:expr, $rm:expr, $expected_result:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let input_bits = $input.to_bits();
                let expected_bits = $expected_result.to_bits();

                let (result_bits, _eflags) = F32::sqrt(input_bits, $rm);
                let result_f32 = f32::from_bits(result_bits);

                if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nInput: {:?}\nExpected NaN, got: {:?} ({:#010x})\n", $input, result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                     panic!("\nInput: {:?}\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $input, $expected_result, expected_bits, result_f32, result_bits);
                }
            }
            )+
        };
    }

	define_test!(
		(four, 4.0f32, RoundingMode::RoundTiesToEven, 2.0f32),
		(two, 2.0f32, RoundingMode::RoundTiesToEven, f32::from_bits(0x3fb504f3)), // 1.4142135
		(one_hundred, 100.0f32, RoundingMode::RoundTiesToEven, 10.0f32),
		(positive_zero, 0.0f32, RoundingMode::RoundTiesToEven, 0.0f32),
		(negative_zero, -0.0f32, RoundingMode::RoundTiesToEven, -0.0f32),
		(
			positive_infinity,
			f32::INFINITY,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(negative_one, -1.0f32, RoundingMode::RoundTiesToEven, f32::NAN),
		(max, f32::MAX, RoundingMode::RoundTiesToEven, f32::from_bits(0x5f7fffff)),
		(
			min_normal,
			f32::from_bits(0x00800000),
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x20000000)
		),
		(
			min_subnormal,
			f32::MIN_POSITIVE,
			RoundingMode::RoundTiesToEven,
			f32::from_bits(0x20000000)
		)
	);
}

#[cfg(test)]
mod fma_tests {
	use crate::{F32, RoundingMode};

	macro_rules! define_test {
        ($(($name:ident, $a:expr, $b:expr, $c:expr, $rm:expr, $expected_result:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let a_bits = $a.to_bits();
                let b_bits = $b.to_bits();
                let c_bits = $c.to_bits();
                let expected_bits = $expected_result.to_bits();

                let (result_bits, _eflags) = F32::fma(a_bits, b_bits, c_bits, $rm);
                let result_f32 = f32::from_bits(result_bits);

                if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nExpected NaN, got: {:?} ({:#010x})\n", result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                     panic!("\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $expected_result, expected_bits, result_f32, result_bits);
                }
            }
            )+
        };
    }

	define_test!(
		(
			fma_basic,
			2.0f32,
			3.0f32,
			4.0f32,
			RoundingMode::RoundTiesToEven,
			10.0f32
		),
		(
			fma_exact,
			0.5f32,
			0.5f32,
			0.5f32,
			RoundingMode::RoundTiesToEven,
			0.75f32
		),
		(
			fma_zero_product,
			0.0f32,
			5.0f32,
			3.0f32,
			RoundingMode::RoundTiesToEven,
			3.0f32
		),
		(
			fma_zero_addend,
			2.0f32,
			3.0f32,
			0.0f32,
			RoundingMode::RoundTiesToEven,
			6.0f32
		),
		(
			fma_inf_times_zero,
			f32::INFINITY,
			0.0f32,
			1.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		),
		(
			fma_inf_addend,
			2.0f32,
			3.0f32,
			f32::INFINITY,
			RoundingMode::RoundTiesToEven,
			f32::INFINITY
		),
		(
			fma_nan_propagation,
			f32::NAN,
			2.0f32,
			3.0f32,
			RoundingMode::RoundTiesToEven,
			f32::NAN
		)
	);
}

#[cfg(test)]
mod min_max_tests {
	use crate::{ExceptionFlags, F32};

	macro_rules! define_test {
        ($(($name:ident, $func:ident, $lhs:expr, $rhs:expr, $expected_result:expr, $expected_flags:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let lhs_bits = $lhs.to_bits();
                let rhs_bits = $rhs.to_bits();
                let expected_bits = $expected_result.to_bits();
				let expected_flags = $expected_flags;

                let (result_bits, flags) = F32::$func(lhs_bits, rhs_bits);
                let result_f32 = f32::from_bits(result_bits);

                if $expected_result.is_nan() {
                    if !result_f32.is_nan() {
                        panic!("\nExpected NaN, got: {:?} ({:#010x})\n", result_f32, result_bits);
                    }
                } else if result_bits != expected_bits {
                     panic!("\nExpected: {:?} ({:#010x})\nGot:      {:?} ({:#010x})\n",
                        $expected_result, expected_bits, result_f32, result_bits);
                }

				if flags.invalid_operation != expected_flags.invalid_operation {
					panic!("\nInvalid operation flag mismatch. Expected: {}, Got: {}\n", expected_flags.invalid_operation, flags.invalid_operation);
				}
            }
            )+
        };
    }

	const OK: ExceptionFlags = ExceptionFlags {
		invalid_operation: false,
		div_by_zero: false,
		overflow: false,
		underflow: false,
		inexact: false,
	};

	const INVALID: ExceptionFlags = ExceptionFlags {
		invalid_operation: true,
		div_by_zero: false,
		overflow: false,
		underflow: false,
		inexact: false,
	};

	define_test!(
		(min_positives, min, 1.0f32, 2.0f32, 1.0f32, OK),
		(min_negatives, min, -1.0f32, -2.0f32, -2.0f32, OK),
		(min_mixed_sign, min, 1.0f32, -2.0f32, -2.0f32, OK),
		(min_plus_minus_zero, min, 0.0f32, -0.0f32, -0.0f32, OK),
		(min_qnan_vs_num, min, f32::NAN, 1.0f32, 1.0f32, OK),
		(min_num_vs_qnan, min, 1.0f32, f32::NAN, 1.0f32, OK),
		(min_qnan_vs_qnan, min, f32::NAN, f32::NAN, f32::NAN, OK),
		(
			min_snan_vs_num,
			min,
			f32::from_bits(0x7f800001),
			1.0f32,
			1.0f32,
			INVALID
		),
		(
			min_snan_vs_snan,
			min,
			f32::from_bits(0x7f800001),
			f32::from_bits(0xffc00001),
			f32::NAN,
			INVALID
		),
		(
			min_snan_vs_qnan,
			min,
			f32::from_bits(0x7f800001),
			f32::NAN,
			f32::NAN,
			INVALID
		),
		(max_positives, max, 1.0f32, 2.0f32, 2.0f32, OK),
		(max_negatives, max, -1.0f32, -2.0f32, -1.0f32, OK),
		(max_mixed_sign, max, 1.0f32, -2.0f32, 1.0f32, OK),
		(max_plus_minus_zero, max, 0.0f32, -0.0f32, 0.0f32, OK),
		(max_qnan_vs_num, max, f32::NAN, 1.0f32, 1.0f32, OK),
		(max_num_vs_qnan, max, 1.0f32, f32::NAN, 1.0f32, OK),
		(max_qnan_vs_qnan, max, f32::NAN, f32::NAN, f32::NAN, OK),
		(
			max_snan_vs_num,
			max,
			f32::from_bits(0x7f800001),
			1.0f32,
			1.0f32,
			INVALID
		),
		(
			max_snan_vs_snan,
			max,
			f32::from_bits(0x7f800001),
			f32::from_bits(0xffc00001),
			f32::NAN,
			INVALID
		),
		(
			max_snan_vs_qnan,
			max,
			f32::from_bits(0x7f800001),
			f32::NAN,
			f32::NAN,
			INVALID
		)
	);
}

#[cfg(test)]
mod cmp_tests {
	use crate::{ExceptionFlags, F32};

	macro_rules! define_test {
        ($(($name:ident, $func:ident, $lhs:expr, $rhs:expr, $expected_result:expr, $expected_flags:expr)),+) => {
            $(
            #[test]
            fn $name() {
                let lhs_bits = $lhs.to_bits();
                let rhs_bits = $rhs.to_bits();
				let expected_flags = $expected_flags;

                let (result, flags) = F32::$func(lhs_bits, rhs_bits);

                if result != $expected_result {
                     panic!("\nFor {} {} {}, Expected: {}, Got: {}\n", stringify!($lhs), stringify!($func), stringify!($rhs), $expected_result, result);
                }

				if flags.invalid_operation != expected_flags.invalid_operation {
					panic!("\nInvalid operation flag mismatch. Expected: {}, Got: {}\n", expected_flags.invalid_operation, flags.invalid_operation);
				}
            }
            )+
        };
    }

	const OK: ExceptionFlags = ExceptionFlags {
		invalid_operation: false,
		div_by_zero: false,
		overflow: false,
		underflow: false,
		inexact: false,
	};

	const INVALID: ExceptionFlags = ExceptionFlags {
		invalid_operation: true,
		div_by_zero: false,
		overflow: false,
		underflow: false,
		inexact: false,
	};

	define_test!(
		(eq_positives_equal, eq, 1.0f32, 1.0f32, true, OK),
		(eq_positives_unequal, eq, 1.0f32, 2.0f32, false, OK),
		(eq_plus_minus_zero, eq, 0.0f32, -0.0f32, true, OK),
		(eq_infinities, eq, f32::INFINITY, f32::INFINITY, true, OK),
		(eq_neg_infinities, eq, f32::NEG_INFINITY, f32::NEG_INFINITY, true, OK),
		(eq_inf_and_neg_inf, eq, f32::INFINITY, f32::NEG_INFINITY, false, OK),
		(eq_qnan_vs_num, eq, f32::NAN, 1.0f32, false, OK),
		(eq_qnan_vs_qnan, eq, f32::NAN, f32::NAN, false, OK),
		(eq_snan_vs_num, eq, f32::from_bits(0x7f800001), 1.0f32, false, INVALID),
		(lt_1_vs_2, lt, 1.0f32, 2.0f32, true, OK),
		(lt_2_vs_1, lt, 2.0f32, 1.0f32, false, OK),
		(lt_neg_2_vs_neg_1, lt, -2.0f32, -1.0f32, true, OK),
		(lt_neg_1_vs_neg_2, lt, -1.0f32, -2.0f32, false, OK),
		(lt_neg_1_vs_1, lt, -1.0f32, 1.0f32, true, OK),
		(lt_zeros, lt, 0.0f32, -0.0f32, false, OK),
		(lt_qnan, lt, 1.0f32, f32::NAN, false, INVALID),
		(lt_snan, lt, 1.0f32, f32::from_bits(0x7f800001), false, INVALID),
		(le_1_vs_2, le, 1.0f32, 2.0f32, true, OK),
		(le_2_vs_1, le, 2.0f32, 1.0f32, false, OK),
		(le_1_vs_1, le, 1.0f32, 1.0f32, true, OK),
		(le_neg_2_vs_neg_1, le, -2.0f32, -1.0f32, true, OK),
		(le_neg_1_vs_neg_2, le, -1.0f32, -2.0f32, false, OK),
		(le_neg_1_vs_1, le, -1.0f32, 1.0f32, true, OK),
		(le_zeros, le, 0.0f32, -0.0f32, true, OK),
		(le_qnan, le, 1.0f32, f32::NAN, false, INVALID),
		(le_snan, le, 1.0f32, f32::from_bits(0x7f800001), false, INVALID)
	);
}
