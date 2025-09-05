#![feature(assert_matches)]
#![feature(cold_path)]

mod cpu;
mod gdb;
mod insn;
mod insn16;
mod insn32;
mod interrupts;
mod mem;
mod regs;
mod soft;
mod ty;
mod util;
mod virtio;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fs, panic};

use ::tracing::level_filters::LevelFilter;
use clap::{command, Parser, Subcommand};
use elfie::{Class, ElfFile, Endianness, ProgramHeaderType, ISA};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use spin::Mutex;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::cpu::{csr, WhiskerCpu, WhiskerExecState};
use crate::gdb::WhiskerEventLoop;
use crate::interrupts::{PLIC_BASE, PLIC_LEN};
use crate::mem::mmio::virtio_block::VIRTIO_BLOCK_BASE;
use crate::mem::mmio::{MMIOKind, UART_BASE};
use crate::mem::{AccessAttrs, AccessKind, MemoryBuilder, MemoryRegion};
use crate::ty::{FPRegisterIndex, GPRegisterIndex, RiscvExtensions};

#[derive(Debug, Parser)]
#[command(version)]
struct CliArgs {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
	Run {
		#[arg(long)]
		logfile: Option<PathBuf>,
		#[arg(long)]
		fs_img: Option<PathBuf>,
		#[arg(short = 'g', long)]
		use_gdb: bool,
		#[arg(long)]
		/// set to true to just load the passed file into memory at DRAM_BASE
		raw_kernel: bool,
		#[arg()]
		bootrom: PathBuf,
		#[arg()]
		kernel: PathBuf,
	},
	GenerateGdbXML,
}

#[macro_export]
macro_rules! trace {
    ($fmt:expr $(, $args:expr)*$(,)* ) => {
        if cfg!(feature = "tracing") {
            ::tracing::trace!($fmt $(, $args)*);
        }
    }
}

#[macro_export]
macro_rules! debug {
    ($fmt:expr $(, $args:expr)*$(,)* ) => {
        if cfg!(feature = "tracing") {
            ::tracing::debug!($fmt $(, $args)*);
        }
    }
}

#[macro_export]
macro_rules! info {
    ($fmt:expr $(, $args:expr)*$(,)* ) => {
        if cfg!(feature = "tracing") {
            ::tracing::info!($fmt $(, $args)*);
        }
    }
}

#[macro_export]
macro_rules! warn {
    ($fmt:expr $(, $args:expr)*$(,)* ) => {
        if cfg!(feature = "tracing") {
            ::tracing::warn!($fmt $(, $args)*);
        }
    }
}

#[macro_export]
macro_rules! error {
    ($fmt:expr $(, $args:expr)*$(,)* ) => {
        if cfg!(feature = "tracing") {
            ::tracing::error!($fmt $(, $args)*);
        }
    }
}

pub mod tracing {
	pub use crate::debug;
	pub use crate::error;
	pub use crate::info;
	pub use crate::trace;
	pub use crate::warn;
}

fn main() {
	if cfg!(feature = "tracing") {
		tracing_subscriber::registry()
			.with(tracing_subscriber::fmt::layer().without_time())
			.with(
				tracing_subscriber::EnvFilter::builder()
					.with_default_directive(LevelFilter::INFO.into())
					.from_env_lossy(),
			)
			.init();
	}

	let cli = CliArgs::parse();

	match cli.command {
		Commands::Run {
			use_gdb: gdb,
			bootrom,
			kernel,
			raw_kernel,
			logfile,
			fs_img,
		} => {
			// FIXME: get this from cli or something
			const NUM_HARTS: u16 = 1;
			let cpu = init_cpu(bootrom, kernel, raw_kernel, logfile, NUM_HARTS, fs_img.as_deref());
			if gdb {
				run_gdb(cpu);
			} else {
				run_normal(cpu);
			}
		}
		Commands::GenerateGdbXML => {
			let xml = generate_gdb_xml();
			fs::write("rv64.xml", xml.as_bytes()).expect("failed to write rv64.xml");
		}
	}
}

