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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
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

	pub const PT_LOOS: u32 = 0x60000000;
	pub const PT_HIOS: u32 = 0x6fffffff;
	pub const PT_LOPROC: u32 = 0x70000000;
	pub const PT_HIPROC: u32 = 0x7fffffff;
}

impl ProgramHeaderType {
	pub const fn to_value(self) -> u32 {
		self.0
	}

	pub fn from_value(value: u32) -> Option<Self> {
		match value {
			v if v == Self::PT_NULL.to_value() => Some(Self::PT_NULL),
			v if v == Self::PT_LOAD.to_value() => Some(Self::PT_LOAD),
			v if v == Self::PT_DYNAMIC.to_value() => Some(Self::PT_DYNAMIC),
			v if v == Self::PT_INTERP.to_value() => Some(Self::PT_INTERP),
			v if v == Self::PT_NOTE.to_value() => Some(Self::PT_NOTE),
			v if v == Self::PT_SHLIB.to_value() => Some(Self::PT_SHLIB),
			v if v == Self::PT_PHDR.to_value() => Some(Self::PT_PHDR),
			v if v == Self::PT_TLS.to_value() => Some(Self::PT_TLS),

			v if (Self::PT_LOOS..Self::PT_HIOS).contains(&v) => Some(Self(v)),
			v if (Self::PT_LOPROC..Self::PT_HIPROC).contains(&v) => Some(Self(v)),
			_ => None,
		}
	}
}

impl std::fmt::Debug for ProgramHeaderType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let (name, print_value) = match *self {
			Self::PT_NULL => ("PT_NULL", false),
			Self::PT_LOAD => ("PT_LOAD", false),
			Self::PT_DYNAMIC => ("PT_DYNAMIC", false),
			Self::PT_INTERP => ("PT_INTERP", false),
			Self::PT_NOTE => ("PT_NOTE", false),
			Self::PT_SHLIB => ("PT_SHLIB", false),
			Self::PT_PHDR => ("PT_PHDR", false),
			Self::PT_TLS => ("PT_TLS", false),
			v if (Self::PT_LOOS..=Self::PT_HIOS).contains(&v.to_value()) => ("OS-Specific", true),
			v if (Self::PT_LOPROC..=Self::PT_HIPROC).contains(&v.to_value()) => ("Processor-Specific", true),
			_ => unreachable!(),
		};
		if print_value {
			write!(f, "ProgramHeaderType({}(0x{:08x}))", name, self.0)
		} else {
			write!(f, "ProgramHeaderType({})", name)
		}
	}
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

#[derive(Clone, Copy)]
#[repr(transparent)]
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

impl std::fmt::Debug for SectionHeaderFlags {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut flags = Vec::new();

		if self.contains(Self::SHF_WRITE) {
			flags.push("SHF_WRITE");
		}
		if self.contains(Self::SHF_ALLOC) {
			flags.push("SHF_ALLOC");
		}
		if self.contains(Self::SHF_EXECINSTR) {
			flags.push("SHF_EXECINSTR");
		}
		if self.contains(Self::SHF_MERGE) {
			flags.push("SHF_MERGE");
		}
		if self.contains(Self::SHF_STRINGS) {
			flags.push("SHF_STRINGS");
		}
		if self.contains(Self::SHF_INFO_LINK) {
			flags.push("SHF_INFO_LINK");
		}
		if self.contains(Self::SHF_LINK_ORDER) {
			flags.push("SHF_LINK_ORDER");
		}
		if self.contains(Self::SHF_OS_NONCONFORMING) {
			flags.push("SHF_OS_NONCONFORMING");
		}
		if self.contains(Self::SHF_GROUP) {
			flags.push("SHF_GROUP");
		}
		if self.contains(Self::SHF_TLS) {
			flags.push("SHF_TLS");
		}
		if self.contains(Self::SHF_ORDERED) {
			flags.push("SHF_ORDERED");
		}
		if self.contains(Self::SHF_EXCLUDE) {
			flags.push("SHF_EXCLUDE");
		}

		// all bits in these masks are reserved
		if self.0 & Self::SHF_MASKOS.0 != 0 {
			flags.push("SHF_MASKOS");
		}
		if self.0 & Self::SHF_MASKPROC.0 != 0 {
			flags.push("SHF_MASKPROC");
		}

		if flags.is_empty() {
			write!(f, "SectionHeaderFlags(0x{:1x})", self.0)
		} else {
			write!(f, "SectionHeaderFlags({})", flags.join(" | "))
		}
	}
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
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

	pub const SHT_LOOS: u32 = 0x60000000;
	pub const SHT_HIOS: u32 = 0x6fffffff;
	pub const SHT_LOPROC: u32 = 0x70000000;
	pub const SHT_HIPROC: u32 = 0x7fffffff;
	pub const SHT_LOUSER: u32 = 0x80000000;
	pub const SHT_HIUSER: u32 = 0xffffffff;
}

impl SectionHeaderType {
	pub const fn to_value(self) -> u32 {
		self.0
	}

