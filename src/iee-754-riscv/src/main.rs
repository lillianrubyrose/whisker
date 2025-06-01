use num_conv::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

const EXPONENT_BIAS: i8 = 127;
const EXPONENT_MAX: i8 = 127;
const EXPONENT_MIN: i8 = 1 - EXPONENT_MAX;
const EXPONENT_NON_FINITE: i8 = -128;

const EXPONENT_MASK: u32 = 0x7F80_0000;

const SIGN_MASK: u32 = 0x8000_0000;

const MANTISSA_BITS: u32 = 23;
const MANTISSA_MASK: u32 = 0x007F_FFFF;

/// zero indexed bit position of the implicit 1/0 bit in the mantissa
const MANTISSA_IMPLICIT_BIT_POS: u32 = 23;

#[derive(Clone, Copy)]
pub struct F32 {
	/// the raw bits of the float, in IEEE-754 binary interchange format
	/// this is equivalent to the format used by f32::to_bits
	bits: u32,
}

impl std::fmt::Debug for F32 {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:#010X}", self.bits)
	}
}

impl PartialEq<f32> for F32 {
	fn eq(&self, other: &f32) -> bool {
		f32::from_bits(self.to_bits()) == *other
	}
}

impl PartialEq<F32> for f32 {
	fn eq(&self, other: &F32) -> bool {
		other == self
	}
}

impl From<f32> for F32 {
	fn from(value: f32) -> Self {
		F32::from_bits(value.to_bits())
	}
}

impl F32 {
	const IMPLICIT_BIT_MASK: u32 = 1 << 23;

	pub fn from_bits(bits: u32) -> Self {
		Self { bits }
	}

	pub fn to_bits(self) -> u32 {
		self.bits
	}

	pub fn is_nan(self) -> bool {
		let exponent = ((self.bits & EXPONENT_MASK) >> MANTISSA_BITS) as u8;
		exponent == 255 && (self.bits & MANTISSA_MASK) != 0
	}

	fn unpack(self) -> UnpackedF32 {
		let sign = (self.bits & SIGN_MASK) != 0;
		let raw_exponent = ((self.bits & EXPONENT_MASK) >> MANTISSA_BITS) as u8;
		let raw_mantissa = self.bits & MANTISSA_MASK;

		const NON_FINITE_EXPONENT: u8 = 255;
		const ZERO_EXP: u8 = 0;
		const MIN_FINITE_EXPONENT: u8 = 1;
		const MAX_FINITE_EXPONENT: u8 = 254;

		// doc: Figure 3.1
		let (exponent, mantissa) = match raw_exponent {
			// NaN
			NON_FINITE_EXPONENT if raw_mantissa != 0 => (EXPONENT_NON_FINITE, raw_mantissa),
			// Inf
			NON_FINITE_EXPONENT => (EXPONENT_NON_FINITE, 0),
			// normal
			MIN_FINITE_EXPONENT..=MAX_FINITE_EXPONENT => {
				(unbias_exponent(raw_exponent), raw_mantissa | Self::IMPLICIT_BIT_MASK)
			}
			// subnormal/zero
			// case works for both since: (0 + 2^(1-p) * T)
			ZERO_EXP => (EXPONENT_MIN, raw_mantissa),
		};

		UnpackedF32 {
			sign,
			exponent,
			mantissa,
		}
	}

	fn handle_special_cases(a: UnpackedF32, b: UnpackedF32) -> Option<(F32, ExceptionFlags)> {
		let mut flags = ExceptionFlags::default();

		// NaN propagation
		if a.is_nan() || b.is_nan() {
			if Self::is_signaling_nan(a) || Self::is_signaling_nan(b) {
				flags.invalid_operation = true;
			}

			// return quiet NaN, prioritize a
			let nan_bits = if a.is_nan() {
				a.to_bits() | 0x0040_0000
			} else {
				b.to_bits() | 0x0040_0000
			};
			return Some((F32::from_bits(nan_bits), flags));
		}

		if a.is_infinity() && b.is_infinity() {
			if a.sign != b.sign {
				// inf + (-inf) = NaN
				flags.invalid_operation = true;
				return Some((F32::from_bits(0x7FC0_0000), flags)); // quiet NaN
			} else {
				// inf + inf = inf
				let inf_bits = if a.sign { 0xFF80_0000 } else { 0x7F80_0000 };
				return Some((F32::from_bits(inf_bits), flags));
			}
		}

		if a.is_infinity() {
			return Some((F32::from_bits(a.to_bits()), flags));
		}

		if b.is_infinity() {
			return Some((F32::from_bits(b.to_bits()), flags));
		}

		None
	}

