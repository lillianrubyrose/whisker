use std::io::{Cursor, Read};

use crate::Endianness;

pub trait ReadExt {
	fn read_u8(&mut self) -> Result<u8, std::io::Error>;
	fn read_16(&mut self, endianness: Endianness) -> Result<u16, std::io::Error>;
	fn read_32(&mut self, endianness: Endianness) -> Result<u32, std::io::Error>;
	fn read_64(&mut self, endianness: Endianness) -> Result<u64, std::io::Error>;
}

impl ReadExt for Cursor<&[u8]> {
	fn read_u8(&mut self) -> Result<u8, std::io::Error> {
		let mut buf = [0; 1];
		self.read_exact(&mut buf)?;
		Ok(buf[0])
	}

	fn read_16(&mut self, endianness: Endianness) -> Result<u16, std::io::Error> {
		let mut buf = [0; 2];
		self.read_exact(&mut buf)?;
		match endianness {
			Endianness::Little => Ok(u16::from_le_bytes(buf)),
			Endianness::Big => Ok(u16::from_be_bytes(buf)),
		}
	}

	fn read_32(&mut self, endianness: Endianness) -> Result<u32, std::io::Error> {
		let mut buf = [0; 4];
		self.read_exact(&mut buf)?;
		match endianness {
			Endianness::Little => Ok(u32::from_le_bytes(buf)),
			Endianness::Big => Ok(u32::from_be_bytes(buf)),
		}
	}

	fn read_64(&mut self, endianness: Endianness) -> Result<u64, std::io::Error> {
		let mut buf = [0; 8];
		self.read_exact(&mut buf)?;
		match endianness {
			Endianness::Little => Ok(u64::from_le_bytes(buf)),
			Endianness::Big => Ok(u64::from_be_bytes(buf)),
		}
	}
}
