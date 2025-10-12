#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
	/// First index in the range (inclusive)
	pub start: usize,
	/// Last index in the range (inclusive)
	pub end: usize,
}

impl Range {
	pub const fn new(start: usize, end: usize) -> Self {
		debug_assert!(start <= end, "end must be >= start");
		Self { start, end }
	}
}
