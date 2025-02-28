mod cpu;
mod csr;
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

use std::io::Write as _;
use std::path::PathBuf;
use std::{fs, io};

use clap::{command, Parser, Subcommand};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

use crate::cpu::{WhiskerCpu, WhiskerExecState};
use crate::gdb::WhiskerEventLoop;
use crate::mem::{MemoryBuilder, PageBase, PageEntry};
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
		#[arg(short = 'g', long)]
		use_gdb: bool,
		#[arg()]
		bootrom: PathBuf,
		#[arg()]
		kernel: PathBuf,
	},
}

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
		} => {
			let cpu = init_cpu(bootrom, kernel);
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
const UART_ADDR: u64 = 0x1000_0000;

fn init_cpu(bootrom: PathBuf, kernel: PathBuf) -> WhiskerCpu {
	let bootrom = fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));
	let kernel = fs::read(&kernel).unwrap_or_else(|_| panic!("could not read kernel file {}", kernel.display()));

	let supported = SupportedExtensions::INTEGER
		| SupportedExtensions::FLOAT
		| SupportedExtensions::COMPRESSED
		| SupportedExtensions::ATOMIC
		| SupportedExtensions::MULTIPLY;

	let mut mem = MemoryBuilder::default()
		.bootrom(bootrom, PageBase::from_addr(BOOTROM_OFFSET))
		.physical_size(DRAM_BASE)
		.phys_mapping(PageBase::from_addr(DRAM_BASE), PageBase::from_addr(0), DRAM_SIZE)
		// MMIO UART mapping
		.add_mapping(
			PageBase::from_addr(UART_ADDR),
			PageEntry::MMIO {
				on_read: Box::new(|_| unimplemented!("read from UART")),
				on_write: Box::new(move |addr, val| {
					if addr == UART_ADDR {
						print!("{}", val as char);
						io::stdout().flush().unwrap();
					}
				}),
			},
		)
		.build();

	mem.write_slice(DRAM_BASE, kernel.as_slice())
		.expect("unable to copy kernel to memory");

	let mut cpu = WhiskerCpu::new(supported, mem);

	cpu.pc = BOOTROM_OFFSET;
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