// THESE MUST BE IN SYNC WITH LINKER SCRIPTS
const BOOTROM_OFFSET: u64 = 0x00001000;
const DRAM_BASE: u64 = 0x8000_0000;
const DRAM_SIZE: u64 = 0x1000_0000;

fn init_cpu(
	bootrom: PathBuf,
	kernel: PathBuf,
	raw_kernel: bool,
	logfile: Option<PathBuf>,
	num_harts: u16,
	fs_img: Option<&Path>,
) -> WhiskerCpu {
	let mut bootrom_data =
		fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));
	bootrom_data.resize(0x1000, 0);

	let kernel_data = fs::read(&kernel).unwrap_or_else(|_| panic!("could not read kernel file {}", kernel.display()));

	let supported = RiscvExtensions::INTEGER
		| RiscvExtensions::FLOAT
		| RiscvExtensions::COMPRESSED
		| RiscvExtensions::ATOMIC
		| RiscvExtensions::MULTIPLY;

	//	let mem = MemoryBuilder::default()
	//		.bootrom(bootrom_data, PageBase::from_addr(BOOTROM_OFFSET))
	//		.physical_size(DRAM_SIZE)
	//		.phys_mapping(PageBase::from_addr(DRAM_BASE), PageBase::from_addr(0), DRAM_SIZE)
	//		.add_mmio(MMIOKind::UART)
	//		.build();

	const ACCESS_MAX_U64: u8 = core::mem::size_of::<u64>() as u8;

	let mut mem_builder = MemoryBuilder::default()
		// FIXME: maybe model the bootrom as an IO region so it can be RX instead of RWX
		.add_region(MemoryRegion::new_main_mem(
			BOOTROM_OFFSET,
			0x1000,
			bootrom_data.into_boxed_slice(),
			AccessAttrs::new(ACCESS_MAX_U64, AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC),
		))
		.add_region(MemoryRegion::new_mmio(
			UART_BASE,
			0x1000,
			MMIOKind::UART,
			AccessAttrs::new(1, AccessKind::READ | AccessKind::WRITE),
		))
		.add_region(MemoryRegion::new_mmio(
			PLIC_BASE,
			PLIC_LEN,
			MMIOKind::PLIC,
			AccessAttrs::new(4, AccessKind::READ | AccessKind::WRITE),
		))
		.add_region(MemoryRegion::new_mmio(
			VIRTIO_BLOCK_BASE,
			0x1000,
			MMIOKind::VirtioBlock,
			AccessAttrs::new(4, AccessKind::READ | AccessKind::WRITE),
		));

	let mut main_mem = vec![0_u8; DRAM_SIZE as usize].into_boxed_slice();

	if raw_kernel {
		main_mem[..kernel_data.len()].copy_from_slice(kernel_data.as_slice());
	} else {
		load_elf(kernel.as_path(), kernel_data.as_slice(), &mut main_mem);
	}

	mem_builder = mem_builder.add_region(MemoryRegion::new_main_mem(
		DRAM_BASE,
		DRAM_SIZE,
		main_mem,
		AccessAttrs::new(
			ACCESS_MAX_U64,
			AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC | AccessKind::ATOMIC,
		),
	));
	cpu::MEMORY.get_or_init(|| Arc::new(mem_builder.build()));

	WhiskerCpu::new(supported, logfile, num_harts, BOOTROM_OFFSET, fs_img)
}

fn load_elf(kernel_path: &Path, kernel_data: &[u8], main_mem: &mut Box<[u8]>) {
	let elf = ElfFile::parse(Cursor::new(&kernel_data))
		.unwrap_or_else(|err| panic!("could not parse ELF file {} | {err}", kernel_path.display()));

	if elf.isa != ISA::RiscV {
		panic!("ELF file is not for RISC-V architecture");
	}
	if elf.class != Class::X64 {
		panic!("ELF file is not 64-bit");
	}
	if elf.endianness != Endianness::Little {
		panic!("ELF file is not little-endian");
	}

	for program_header in &elf.program_headers {
		if program_header.ty == ProgramHeaderType::PT_LOAD {
			let file_offset = program_header.offset as usize;
			let mem_offset = (program_header.physical_address - DRAM_BASE) as usize;
			let len = program_header.size_in_file as usize;

			let file_data = &kernel_data[file_offset..(file_offset + program_header.size_in_file as usize)];

			main_mem[mem_offset..][..len].copy_from_slice(file_data);

			info!(
				"Loaded ELF segment: paddr={:#x}, size={:#x}, file_size={:#x}",
				program_header.physical_address, program_header.size_in_memory, program_header.size_in_file
			);
		}
	}
}