	fn is_signaling_nan(unpacked: UnpackedF32) -> bool {
		unpacked.is_nan() && (unpacked.mantissa & 0x0040_0000) == 0
	}

	fn align_mantissas(mut a: UnpackedF32, mut b: UnpackedF32) -> (UnpackedF32, UnpackedF32, i8) {
		if a.is_zero() && b.is_zero() {
			return (a, b, 0);
		}

		if a.is_zero() {
			a.exponent = b.exponent;
			return (a, b, 0);
		}

		if b.is_zero() {
			b.exponent = a.exponent;
			return (a, b, 0);
		}

		let exp_diff = a.exponent - b.exponent;

		match exp_diff {
			// b has larger exponent, shift a's mantissa right
			..=-32 => {
				a.mantissa = if a.mantissa != 0 { 1 } else { 0 }; // sticky bit
				a.exponent = b.exponent;
			}
			-31..=-1 => {
				let sticky = if a.mantissa & ((1u32 << -exp_diff) - 1) != 0 {
					1
				} else {
					0
				};
				a.mantissa = (a.mantissa >> -exp_diff) | sticky;
				a.exponent = b.exponent;
			}

			// same magnitude, no adjustments needed
			0 => {}

			// a has larger exponent, shift b's mantissa right
			1..=31 => {
				let sticky = if b.mantissa & ((1u32 << exp_diff) - 1) != 0 {
					1
				} else {
					0
				};
				b.mantissa = (b.mantissa >> exp_diff) | sticky;
				b.exponent = a.exponent;
			}
			32.. => {
				b.mantissa = if b.mantissa != 0 { 1 } else { 0 }; // sticky bit
				b.exponent = a.exponent;
			}
		}

		(a, b, exp_diff)
	}

	fn add_aligned_mantissas(a: UnpackedF32, b: UnpackedF32) -> (bool, i8, u32, bool) {
		let same_sign = a.sign == b.sign;
		let result_sign;
		let mantissa_sum;
		let carry;

		if same_sign {
			// add magnitudes
			result_sign = a.sign;
			mantissa_sum = a.mantissa + b.mantissa;
			carry = mantissa_sum >= (2 * Self::IMPLICIT_BIT_MASK);
		} else {
			// subtract magnitudes
			if a.mantissa >= b.mantissa {
				result_sign = a.sign;
				mantissa_sum = a.mantissa - b.mantissa;
			} else {
				result_sign = b.sign;
				mantissa_sum = b.mantissa - a.mantissa;
			}
			carry = false;
		}

		(result_sign, a.exponent, mantissa_sum, carry)
	}

	fn pack_f32(sign: bool, exponent: i8, mantissa: u32) -> u32 {
		let sign_bit = if sign { 1u32 << 31 } else { 0 };
		let biased_exponent = bias_exponent(exponent);
		let exp_bits = (biased_exponent.cast_unsigned().extend::<u32>()) << MANTISSA_BITS;
		let mantissa_bits = mantissa & MANTISSA_MASK;

		println!(
			"PACK DEBUG: sign={}, exp={}, biased_exp={}, mantissa=0x{:X}, mantissa_bits=0x{:X}, final=0x{:08X}",
			sign,
			exponent,
			biased_exponent,
			mantissa,
			mantissa_bits,
			sign_bit | exp_bits | mantissa_bits
		);

		sign_bit | exp_bits | mantissa_bits
	}

