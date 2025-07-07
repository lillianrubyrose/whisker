mod cpu;
mod gdb;
mod insn;
mod insn16;
mod insn32;
mod mem;
mod regs;
mod soft;
mod ty;
mod util;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::fs;
use std::path::PathBuf;

use clap::{command, Parser, Subcommand};
use elfie::{Class, ElfFile, Endianness, ProgramHeaderType, ISA};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::cpu::{WhiskerCpu, WhiskerExecState};
use crate::gdb::WhiskerEventLoop;
use crate::mem::{MMIOKind, MemoryBuilder, PageBase};
use crate::ty::{HartId, SupportedExtensions};

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
		#[arg(short = 'g', long)]
		use_gdb: bool,
		#[arg()]
		bootrom: PathBuf,
		#[arg()]
		kernel: PathBuf,
	},
}

macro_rules! log {
    ($cpu:ident, $($arg:tt)*) => {
        {
        use tracing::*;
        use std::io::Write as _;
        if let Some(logfile) = $cpu.logfile.as_mut() {
            trace!($($arg)*);
            logfile.write_fmt(format_args!($($arg)*)).expect("failed to write to log");
            writeln!(logfile).expect("failed to write to log");
            logfile.flush().expect("failed to write to log");
        }
        }
    };
}
pub(crate) use log;

fn main() {
	tracing_subscriber::registry()
		.with(tracing_subscriber::fmt::layer().without_time())
		.with(
			tracing_subscriber::EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env_lossy(),
		)
		.init();

	let cli = CliArgs::parse();

	match cli.command {
		Commands::Run {
			use_gdb: gdb,
			bootrom,
			kernel,
			logfile,
		} => {
			let cpu = init_cpu(bootrom, kernel, logfile);
			if gdb {
				run_gdb(cpu);
			} else {
				run_normal(cpu);
			}
		}
	}
}

// THESE MUST BE IN SYNC WITH LINKER SCRIPTS
const BOOTROM_OFFSET: u64 = 0x00001000;
const DRAM_BASE: u64 = 0x8000_0000;
const DRAM_SIZE: u64 = 0x1000_0000;

fn init_cpu(bootrom: PathBuf, kernel: PathBuf, logfile: Option<PathBuf>) -> WhiskerCpu {
	let bootrom_data =
		fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));
	let kernel_data = fs::read(&kernel).unwrap_or_else(|_| panic!("could not read kernel file {}", kernel.display()));

	let elf = ElfFile::parse(&mut std::io::Cursor::new(kernel_data.as_slice()))
		.unwrap_or_else(|err| panic!("could not parse ELF file {} | {err}", kernel.display()));

	if elf.isa != ISA::RiscV {
		panic!("ELF file is not for RISC-V architecture");
	}
	if elf.class != Class::X64 {
		panic!("ELF file is not 64-bit");
	}
	if elf.endianness != Endianness::Little {
		panic!("ELF file is not little-endian");
	}

	let supported = SupportedExtensions::INTEGER
		| SupportedExtensions::FLOAT
		| SupportedExtensions::COMPRESSED
		| SupportedExtensions::ATOMIC
		| SupportedExtensions::MULTIPLY;

	let mem = MemoryBuilder::default()
		.bootrom(bootrom_data, PageBase::from_addr(BOOTROM_OFFSET))
		.physical_size(DRAM_SIZE)
		.phys_mapping(PageBase::from_addr(DRAM_BASE), PageBase::from_addr(0), DRAM_SIZE)
		.add_mmio(MMIOKind::UART)
		.build();

	let mut cpu = WhiskerCpu::new(supported, mem, logfile);

	for program_header in &elf.program_headers {
		if program_header.ty == ProgramHeaderType::PT_LOAD {
			let offset = program_header.offset as usize;
			let file_data = &kernel_data[offset..(offset + program_header.size_in_file as usize)];

			cpu.write_slice(HartId::HART0, program_header.virtual_address, file_data)
				.unwrap_or_else(|addr| panic!("unable to copy ELF segment to memory at address {:#x}", addr));

			println!(
				"Loaded ELF segment: vaddr={:#x}, size={:#x}, file_size={:#x}",
				program_header.virtual_address, program_header.size_in_memory, program_header.size_in_file
			);
		}
	}

	cpu.pc = elf.entrypoint;
	println!("ELF entry point: {:#x}", elf.entrypoint);
	cpu
}

fn run_gdb(mut cpu: WhiskerCpu) {
	let conn: Box<dyn ConnectionExt<Error = std::io::Error>> = Box::new(gdb::wait_for_tcp().expect("listener to bind"));
	let gdb = GdbStub::new(conn);
	match gdb.run_blocking::<WhiskerEventLoop>(&mut cpu) {
		Ok(dc_reason) => match dc_reason {
			gdbstub::stub::DisconnectReason::TargetExited(result) => {
				println!("Target exited: {result}")
			}
			gdbstub::stub::DisconnectReason::TargetTerminated(signal) => {
				println!("Target terminated: {signal:?}");
			}
			gdbstub::stub::DisconnectReason::Disconnect => {
				cpu.exec_state = WhiskerExecState::Running;
				loop {
					// FIXME: handle this better
					#[allow(unused_must_use)]
					cpu.execute_one();
				}
			}
			gdbstub::stub::DisconnectReason::Kill => println!("(GDB) Received kill command"),
		},
		Err(err) => {
			dbg!(&err);
			if err.is_target_error() {
				println!(
					"target encountered a fatal error: {:?}",
					err.into_target_error().unwrap()
				)
			} else if err.is_connection_error() {
				let (err, kind) = err.into_connection_error().unwrap();
				println!("connection error: {kind:?} - {err:?}")
			} else {
				println!("gdbstub encountered a fatal error: {err:?}")
			}
		}
	}
}

fn run_normal(mut cpu: WhiskerCpu) {
	cpu.exec_state = WhiskerExecState::Running;
	loop {
		// FIXME: handle this better
		#[allow(unused_must_use)]
		cpu.execute_one();
	}
}
