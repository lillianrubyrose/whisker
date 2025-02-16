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

pub fn sign_ext_imm(imm: u32, sign_bit_idx: u8) -> i64 {
	let sign_mask = 1 << sign_bit_idx;
	let high_bits = if imm & sign_mask != 0 {
		i64::MIN >> (64 - sign_bit_idx - 1)
	} else {
		0
	};
	(imm as i64) | high_bits
}
