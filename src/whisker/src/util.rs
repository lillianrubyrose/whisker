use crate::tracing::*;

/// extracts bits start..=end from val
pub fn extract_bits_8(val: u8, start: u8, end: u8) -> u8 {
	debug_assert!(start <= end);
	debug_assert!(start < u8::BITS as u8);
	debug_assert!(end < u8::BITS as u8);

	// masks off the low bits
	let low_mask = (u8::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u8::MAX << (u8::BITS - u32::from(end) - 1)) >> (u8::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
pub fn extract_bits_16(val: u16, start: u8, end: u8) -> u16 {
	debug_assert!(start <= end);
	debug_assert!(start < u16::BITS as u8);
	debug_assert!(end < u16::BITS as u8);

	// masks off the low bits
	let low_mask = (u16::MAX >> start) << start;
	// shift off the high bits
	let high_mask = (u16::MAX << (u16::BITS - u32::from(end) - 1)) >> (u16::BITS - u32::from(end) - 1);
	(val & low_mask & high_mask) >> start
}

/// extracts bits start..=end from val
pub fn extract_bits_32(val: u32, start: u8, end: u8) -> u32 {
	debug_assert!(start <= end);
	debug_assert!(start < u32::BITS as u8);
	debug_assert!(end < u32::BITS as u8);

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

// FIXME: kitty doesn't show the socat output, maybe fix that?
#[cfg(unix)]
const KNOWN_TERMINALS: &[&str] = &["xterm", "alacritty", "ghostty", "konsole", "gnome-terminal"];

#[cfg(unix)]
pub fn find_terminal() -> Result<String, ()> {
	use std::env;
	use std::process::{Command, Stdio};

	let test_term = |term: &str| -> bool {
		let Ok(mut child) = Command::new(term)
			.arg("--version") // search for command
			.arg(term)
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.spawn()
		else {
			return false;
		};

		let _ = child.kill();
		trace!("found terminal `{}`", term);
		true
	};

	if let Ok(term) = env::var("TERMINAL") {
		if test_term(&term) {
			return Ok(term);
		}
	}

	for term in KNOWN_TERMINALS {
		if test_term(term) {
			return Ok(term.to_string());
		}
	}

	Err(())
}