	pub fn from_value(value: u32) -> Option<Self> {
		match value {
			v if v == Self::SHT_NULL.to_value() => Some(Self::SHT_NULL),
			v if v == Self::SHT_PROGBITS.to_value() => Some(Self::SHT_PROGBITS),
			v if v == Self::SHT_SYMTAB.to_value() => Some(Self::SHT_SYMTAB),
			v if v == Self::SHT_STRTAB.to_value() => Some(Self::SHT_STRTAB),
			v if v == Self::SHT_RELA.to_value() => Some(Self::SHT_RELA),
			v if v == Self::SHT_HASH.to_value() => Some(Self::SHT_HASH),
			v if v == Self::SHT_DYNAMIC.to_value() => Some(Self::SHT_DYNAMIC),
			v if v == Self::SHT_NOTE.to_value() => Some(Self::SHT_NOTE),
			v if v == Self::SHT_NOBITS.to_value() => Some(Self::SHT_NOBITS),
			v if v == Self::SHT_REL.to_value() => Some(Self::SHT_REL),
			v if v == Self::SHT_SHLIB.to_value() => Some(Self::SHT_SHLIB),
			v if v == Self::SHT_DYNSYM.to_value() => Some(Self::SHT_DYNSYM),
			v if v == Self::SHT_INIT_ARRAY.to_value() => Some(Self::SHT_INIT_ARRAY),
			v if v == Self::SHT_FINI_ARRAY.to_value() => Some(Self::SHT_FINI_ARRAY),
			v if v == Self::SHT_PREINIT_ARRAY.to_value() => Some(Self::SHT_PREINIT_ARRAY),
			v if v == Self::SHT_GROUP.to_value() => Some(Self::SHT_GROUP),
			v if v == Self::SHT_SYMTAB_SHNDX.to_value() => Some(Self::SHT_SYMTAB_SHNDX),
			v if v == Self::SHT_NUM.to_value() => Some(Self::SHT_NUM),

			v if (Self::SHT_LOOS..Self::SHT_HIOS).contains(&v) => Some(Self(v)),
			_ => None,
		}
	}
}

impl std::fmt::Debug for SectionHeaderType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let (name, print_value) = match *self {
			Self::SHT_NULL => ("SHT_NULL", false),
			Self::SHT_PROGBITS => ("SHT_PROGBITS", false),
			Self::SHT_SYMTAB => ("SHT_SYMTAB", false),
			Self::SHT_STRTAB => ("SHT_STRTAB", false),
			Self::SHT_RELA => ("SHT_RELA", false),
			Self::SHT_HASH => ("SHT_HASH", false),
			Self::SHT_DYNAMIC => ("SHT_DYNAMIC", false),
			Self::SHT_NOTE => ("SHT_NOTE", false),
			Self::SHT_NOBITS => ("SHT_NOBITS", false),
			Self::SHT_REL => ("SHT_REL", false),
			Self::SHT_SHLIB => ("SHT_SHLIB", false),
			Self::SHT_DYNSYM => ("SHT_DYNSYM", false),
			Self::SHT_INIT_ARRAY => ("SHT_INIT_ARRAY", false),
			Self::SHT_FINI_ARRAY => ("SHT_FINI_ARRAY", false),
			Self::SHT_PREINIT_ARRAY => ("SHT_PREINIT_ARRAY", false),
			Self::SHT_GROUP => ("SHT_GROUP", false),
			Self::SHT_SYMTAB_SHNDX => ("SHT_SYMTAB_SHNDX", false),
			Self::SHT_NUM => ("SHT_NUM", false),
			v if (SectionHeaderType::SHT_LOOS..=SectionHeaderType::SHT_HIOS).contains(&v.to_value()) => {
				("OS-Specific", true)
			}
			v if (SectionHeaderType::SHT_LOPROC..=SectionHeaderType::SHT_HIPROC).contains(&v.to_value()) => {
				("Processor-Specific", true)
			}
			v if (SectionHeaderType::SHT_LOUSER..=SectionHeaderType::SHT_HIUSER).contains(&v.to_value()) => {
				("User-Specific", true)
			}
			_ => unreachable!(),
		};
		if print_value {
			write!(f, "SectionHeaderType({}(0x{:08x}))", name, self.0)
		} else {
			write!(f, "SectionHeaderType({})", name)
		}
	}
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum ElfType {
	None,
	Relative,
	Executable,
	Dynamic,
	Core,
}

impl ElfType {
	pub fn to_value(self) -> u16 {
		match self {
			Self::None => 0x00,
			Self::Relative => 0x01,
			Self::Executable => 0x02,
			Self::Dynamic => 0x03,
			Self::Core => 0x04,
		}
	}

	pub fn from_value(value: u16) -> Option<Self> {
		match value {
			0x00 => Some(Self::None),
			0x01 => Some(Self::Relative),
			0x02 => Some(Self::Executable),
			0x03 => Some(Self::Dynamic),
			0x04 => Some(Self::Core),
			_ => None,
		}
	}
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
	pub fn parse(mut cursor: Cursor<&[u8]>) -> Result<Self, std::io::Error> {
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

		let Some(ty) = ElfType::from_value(cursor.read_16(endianness)?) else {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				"Invalid ELF file type",
			));
		};
		assert_eq!(ty, ElfType::Executable);

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

	pub fn section(&self, name: &str) -> Option<&SectionHeader> {
		for header in &self.section_headers {
			if header.name == name {
				return Some(header);
			}
		}
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_elf_file() {
		let data = include_bytes!("../../../target/out.elf");
		let elf = ElfFile::parse(Cursor::new(data)).unwrap();

		dbg!(elf);
	}
}
