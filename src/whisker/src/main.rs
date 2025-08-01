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

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs, panic};

use clap::{command, Parser, Subcommand};
use elfie::{Class, ElfFile, Endianness, ProgramHeaderType, ISA};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::cpu::interrupts::InterruptController;
use crate::cpu::{WhiskerCpu, WhiskerExecState};
use crate::gdb::WhiskerEventLoop;
use crate::mem::mmio::{MMIOKind, UART_BASE};
use crate::mem::{AccessAttrs, AccessKind, MemoryBuilder, MemoryRegion};
use crate::ty::SupportedExtensions;

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
	let mut bootrom_data =
		fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));
	bootrom_data.resize(0x1000, 0);

	let kernel_data = fs::read(&kernel).unwrap_or_else(|_| panic!("could not read kernel file {}", kernel.display()));

	let elf = ElfFile::parse(Cursor::new(&kernel_data))
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
		));

	// load the ELF into main memory
	let mut main_mem_backing = vec![0_u8; DRAM_SIZE as usize].into_boxed_slice();
	for program_header in &elf.program_headers {
		if program_header.ty == ProgramHeaderType::PT_LOAD {
			let file_offset = program_header.offset as usize;
			let mem_offset = (program_header.physical_address - DRAM_BASE) as usize;
			let len = program_header.size_in_file as usize;

			let file_data = &kernel_data[file_offset..(file_offset + program_header.size_in_file as usize)];

			main_mem_backing[mem_offset..][..len].copy_from_slice(file_data);

			info!(
				"Loaded ELF segment: paddr={:#x}, size={:#x}, file_size={:#x}",
				program_header.physical_address, program_header.size_in_memory, program_header.size_in_file
			);
		}
	}

	mem_builder = mem_builder.add_region(MemoryRegion::new_main_mem(
		DRAM_BASE,
		DRAM_SIZE,
		main_mem_backing,
		AccessAttrs::new(
			ACCESS_MAX_U64,
			AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC | AccessKind::ATOMIC,
		),
	));
	cpu::MEMORY.get_or_init(|| Mutex::new(mem_builder.build()));

	let (int_tx, interrupt_controller) = InterruptController::new();

	mem::mmio::register_mmio(MMIOKind::UART, mem::mmio::UART::init(int_tx.clone()) as Arc<Mutex<_>>).unwrap();

	let mut cpu = WhiskerCpu::new(supported, interrupt_controller, logfile);

	cpu.pc = BOOTROM_OFFSET;
	cpu
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
				cpu.exec_state = WhiskerExecState::Running;
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
				error!(
					"target encountered a fatal error: {:?}",
					err.into_target_error().unwrap()
				)
			} else if err.is_connection_error() {
				let (err, kind) = err.into_connection_error().unwrap();
				error!("connection error: {kind:?} - {err:?}")
			} else {
				error!("gdbstub encountered a fatal error: {err:?}")
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