	fn round_to_f32(
		sign: bool,
		mut exponent: i8,
		mut mantissa: u32,
		rounding_mode: RoundingMode,
	) -> (F32, ExceptionFlags) {
		let mut flags = ExceptionFlags::default();

		// FIXME: non-finite exp
		if mantissa == 0 {
			return (F32::from_bits(if sign { 0x8000_0000 } else { 0 }), flags);
		}

		println!("ROUND DEBUG: Initial - exp={}, mantissa=0x{:X}", exponent, mantissa);

		// handle carry from addition
		if mantissa >= (2 * Self::IMPLICIT_BIT_MASK) {
			mantissa >>= 1;
			// FIXME: i think this is sus and probably needs to handle inf
			exponent += 1;
			println!(
				"ROUND DEBUG: Handled carry - exp={}, mantissa=0x{:X}",
				exponent, mantissa
			);
		}

		// find leading 1 bit
		let leading_zeros = mantissa.leading_zeros();
		let msb_pos = u32::BITS - 1 - leading_zeros;

		// get the signed difference between the actual MSB of the mantissa and the target
		let shift = msb_pos
			.wrapping_sub(MANTISSA_IMPLICIT_BIT_POS)
			.cast_signed()
			// bit positions fit in an i8, and this is convenient for exponents
			.truncate::<i8>();

		// collect any bits that will be lost in shifting (for inexact flag and rounding)
		let mut guard_bit = 0;
		let mut round_bit = 0;
		let mut sticky_bit = 0;
		println!("ROUND DEBUG: shift={}", shift);

		match shift {
			1 => {
				guard_bit = mantissa & 1;
				round_bit = 0;
				sticky_bit = 0;
				mantissa >>= 1;
				exponent += shift;
			}
			2 => {
				guard_bit = (mantissa >> 1) & 1;
				round_bit = mantissa & 1;
				sticky_bit = 0;
				mantissa >>= 2;
				exponent += shift;
			}
			shift if shift >= 3 => {
				// mantissa too large, shift right
				guard_bit = (mantissa >> (shift - 1)) & 1;
				round_bit = (mantissa >> (shift - 2)) & 1;
				sticky_bit = if (mantissa & ((1u32 << (shift - 2)) - 1)) != 0 {
					1
				} else {
					0
				};
				mantissa >>= shift;
				exponent += shift;
			}
			shift if shift < 0 => {
				mantissa <<= -shift; // mantissa too small, shift left
				exponent += shift; // subtract abs(shift)
			}
			_ => {}
		}

		println!(
			"ROUND DEBUG: after normalization - exp={}, mantissa=0x{:X}",
			exponent, mantissa
		);
		println!(
			"ROUND DEBUG: guard={}, round={}, sticky={}",
			guard_bit, round_bit, sticky_bit
		);

		let round_up = match rounding_mode {
			RoundingMode::RoundToNearestTieEven => {
				if guard_bit == 0 {
					false
				} else if round_bit != 0 || sticky_bit != 0 {
					true
				} else {
					(mantissa & 1) != 0 // tie, round to even
				}
			}
			RoundingMode::RoundTowardsZero => false,
			RoundingMode::RoundDown => sign && (guard_bit != 0 || round_bit != 0 || sticky_bit != 0),
			RoundingMode::RoundUp => !sign && (guard_bit != 0 || round_bit != 0 || sticky_bit != 0),
			RoundingMode::RoundToNearestTiesMaxMagnitude => {
				guard_bit != 0 && (round_bit != 0 || sticky_bit != 0 || true)
			}
		};

		println!("ROUND DEBUG: round_up={}", round_up);

		if round_up {
			mantissa += 1;

			// check for overflow
			if mantissa >= (1u32 << 24) {
				mantissa >>= 1;
				exponent += 1;
			}
		}

		// if we discarded any bits, set inexact flag
		if guard_bit != 0 || round_bit != 0 || sticky_bit != 0 {
			flags.inexact = true;
		}

		println!(
			"ROUND DEBUG: After rounding - exp={}, mantissa=0x{:X}, inexact={}",
			exponent, mantissa, flags.inexact
		);

		// overflow
		// FIXME: overflow and underflow code doesnt work right, at least not when dealing with zeros. 0 + 90000 still results in zero
		// maybe not even a flaw of this function, probably not. mew.
		if exponent == EXPONENT_MAX {
			flags.overflow = true;
			flags.inexact = true;

			let result_bits = match rounding_mode {
				RoundingMode::RoundToNearestTieEven | RoundingMode::RoundToNearestTiesMaxMagnitude => {
					if sign {
						0xFF800000
					} else {
						0x7F800000
					}
				}
				RoundingMode::RoundTowardsZero => {
					if sign {
						0xFF7FFFFF
					} else {
						0x7F7FFFFF
					}
				}
				RoundingMode::RoundDown => {
					if sign {
						0xFF800000
					} else {
						0x7F7FFFFF
					}
				}
				RoundingMode::RoundUp => {
					if sign {
						0xFF7FFFFF
					} else {
						0x7F800000
					}
				}
			};
			return (F32::from_bits(result_bits), flags);
		}

		// underflow
		// idk mew. this case gets hit but causes failure still.
		if exponent == EXPONENT_MIN {
			flags.underflow = true;
			flags.inexact = true;

			// rounds to zero in all rounding modes for severe underflow
			println!(
				"UNDERFLOW DEBUG: rounding to zero: {:?}",
				F32::from_bits(if sign { 0x8000_0000 } else { 0 })
			);
			return (F32::from_bits(if sign { 0x8000_0000 } else { 0 }), flags);
		}

		let result_bits = Self::pack_f32(sign, exponent, mantissa);
		println!("ROUND DEBUG: Final result=0x{:08X}", result_bits);

		(F32::from_bits(result_bits), flags)
	}

