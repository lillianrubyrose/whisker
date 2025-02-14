pub struct Memory {
	inner: Box<[u8]>,
}

impl Memory {
	pub fn new(size: usize) -> Self {
		Self {
			inner: vec![0; size].into_boxed_slice(),
		}
	}

	pub fn read_u8(&self, offset: u64) -> u8 {
		let b0 = self.inner[offset as usize];
		u8::from_le_bytes([b0])
	}

	pub fn read_u16(&self, offset: u64) -> u16 {
		let b0 = self.inner[offset as usize];
		let b1 = self.inner[offset as usize + 1];
		u16::from_le_bytes([b0, b1])
	}

	pub fn read_u32(&self, offset: u64) -> u32 {
		let b0 = self.inner[offset as usize];
		let b1 = self.inner[offset as usize + 1];
		let b2 = self.inner[offset as usize + 2];
		let b3 = self.inner[offset as usize + 3];
		u32::from_le_bytes([b0, b1, b2, b3])
	}

	pub fn read_u64(&self, offset: u64) -> u64 {
		let b0 = self.inner[offset as usize];
		let b1 = self.inner[offset as usize + 1];
		let b2 = self.inner[offset as usize + 2];
		let b3 = self.inner[offset as usize + 3];
		let b4 = self.inner[offset as usize + 4];
		let b5 = self.inner[offset as usize + 5];
		let b6 = self.inner[offset as usize + 6];
		let b7 = self.inner[offset as usize + 7];
		u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7])
	}

	pub fn write_u8(&mut self, offset: u64, val: u8) {
		self.inner[offset as usize] = val;
	}

	pub fn write_u16(&mut self, offset: u64, val: u16) {
		let [b0, b1] = u16::to_le_bytes(val);
		self.inner[offset as usize] = b0;
		self.inner[offset as usize + 1] = b1;
	}

	pub fn write_u32(&mut self, offset: u64, val: u32) {
		let [b0, b1, b2, b3] = u32::to_le_bytes(val);
		self.inner[offset as usize] = b0;
		self.inner[offset as usize + 1] = b1;
		self.inner[offset as usize + 2] = b2;
		self.inner[offset as usize + 3] = b3;
	}

	pub fn write_u64(&mut self, offset: u64, val: u64) {
		let [b0, b1, b2, b3, b4, b5, b6, b7] = u64::to_le_bytes(val);
		self.inner[offset as usize] = b0;
		self.inner[offset as usize + 1] = b1;
		self.inner[offset as usize + 2] = b2;
		self.inner[offset as usize + 3] = b3;
		self.inner[offset as usize + 4] = b4;
		self.inner[offset as usize + 5] = b5;
		self.inner[offset as usize + 6] = b6;
		self.inner[offset as usize + 7] = b7;
	}

	/// TODO: handle wrapping the address space and such
	pub fn write_slice(&mut self, offset: u64, bytes: &[u8]) {
		for (idx, val) in bytes.into_iter().enumerate() {
			let offset = offset + idx as u64;
			self.write_u8(offset, *val);
		}
	}
}
