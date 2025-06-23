use num_conv::Extend;

/// extracts bits start..=end from val
pub fn extract_bits_8(val: u8, start: u8, end: u8) -> u8 {
	assert!(start <= end);
	assert!(start < u8::BITS as u8);
	assert!(end < u8::BITS as u8);

	// masks off the low bits
	let low_mask = (u8::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u8::MAX << (u8::BITS - u32::from(end) - 1)) >> (u8::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
pub fn extract_bits_16(val: u16, start: u8, end: u8) -> u16 {
	assert!(start <= end);
	assert!(start < u16::BITS as u8);
	assert!(end < u16::BITS as u8);

	// masks off the low bits
	let low_mask = (u16::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u16::MAX << (u16::BITS - u32::from(end) - 1)) >> (u16::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
pub fn extract_bits_32(val: u32, start: u8, end: u8) -> u32 {
	assert!(start <= end);
	assert!(start < u32::BITS as u8);
	assert!(end < u32::BITS as u8);

	// masks off the low bits
	let low_mask = (u32::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u32::MAX << (u32::BITS - u32::from(end) - 1)) >> (u32::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
pub fn extract_bits_64(val: u64, start: u8, end: u8) -> u64 {
	assert!(start <= end);
	assert!(start < u64::BITS as u8);
	assert!(end < u64::BITS as u8);

	// masks off the low bits
	let low_mask = (u64::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u64::MAX << (u64::BITS - u32::from(end) - 1)) >> (u64::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extract the bit at position `bit` from `val`
pub fn extract_bit_64(val: u64, bit: u8) -> u64 {
	debug_assert!(bit < u64::BITS as u8);
	(val & (1 << bit)) >> bit
}

/// inserts a value into bits start..=end in the input val
/// the inserted value must fit within the specified bit range
pub fn insert_bits_64(val: u64, insert: u64, start: u8, end: u8) -> u64 {
	assert!(start <= end);
	assert!(start < u64::BITS as u8);
	assert!(end < u64::BITS as u8);

	// mask off the low bits
	let low_mask = (u64::MAX >> start) << start;
	// mask off the high bits
	let high_mask = (u64::MAX << (u64::BITS - u32::from(end) - 1)) >> (u64::BITS - u32::from(end) - 1);
	// invert the combination of these bits to get the mask that deletes the bits that are being replaced
	let mask = !(low_mask & high_mask);
	(val & mask) | (insert << start)
}

/// inserts the low bit of `insert` into `val` at bit position `bit`
pub fn insert_bit_64(val: u64, insert: u8, bit: u8) -> u64 {
	debug_assert!(insert & 0b1111_1110 == 0, "insert must only have the low bit set");
	debug_assert!(bit < u64::BITS as u8);

	(val & !(1_u64 << bit)) | (insert.extend::<u64>() << bit)
}

pub fn sign_ext_imm(imm: u32, sign_bit_idx: u8) -> i64 {
	let sign_mask = 1 << sign_bit_idx;
	let high_bits = if imm & sign_mask != 0 {
		i64::MIN >> (64 - sign_bit_idx - 1)
	} else {
		0
	};
	(imm as i64) | high_bits
}