	pub fn add(self, other: F32, rounding_mode: RoundingMode) -> (F32, ExceptionFlags) {
		let a = self.unpack();
		let b = other.unpack();

		// special cases first or it gets angwy
		if let Some(result) = Self::handle_special_cases(a, b) {
			return result;
		}

		let (aligned_a, aligned_b, _exp_diff) = Self::align_mantissas(a, b);
		println!("aligned_a={aligned_a:?}, aligned_b={aligned_b:?}, _exp_diff={_exp_diff:X}");

		let (mut result_sign, result_exp, result_mantissa, _carry) = Self::add_aligned_mantissas(aligned_a, aligned_b);

		// FIXME: when self and other are 0, this has a special case for the special case
		// when the result of a sum is a precise zero, the sign is + unless rounding mode is round toward negative
		if result_mantissa == 0 {
			result_sign = rounding_mode == RoundingMode::RoundDown;
		}

		Self::round_to_f32(result_sign, result_exp, result_mantissa, rounding_mode)
	}
}

#[derive(Debug, Clone, Copy)]
struct UnpackedF32 {
	/// the sign of the float, true if the sign bit is set, false otherwise
	sign: bool,
	/// the adjusted exponent of the float. values can range from -126..=127
	/// or -128 to represent non-finite exponents.
	exponent: i8,
	/// a "parsed" form of the mantissa of the float, represented as a 24 bit value such that
	/// bit 23 represents the implicit 0 or 1 in the "infinite precision" value of the mantissa
	/// and bits 0..=22 represent the fractional part of the value of the mantissa.
	mantissa: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExceptionFlags {
	pub invalid_operation: bool,
	pub divide_by_zero: bool,
	pub overflow: bool,
	pub underflow: bool,
	pub inexact: bool,
}

impl UnpackedF32 {
	fn to_bits(self) -> u32 {
		if self.is_nan() {
			// return the NaN with original mantissa bits
			let sign_bit = if self.sign { 0x8000_0000 } else { 0 };
			let exp_bits = 0x7F80_0000;
			let mantissa_bits = (self.mantissa as u32) & 0x007F_FFFF;
			return sign_bit | exp_bits | mantissa_bits;
		}

		if self.is_infinity() {
			// return infinity with correct sign
			return if self.sign { 0xFF80_0000 } else { 0x7F80_0000 };
		}

		if self.is_zero() || self.mantissa == 0 {
			// return signed zero
			return if self.sign { 0x8000_0000 } else { 0x0000_0000 };
		}

		let sign_bit = if self.sign { 0x8000_0000 } else { 0 };

		// check if this should be a subnormal number
		if self.exponent < (1 - EXPONENT_BIAS) {
			// this should be represented as subnormal
			let shift = (1 - EXPONENT_BIAS) - self.exponent;

			if shift >= 24 {
				// too small to represent, return signed zero
				return sign_bit;
			}

			// shift mantissa right and remove implicit bit
			let shifted_mantissa = if self.mantissa >= F32::IMPLICIT_BIT_MASK {
				(self.mantissa - F32::IMPLICIT_BIT_MASK) >> shift
			} else {
				self.mantissa >> shift
			};

			let mantissa_bits = (shifted_mantissa as u32) & 0x007F_FFFF;
			return sign_bit | mantissa_bits; // exponent is 0 for subnormal
		}

		// pack normal number
		let biased_exponent = bias_exponent(self.exponent);
		let exp_bits = biased_exponent.extend::<u32>() << 23;
		let mantissa_bits = self.mantissa & MANTISSA_MASK;

		sign_bit | exp_bits | mantissa_bits
	}

