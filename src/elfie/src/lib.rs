use std::io::{Cursor, Read, Seek};

use crate::ext::ReadExt;

mod ext;

#[derive(Debug, Clone, Copy)]
pub enum Endianness {
	Little,
	Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
	X32,
	X64,
}

#[derive(Debug)]
pub enum ISA {
	RiscV,
}

impl ISA {
	pub fn parse(cursor: &mut Cursor<&[u8]>) -> Result<Self, std::io::Error> {
		let value = cursor.read_u8()?;
		match value {
			0xF3 => Ok(ISA::RiscV),
			_ => Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ISA value",
			)),
		}
	}
}

pub struct ProgramHeaderType(i32);

impl ProgramHeaderType {
	pub const PT_NULL: Self = Self(0x00000000);
	pub const PT_LOAD: Self = Self(0x00000001);
	pub const PT_DYNAMIC: Self = Self(0x00000002);
	pub const PT_INTERP: Self = Self(0x00000003);
	pub const PT_NOTE: Self = Self(0x00000004);
	pub const PT_SHLIB: Self = Self(0x00000005);
	pub const PT_PHDR: Self = Self(0x00000006);
	pub const PT_TLS: Self = Self(0x00000007);
}

pub struct ProgramHeader {
	pub ty: ProgramHeaderType,
	pub flags: i32,
	pub offset: u64,
	pub size_in_file: u64,
	pub virtual_address: u64,
	pub physical_address: u64,
	pub size_in_memory: u64,
	pub alignment: u64,
}

pub struct SectionHeaderFlags(u64);

impl SectionHeaderFlags {
	pub const SHF_WRITE: Self = Self(0x1);
	pub const SHF_ALLOC: Self = Self(0x2);
	pub const SHF_EXECINSTR: Self = Self(0x4);
	pub const SHF_MERGE: Self = Self(0x10);
	pub const SHF_STRINGS: Self = Self(0x20);
	pub const SHF_INFO_LINK: Self = Self(0x40);
	pub const SHF_LINK_ORDER: Self = Self(0x80);
	pub const SHF_OS_NONCONFORMING: Self = Self(0x100);
	pub const SHF_GROUP: Self = Self(0x200);
	pub const SHF_TLS: Self = Self(0x400);
	pub const SHF_MASKOS: Self = Self(0x0ff00000);
	pub const SHF_MASKPROC: Self = Self(0xf0000000);
	pub const SHF_ORDERED: Self = Self(0x4000000);
	pub const SHF_EXCLUDE: Self = Self(0x8000000);

	pub fn contains(&self, other: Self) -> bool {
		(self.0 & other.0) == other.0
	}

	pub fn intersects(&self, other: Self) -> bool {
		(self.0 & other.0) != 0
	}

	pub fn insert(&mut self, other: Self) {
		self.0 |= other.0;
	}

	pub fn remove(&mut self, other: Self) {
		self.0 &= !other.0;
	}

	pub fn toggle(&mut self, other: Self) {
		self.0 ^= other.0;
	}

	pub fn set(&mut self, other: Self, value: bool) {
		if value {
			self.insert(other);
		} else {
			self.remove(other);
		}
	}
}

impl std::ops::BitOr for SectionHeaderFlags {
	type Output = Self;

	fn bitor(self, rhs: Self) -> Self::Output {
		Self(self.0 | rhs.0)
	}
}

impl std::ops::BitOrAssign for SectionHeaderFlags {
	fn bitor_assign(&mut self, rhs: Self) {
		self.0 |= rhs.0;
	}
}

impl std::ops::BitAnd for SectionHeaderFlags {
	type Output = Self;

	fn bitand(self, rhs: Self) -> Self::Output {
		Self(self.0 & rhs.0)
	}
}

impl std::ops::BitAndAssign for SectionHeaderFlags {
	fn bitand_assign(&mut self, rhs: Self) {
		self.0 &= rhs.0;
	}
}

impl std::ops::BitXor for SectionHeaderFlags {
	type Output = Self;

	fn bitxor(self, rhs: Self) -> Self::Output {
		Self(self.0 ^ rhs.0)
	}
}

impl std::ops::BitXorAssign for SectionHeaderFlags {
	fn bitxor_assign(&mut self, rhs: Self) {
		self.0 ^= rhs.0;
	}
}

impl std::ops::Not for SectionHeaderFlags {
	type Output = Self;

	fn not(self) -> Self::Output {
		Self(!self.0)
	}
}

pub struct SectionHeaderType(i32);