fn run_gdb(mut cpu: WhiskerCpu) {
	let conn: Box<dyn ConnectionExt<Error = std::io::Error>> = Box::new(gdb::wait_for_tcp().expect("listener to bind"));
	let gdb = GdbStub::new(conn);
	match gdb.run_blocking::<WhiskerEventLoop>(&mut cpu) {
		Ok(dc_reason) => match dc_reason {
			gdbstub::stub::DisconnectReason::TargetExited(result) => {
				error!("Target exited: {result}")
			}
			gdbstub::stub::DisconnectReason::TargetTerminated(signal) => {
				error!("Target terminated: {signal:?}");
			}
			gdbstub::stub::DisconnectReason::Disconnect => {
				cpu.hart_states.fill(WhiskerExecState::Running);
				loop {
					// FIXME: handle this better
					#[allow(unused_must_use)]
					cpu.execute_one();
				}
			}
			gdbstub::stub::DisconnectReason::Kill => info!("(GDB) Received kill command"),
		},
		Err(err) => {
			dbg!(&err);
			if err.is_target_error() {
				error!("target encountered a fatal error: {}", err);
			} else if err.is_connection_error() {
				let (err, kind) = err.into_connection_error().unwrap();
				error!("connection error: {kind:?} - {err}");
			} else {
				error!("gdbstub encountered a fatal error: {err}");
			}
		}
	}
}

fn run_normal(mut cpu: WhiskerCpu) {
	cpu.hart_states.fill(WhiskerExecState::Running);
	loop {
		// FIXME: handle this better
		#[allow(unused_must_use)]
		cpu.execute_one();
	}
}

fn generate_gdb_xml() -> String {
	let mut xml = String::from(
		r#"<?xml version="1.0"?>
<!-- Copyright (C) 2018-2024 Free Software Foundation, Inc.

     Copying and distribution of this file, with or without modification,
     are permitted in any medium without royalty provided the copyright
     notice and this notice are preserved.  -->

<!-- Register numbers are hard-coded in order to maintain backward
     compatibility with older versions of tools that didn't use xml
     register descriptions.  -->

<!DOCTYPE feature SYSTEM "gdb-target.dtd">
<target>
"#,
	);

	// GPRs
	xml.push_str("  <feature name=\"org.gnu.gdb.riscv.cpu\">\n");
	for reg in GPRegisterIndex::ALL_REGS {
		writeln!(
			&mut xml,
			"    <reg name=\"{}\" bitsize=\"64\" type=\"{}\" regnum=\"{}\"/>",
			reg.display(),
			reg.abi_kind(),
			reg.as_u8()
		)
		.unwrap();
	}
	xml.push_str("    <reg name=\"pc\" bitsize=\"64\" type=\"code_ptr\" regnum=\"32\"/>\n");
	xml.push_str("  </feature>\n");

	// FPRs
	xml.push_str("  <feature name=\"org.gnu.gdb.riscv.fpu\">\n");
	for reg in FPRegisterIndex::ALL_REGS {
		writeln!(
			&mut xml,
			"    <reg name=\"{}\" bitsize=\"64\" type=\"ieee_double\" regnum=\"{}\"/>",
			reg.display(),
			33 + reg.as_u8()
		)
		.unwrap();
	}
	xml.push_str("  </feature>\n");

	xml.push_str(&csr::generate_csr_xml(&csr::create_info()));

	xml.push_str("</target>\n");

	xml
}