	#[must_use]
	fn is_nan(self) -> bool {
		self.exponent == EXPONENT_NON_FINITE && self.mantissa != 0
	}

	#[must_use]
	fn is_infinity(self) -> bool {
		self.exponent == EXPONENT_NON_FINITE && self.mantissa == 0
	}

	#[must_use]
	fn is_zero(self) -> bool {
		self.exponent == EXPONENT_MIN && self.mantissa == 0
	}
}

/// adjust an unbiased exponent from the range -126..=127 to the biased binary format of 1..=254
#[must_use]
fn bias_exponent(unbiased: i8) -> u8 {
	debug_assert!(unbiased != -127 && unbiased != -128, "invalid unbiased exponent");
	(unbiased.extend::<i16>() + EXPONENT_BIAS.extend::<i16>())
		.truncate::<i8>()
		.cast_unsigned()
}

/// adjust a biased exponent from the range 1..=254 to the range -126..=127
#[must_use]
fn unbias_exponent(biased: u8) -> i8 {
	debug_assert!(biased != 0 && biased != 255, "invalid biased exponent");
	(biased.extend::<u16>().cast_signed() - EXPONENT_BIAS.extend::<i16>()).truncate::<i8>()
}

impl ExceptionFlags {
	pub fn new() -> Self {
		Self {
			invalid_operation: false,
			divide_by_zero: false,
			overflow: false,
			underflow: false,
			inexact: false,
		}
	}
}

impl Default for ExceptionFlags {
	fn default() -> Self {
		Self::new()
	}
}

impl ExceptionFlags {
	pub fn no_exceptions(self) -> bool {
		!self.invalid_operation && !self.divide_by_zero && !self.overflow && !self.underflow && !self.inexact
	}
}

impl F32 {
	pub fn debug_bits(&self) -> String {
		let sign = if (self.bits & 0x8000_0000) != 0 { "1" } else { "0" };
		let exponent = (self.bits >> 23) & 0xFF;
		let mantissa = self.bits & 0x007F_FFFF;
		format!(
			"sign={}, exp={:02X} ({}), mantissa={:06X}",
			sign,
			exponent,
			exponent as i32 - 127,
			mantissa
		)
	}
}

#[cfg(test)]
mod tests {
	use std::f32;

	use super::*;

