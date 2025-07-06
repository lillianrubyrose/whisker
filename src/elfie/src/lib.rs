use std::{
	io::{Cursor, Read, Seek},
	string,
};

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

#[derive(Debug, Clone, Copy)]
pub enum ISA {
	RiscV,
}

impl ISA {
	pub fn parse(cursor: &mut Cursor<&[u8]>, endianness: Endianness) -> Result<Self, std::io::Error> {
		let value = cursor.read_16(endianness)?;
		match value {
			0xF3 => Ok(ISA::RiscV),
			_ => Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ISA value",
			)),
		}
	}
}

#[derive(Debug)]
pub struct ProgramHeaderType(u32);

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

#[derive(Debug)]
pub struct ProgramHeader {
	pub ty: ProgramHeaderType,
	pub flags: u32,
	pub offset: u64,
	pub size_in_file: u64,
	pub virtual_address: u64,
	pub physical_address: u64,
	pub size_in_memory: u64,
	pub alignment: u64,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct SectionHeaderType(u32);

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

pub struct PartialSectionHeader {
	pub name_offset: u32,
	pub ty: SectionHeaderType,
	pub flags: SectionHeaderFlags,
	pub virtual_address: u64,
	pub offset: u64,
	pub size: u64,
	pub link: u32,
	pub info: u32,
	pub alignment: u64,
	pub entry_size: u64,
}

#[derive(Debug)]
pub struct SectionHeader {
	pub name: String,
	pub ty: SectionHeaderType,
	pub flags: SectionHeaderFlags,
	pub virtual_address: u64,
	pub offset: u64,
	pub size: u64,
	pub link: u32,
	pub info: u32,
	pub alignment: u64,
	pub entry_size: u64,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct ElfType(u16);

impl ElfType {
	pub const ET_NONE: Self = Self(0x00);
	pub const ET_REL: Self = Self(0x01);
	pub const ET_EXEC: Self = Self(0x02);
	pub const ET_DYN: Self = Self(0x03);
	pub const ET_CORE: Self = Self(0x04);
}

#[derive(Debug)]
pub struct ElfFile {
	pub version: u32,
	pub class: Class,
	pub endianness: Endianness,
	pub ty: ElfType,
	pub isa: ISA,
	pub abi: u8,
	pub abi_version: u8,
	pub flags: u32,
	pub entrypoint: u64,
	pub section_name_string_table_index: u16,
	pub header_size: u16,

	pub program_header_table_offset: u64,
	pub program_header_table_entry_count: u16,
	pub program_header_table_entry_size: u16,
	pub program_headers: Vec<ProgramHeader>,

	pub section_header_table_offset: u64,
	pub section_header_table_entry_count: u16,
	pub section_header_table_entry_size: u16,
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
		assert_eq!(ty, ElfType::ET_EXEC);

		let isa = ISA::parse(&mut cursor, endianness)?;
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
		let header_size = cursor.read_16(endianness)?;

		let program_header_table_entry_size = cursor.read_16(endianness)?;
		let program_header_table_entry_count = cursor.read_16(endianness)?;
		let section_header_table_entry_size = cursor.read_16(endianness)?;
		let section_header_table_entry_count = cursor.read_16(endianness)?;
		let section_name_string_table_index = cursor.read_16(endianness)?;

		let mut program_headers: Vec<ProgramHeader> = Vec::new();
		cursor.set_position(program_header_table_offset);

		for _ in 0..program_header_table_entry_count {
			program_headers.push(ProgramHeader {
				ty: ProgramHeaderType(cursor.read_32(endianness)?),
				flags: cursor.read_32(endianness)?,
				offset: cursor.read_64(endianness)?,
				virtual_address: cursor.read_64(endianness)?,
				physical_address: cursor.read_64(endianness)?,
				size_in_file: cursor.read_64(endianness)?,
				size_in_memory: cursor.read_64(endianness)?,
				alignment: cursor.read_64(endianness)?,
			});

			// minimum entry size for elf64
			cursor.seek_relative((program_header_table_entry_size.cast_signed() as i64).saturating_sub(0x38))?;
		}

		let mut section_headers: Vec<PartialSectionHeader> = Vec::new();
		cursor.set_position(section_header_table_offset);

		for _ in 0..section_header_table_entry_count {
			section_headers.push(PartialSectionHeader {
				name_offset: cursor.read_32(endianness)?,
				ty: SectionHeaderType(cursor.read_32(endianness)?),
				flags: SectionHeaderFlags(cursor.read_64(endianness)?),
				virtual_address: cursor.read_64(endianness)?,
				offset: cursor.read_64(endianness)?,
				size: cursor.read_64(endianness)?,
				link: cursor.read_32(endianness)?,
				info: cursor.read_32(endianness)?,
				alignment: cursor.read_64(endianness)?,
				entry_size: cursor.read_64(endianness)?,
			});

			// minimum entry size for elf64
			cursor.seek_relative((section_header_table_entry_size.cast_signed() as i64).saturating_sub(0x40))?;
		}

		let string_table_section_offset = section_headers[section_name_string_table_index as usize].offset;

		Ok(ElfFile {
			version,
			class,
			endianness,
			ty,
			isa,
			abi,
			abi_version,
			flags,
			entrypoint,
			section_name_string_table_index,
			header_size,
			program_header_table_offset,
			program_header_table_entry_count,
			program_header_table_entry_size,
			program_headers,
			section_header_table_offset,
			section_header_table_entry_count,
			section_header_table_entry_size,
			section_headers: section_headers
				.into_iter()
				.map(|header| {
					let name_index = string_table_section_offset + header.name_offset as u64;
					cursor.set_position(name_index);

					let mut name = String::new();
					loop {
						let byte = cursor.read_u8()?;
						if byte == b'\0' {
							break;
						}

						name.push(byte as char); // something something this isnt correct probably
					}

					Ok(SectionHeader {
						name,
						ty: header.ty,
						flags: header.flags,
						virtual_address: header.virtual_address,
						offset: header.offset,
						size: header.size,
						link: header.link,
						info: header.info,
						alignment: header.alignment,
						entry_size: header.entry_size,
					})
				})
				.collect::<Result<Vec<SectionHeader>, std::io::Error>>()?,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_elf_file() {
		let data = include_bytes!("../../../target/out.elf");
		let elf = ElfFile::parse(data).unwrap();
		dbg!(elf);
	}
}