impl SectionHeaderType {
	pub const SHT_NULL: Self = Self(0x0);
	pub const SHT_PROGBITS: Self = Self(0x1);
	pub const SHT_SYMTAB: Self = Self(0x2);
	pub const SHT_STRTAB: Self = Self(0x3);
	pub const SHT_RELA: Self = Self(0x4);
	pub const SHT_HASH: Self = Self(0x5);
	pub const SHT_DYNAMIC: Self = Self(0x6);
	pub const SHT_NOTE: Self = Self(0x7);
	pub const SHT_NOBITS: Self = Self(0x8);
	pub const SHT_REL: Self = Self(0x9);
	pub const SHT_SHLIB: Self = Self(0x0A);
	pub const SHT_DYNSYM: Self = Self(0x0B);
	pub const SHT_INIT_ARRAY: Self = Self(0x0E);
	pub const SHT_FINI_ARRAY: Self = Self(0x0F);
	pub const SHT_PREINIT_ARRAY: Self = Self(0x10);
	pub const SHT_GROUP: Self = Self(0x11);
	pub const SHT_SYMTAB_SHNDX: Self = Self(0x12);
	pub const SHT_NUM: Self = Self(0x13);
}

pub struct SectionHeader {
	pub name: String,
	pub name_offset: u32,
	pub ty: SectionHeaderType,
	pub flags: SectionHeaderFlags,
	pub offset: u64,
	pub size: u64,
	pub virtual_address: u64,
	pub alignment: u64,
	pub link: u32,
	pub info: u32,
	pub entry_size: u64,
}

#[derive(Debug)]
pub struct ElfType(u16);

impl ElfType {
	pub const ET_NONE: Self = Self(0x00);
	pub const ET_REL: Self = Self(0x01);
	pub const ET_EXEC: Self = Self(0x02);
	pub const ET_DYN: Self = Self(0x03);
	pub const ET_CORE: Self = Self(0x04);
}

pub struct ElfFile {
	pub version: i32,
	pub class: Class,
	pub endianness: Endianness,
	pub ty: ElfType,
	pub isa: ISA,
	pub abi: i32,
	pub abi_version: i32,
	pub flags: i32,
	pub entry: u64,
	pub section_name_entry_index: u32,
	pub header_size: u32,

	pub program_header_table_offset: u64,
	pub program_header_table_entry_count: u32,
	pub program_header_table_entry_size: u32,
	pub program_headers: Vec<ProgramHeader>,

	pub section_header_table_offset: u64,
	pub section_header_table_entry_count: u32,
	pub section_header_table_entry_size: u32,
	pub section_headers: Vec<SectionHeader>,
}

impl ElfFile {
	pub fn parse(data: &[u8]) -> Result<Self, std::io::Error> {
		let mut cursor = Cursor::new(data);

		let mut magic = [0; 4];
		cursor.read_exact(&mut magic)?;

		if magic != *b"\x7FELF" {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ELF magic",
			));
		}

		let class = match cursor.read_u8()? {
			1 => Class::X32,
			2 => Class::X64,
			_ => {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Invalid ELF class",
				));
			}
		};
		assert_eq!(class, Class::X64, "We dont support 32-bit ELF files");

		let endianness = match cursor.read_u8()? {
			1 => Endianness::Little,
			2 => Endianness::Big,
			_ => {
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Invalid ELF endianness",
				));
			}
		};

		let version = cursor.read_u8()?;
		if version != 1 {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ELF header version",
			));
		}

		let abi = cursor.read_u8()?;
		let abi_version = cursor.read_u8()?;

		cursor.seek_relative(7)?; // skip padding

		let ty = ElfType(cursor.read_16(endianness)?);
		let isa = ISA::parse(&mut cursor)?;
		let version = cursor.read_32(endianness)?;
		if version != 1 {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ELF version",
			));
		}

		let entrypoint = cursor.read_64(endianness)?;
		let program_header_table_offset = cursor.read_64(endianness)?;
		let section_header_table_offset = cursor.read_64(endianness)?;
		let flags = cursor.read_32(endianness)?;
		let header_size = cursor.read_32(endianness)?;

		let program_header_table_entry_size = cursor.read_16(endianness)?;
		let program_header_table_entry_count = cursor.read_16(endianness)?;
		let section_header_table_entry_size = cursor.read_16(endianness)?;
		let section_header_table_entry_count = cursor.read_16(endianness)?;
		let section_header_table_index = cursor.read_16(endianness)?;

		dbg!(
			class,
			endianness,
			ty,
			isa,
			version,
			entrypoint,
			program_header_table_offset,
			section_header_table_offset,
			flags,
			header_size,
			program_header_table_entry_size,
			program_header_table_entry_count,
			section_header_table_entry_size,
			section_header_table_entry_count,
			section_header_table_index
		);

		todo!()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_elf_file() {
		let data = include_bytes!("../../../target/boot.o");
		let elf = ElfFile::parse(data).unwrap();
	}
}