use num_conv::{Extend, Truncate};

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

	fn is_nan(&self) -> bool {
		self.exponent == 0xFF && self.fraction != 0
	}

	fn is_signaling_nan(&self) -> bool {
		self.is_nan() && (self.fraction & 0x400000) == 0
	}

	fn is_infinity(&self) -> bool {
		self.exponent == 0xFF && self.fraction == 0
	}

	fn is_zero(&self) -> bool {
		self.exponent == 0 && self.fraction == 0
	}

	fn is_subnormal(&self) -> bool {
		self.exponent == 0 && self.fraction != 0
	}
}

impl F32 {
	pub fn add(lhs_bits: u32, rhs_bits: u32, rm: RoundingMode) -> (u32, ExceptionFlags) {
		let lhs = F32::from(lhs_bits);
		let rhs = F32::from(rhs_bits);

		let mut flags = ExceptionFlags::default();

		// =============
		// Special Cases
		// Reference: Section 6
		// =============

		// Either sNaN: invalid operation and return qNaN
		// Section 7.2
		if lhs.is_signaling_nan() || rhs.is_signaling_nan() {
			flags.invalid();
			return (Self::Q_NAN, flags);
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
			return (rhs.sign, flags);
		}

		// Section 6.3
		if lhs.is_zero() && rhs.is_zero() {
			// Signs are different: +0 unless RoundTowardNegative
			if lhs.sign != rhs.sign {
				return (if rm == RoundTowardNegative { 0x80000000 } else { 0 }, flags);
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
		let mut significand_lhs = lhs.fraction.extend::<u64>();
		let mut significand_rhs = rhs.fraction.extend::<u64>();

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
				let res = if rm == RoundTowardNegative { 0x80000000 } else { 0 };
				return (res, flags);
			}
		}

		// =========================================================================
		// Shift the result significand left or right until it is in the form 1.XXXX
		// and adjust the exponent
		// =========================================================================

		let mut result_exponent = exponent_lhs.max(exponent_rhs);

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
			let final_exp = if result_fraction == 0 { 0xFF } else { 0xFE };
			return ((result_sign << 31) | (final_exp << 23) | result_fraction, flags);
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

			let mut final_fraction = result_significand >> (shift + 3);
			let lsb = final_fraction & 1;

			let needs_round = match rm {
				RoundTiesToEven => guard == 1 && (round_sticky || lsb == 1),
				RoundTiesToAway => guard == 1,
				RoundTowardPositive => result_sign == 0 && lost_bits,
				RoundTowardNegative => result_sign == 1 && lost_bits,
				RoundTowardZero => false,
			};

			if needs_round {
				final_fraction += 1;
			}

			// Did rounding a subnormal make it normal?
			if final_fraction == (1 << 23) {
				return ((result_sign << 31) | (1 << 23) | 0, flags);
			}

			return (
				(result_sign << 31) | (0 << 23) | (final_fraction as u32 & 0x7FFFFF),
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
		let mut final_significand = result_significand >> 3;

		let should_round = match rm {
			RoundTiesToEven => guard == 1 && (round == 1 || sticky == 1 || pre_round_result_significand_lsb == 1),
			RoundTiesToAway => guard == 1,
			RoundTowardPositive => result_sign == 0 && pre_round_inexact,
			RoundTowardNegative => result_sign == 1 && pre_round_inexact,
			RoundTowardZero => false,
		};

		if should_round {
			final_significand += 1;

			// Did rounding cause an overflow to the next exponent?
			if final_significand >= (2 << 23) {
				final_significand >>= 1;
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

		// Remove implicit bit
		let final_fraction = final_significand.truncate::<u32>() & 0x7FFFFF;
		let final_exponent = result_exponent.cast_unsigned();

		((result_sign << 31) | (final_exponent << 23) | final_fraction, flags)
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
						panic!();
					}
				} else if result_bits != expected_bits {
					panic!();
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