	#[test]
	fn test_unpacking() {
		let f1 = F32::from_bits(1.0f32.to_bits());
		let unpacked1 = f1.unpack();
		println!(
			"1.0 unpacked: sign={}, exp={}, mantissa=0x{:X}, zero={}, inf={}, nan={}",
			unpacked1.sign,
			unpacked1.exponent,
			unpacked1.mantissa,
			unpacked1.is_zero(),
			unpacked1.is_infinity(),
			unpacked1.is_zero()
		);

		let f2 = F32::from_bits(2.0f32.to_bits());
		let unpacked2 = f2.unpack();
		println!(
			"2.0 unpacked: sign={}, exp={}, mantissa=0x{:X}, zero={}, inf={}, nan={}",
			unpacked2.sign,
			unpacked2.exponent,
			unpacked2.mantissa,
			unpacked2.is_zero(),
			unpacked2.is_infinity(),
			unpacked2.is_zero()
		);

		assert_eq!(unpacked1.exponent, 0);
		assert_eq!(unpacked1.mantissa, 0x800000);
		assert_eq!(unpacked2.exponent, 1);
		assert_eq!(unpacked2.mantissa, 0x800000);
	}

	#[test]
	fn test_packing() {
		let packed1 = F32::pack_f32(false, 0, 0x800000);
		println!("Packed 1.0: 0x{:08X} (expected 0x3F800000)", packed1);
		assert_eq!(packed1, 1.0f32.to_bits());

		let packed2 = F32::pack_f32(false, 1, 0x800000);
		println!("Packed 2.0: 0x{:08X} (expected 0x40000000)", packed2);
		assert_eq!(packed2, 2.0f32.to_bits());

		let packed3 = F32::pack_f32(false, 1, 0xC00000);
		println!("Packed 3.0: 0x{:08X} (expected 0x40400000)", packed3);
		assert_eq!(packed3, 3.0f32.to_bits());
	}

	#[test]
	fn test_alignment() {
		let f1 = F32::from_bits(1.0f32.to_bits());
		let f2 = F32::from_bits(2.0f32.to_bits());
		let unpacked1 = f1.unpack();
		let unpacked2 = f2.unpack();

		println!("Before alignment:");
		println!("  1.0: exp={}, mantissa=0x{:X}", unpacked1.exponent, unpacked1.mantissa);
		println!("  2.0: exp={}, mantissa=0x{:X}", unpacked2.exponent, unpacked2.mantissa);

		let (aligned1, aligned2, exp_diff) = F32::align_mantissas(unpacked1, unpacked2);

		println!("After alignment (exp_diff={}):", exp_diff);
		println!("  1.0: exp={}, mantissa=0x{:X}", aligned1.exponent, aligned1.mantissa);
		println!("  2.0: exp={}, mantissa=0x{:X}", aligned2.exponent, aligned2.mantissa);

		// 2.0 has larger exponent, 1.0's mantissa should be shifted right
		assert_eq!(exp_diff, -1);
		assert_eq!(aligned1.exponent, aligned2.exponent);
	}

	#[test]
	fn test_mantissa_addition() {
		let f1 = F32::from_bits(1.0f32.to_bits());
		let f2 = F32::from_bits(2.0f32.to_bits());
		let unpacked1 = f1.unpack();
		let unpacked2 = f2.unpack();

		let (aligned1, aligned2, _exp_diff) = F32::align_mantissas(unpacked1, unpacked2);
		let (result_sign, result_exp, result_mantissa, carry) = F32::add_aligned_mantissas(aligned1, aligned2);

		println!("Addition result:");
		println!(
			"  sign={}, exp={}, mantissa=0x{:X}, carry={}",
			result_sign, result_exp, result_mantissa, carry
		);

		// mantissa should be 0xC00000 (1.5 * 2^23)
		assert!(!result_sign);
		assert_eq!(result_exp, 1);
		assert_eq!(result_mantissa, 0xC00000);
	}

	#[test]
	fn test_step_by_step_addition() {
		let a = F32::from_bits(1.0f32.to_bits());
		let b = F32::from_bits(2.0f32.to_bits());

		println!("Input: a=0x{:08X} (1.0), b=0x{:08X} (2.0)", a.to_bits(), b.to_bits());

		let unpacked_a = a.unpack();
		let unpacked_b = b.unpack();

		println!(
			"Unpacked a: sign={}, exp={}, mantissa=0x{:X}",
			unpacked_a.sign, unpacked_a.exponent, unpacked_a.mantissa
		);
		println!(
			"Unpacked b: sign={}, exp={}, mantissa=0x{:X}",
			unpacked_b.sign, unpacked_b.exponent, unpacked_b.mantissa
		);

		if let Some((result, _flags)) = F32::handle_special_cases(unpacked_a, unpacked_b) {
			println!("Special case result: 0x{:08X}", result.to_bits());
			return;
		}

		let (aligned_a, aligned_b, exp_diff) = F32::align_mantissas(unpacked_a, unpacked_b);

		println!("After alignment (exp_diff={}):", exp_diff);
		println!(
			"  aligned_a: exp={}, mantissa=0x{:X}",
			aligned_a.exponent, aligned_a.mantissa
		);
		println!(
			"  aligned_b: exp={}, mantissa=0x{:X}",
			aligned_b.exponent, aligned_b.mantissa
		);

		let (result_sign, result_exp, result_mantissa, carry) = F32::add_aligned_mantissas(aligned_a, aligned_b);

		println!("Mantissa addition result:");
		println!(
			"  sign={}, exp={}, mantissa=0x{:X}, carry={}",
			result_sign, result_exp, result_mantissa, carry
		);

		let (final_result, flags) = F32::round_to_f32(
			result_sign,
			result_exp,
			result_mantissa,
			RoundingMode::RoundToNearestTieEven,
		);

		println!("Final result: 0x{:08X} (expected 0x40400000)", final_result.to_bits());
		println!(
			"Flags: invalid={}, overflow={}, underflow={}, inexact={}",
			flags.invalid_operation, flags.overflow, flags.underflow, flags.inexact
		);

		assert_eq!(final_result.to_bits(), 0x40400000);
	}

	#[test]
	fn test_rounding_function() {
		let (result, flags) = F32::round_to_f32(
			false,    // positive
			1,        // exponent 1
			0xC00000, // mantissa for 1.5 (before normalization)
			RoundingMode::RoundToNearestTieEven,
		);

		println!("Rounding test:");
		println!("  Input: sign=false, exp=1, mantissa=0xC00000");
		println!("  Result: 0x{:08X} (expected 0x40400000)", result.to_bits());
		println!("  Flags: inexact={}", flags.inexact);

		assert_eq!(result.to_bits(), 3.0f32.to_bits());
		assert!(!flags.inexact);
	}

	#[test]
	fn test_basic_addition() {
		let a = F32::from_bits(1.0f32.to_bits());
		let b = F32::from_bits(2.0f32.to_bits());
		let (result, flags) = a.add(b, RoundingMode::RoundToNearestTieEven);

		println!("1.0 + 2.0 = 0x{:08X} (expected 0x40400000)", result.to_bits());
		println!("Result as f32: {}", f32::from_bits(result.to_bits()));
		println!("Expected as f32: {}", f32::from_bits(0x40400000));

		assert_eq!(result.to_bits(), 3.0f32.to_bits());
		assert!(!flags.inexact);
	}

	#[test]
	fn test_pack_with_bias() {
		// 1.0: unbiased_exp=0, should become biased_exp=127
		let result = F32::pack_f32(false, 0, 0x800000);
		println!("pack_f32(false, 0, 0x800000) = 0x{:08X} (expected 0x3F800000)", result);

		// 2.0: unbiased_exp=1, should become biased_exp=128
		let result = F32::pack_f32(false, 1, 0x800000);
		println!("pack_f32(false, 1, 0x800000) = 0x{:08X} (expected 0x40000000)", result);

		// 3.0: unbiased_exp=1, mantissa=0xC00000 (1.5), should become biased_exp=128, mantissa=0x400000
		let result = F32::pack_f32(false, 1, 0xC00000);
		println!("pack_f32(false, 1, 0xC00000) = 0x{:08X} (expected 0x40400000)", result);
	}

	#[test]
	fn test_zeros() {
		let positive_zero = F32::from(0.0_f32);
		let negative_zero = F32::from(-0.0_f32);

		let (res, e) = positive_zero.add(positive_zero, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, 0.0f32);
		assert!(e.no_exceptions());

		let (res, e) = positive_zero.add(negative_zero, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, 0.0f32);
		assert!(e.no_exceptions());

		let (res, e) = negative_zero.add(positive_zero, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, 0.0f32);
		assert!(e.no_exceptions());

		let (res, e) = negative_zero.add(negative_zero, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, -0.0f32);
		assert!(e.no_exceptions());
	}

	#[test]
	fn test_inf() {
		let positive_inf = F32::from(f32::INFINITY);
		let negative_inf = F32::from(f32::NEG_INFINITY);
		let one = F32::from(1_f32);

		let (res, e) = positive_inf.add(one, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = one.add(positive_inf, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = negative_inf.add(one, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::NEG_INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = one.add(negative_inf, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::NEG_INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = positive_inf.add(positive_inf, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = negative_inf.add(negative_inf, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res, f32::NEG_INFINITY);
		assert!(e.no_exceptions());

		let (res, e) = positive_inf.add(negative_inf, RoundingMode::RoundToNearestTieEven);
		assert!(res.is_nan());
		assert!(e.invalid_operation);

		let (res, e) = negative_inf.add(positive_inf, RoundingMode::RoundToNearestTieEven);
		assert!(res.is_nan());
		assert!(e.invalid_operation);
	}

	#[test]
	fn test_nan_addition() {
		let one = F32::from(1_f32);
		let inf = F32::from(f32::INFINITY);
		let nan = F32::from(f32::NAN);

		assert!(nan.add(one, RoundingMode::RoundToNearestTieEven).0.is_nan());
		assert!(one.add(nan, RoundingMode::RoundToNearestTieEven).0.is_nan());

		assert!(nan.add(inf, RoundingMode::RoundToNearestTieEven).0.is_nan());
		assert!(inf.add(nan, RoundingMode::RoundToNearestTieEven).0.is_nan());

		assert!(nan.add(nan, RoundingMode::RoundToNearestTieEven).0.is_nan());
	}

	#[test]
	fn add_signed_zero_results() {
		let two = F32::from(2.0_f32);
		let neg_two = F32::from(-2.0_f32);

		let (res, e) = neg_two.add(two, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res.to_bits(), 0.0f32.to_bits());
		assert!(e.no_exceptions());

		let (res, e) = two.add(neg_two, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res.to_bits(), 0.0f32.to_bits());
		assert!(e.no_exceptions());
	}

	#[test]
	fn addition_zero_plus_90k() {
		println!("addition_zero_plus_90k");
		let zero = F32::from(0f32);
		let ninety_k = F32::from(90000f32);

		let (res, e) = zero.add(ninety_k, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res.to_bits(), 90000f32.to_bits());
		assert!(e.no_exceptions());
	}

	#[test]
	fn addition_90k_plus_zero() {
		println!("addition_90k_plus_zero");
		let zero = F32::from(0f32);
		let ninety_k = F32::from(90000f32);

		let (res, e) = ninety_k.add(zero, RoundingMode::RoundToNearestTieEven);
		assert_eq!(res.to_bits(), 90000f32.to_bits());
		assert!(e.no_exceptions());
	}

	#[test]
	fn test_addition_big() {
		for i in -1000..900 {
			let a = i as f32 * 100.0;
			for j in 900..1000 {
				let b = j as f32 * 100.0;
				println!("START: {} + {}", a, b);
				let res = F32::from(a).add(F32::from(b), RoundingMode::RoundToNearestTieEven).0;
				assert_eq!(res.to_bits(), (a + b).to_bits(), "{res:?}");
				//				println!("\n\n\n\n");
			}
		}
	}
}

fn main() {
	println!("Hello, world!");
}
