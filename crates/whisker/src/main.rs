#![feature(structural_match)]

mod cpu;
mod csr;
mod gdb;
mod insn;
mod insn16;
mod insn32;
mod mem;
mod ty;
mod util;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::io::Write as _;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, io};

use clap::{command, Parser, Subcommand};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use ty::GPRegisterIndex;

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
		#[arg(long, value_parser = parse_dec_or_hex)]
		bootrom_offset: u64,
	},
}

fn parse_dec_or_hex(s: &str) -> Result<u64, <u64 as FromStr>::Err> {
	if let Some(hex) = s.strip_prefix("0x") {
		u64::from_str_radix(hex, 16)
	} else {
		// try decimal and non-prefixed hex
		u64::from_str_radix(s, 10).or_else(|_| u64::from_str_radix(s, 16))
	}
}

fn main() {
	tracing_subscriber::registry()
		.with(tracing_subscriber::fmt::layer().without_time())
		.with(tracing_subscriber::EnvFilter::from_default_env())
		.init();

	let cli = CliArgs::parse();

	match cli.command {
		Commands::Run {
			use_gdb: gdb,
			bootrom,
			bootrom_offset,
		} => {
			let cpu = init_cpu(bootrom, bootrom_offset);
			if gdb {
				run_gdb(cpu);
			} else {
				run_normal(cpu);
			}
		}
	}
}

fn init_cpu(bootrom: PathBuf, bootrom_offset: u64) -> WhiskerCpu {
	let prog = fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));

	let supported = SupportedExtensions::INTEGER | SupportedExtensions::FLOAT | SupportedExtensions::COMPRESSED;

	let dram_base = 0x8000_0000u64;
	let dram_size = 0x1000_0000u64;
	let uart_addr = 0x1000_0000u64;

	let mem = MemoryBuilder::default()
		.bootrom(prog, PageBase::from_addr(bootrom_offset))
		.physical_size(dram_size)
		.phys_mapping(PageBase::from_addr(dram_base), PageBase::from_addr(0), dram_size)
		// MMIO UART mapping
		.add_mapping(
			PageBase::from_addr(uart_addr),
			PageEntry::MMIO {
				on_read: Box::new(|_| unimplemented!("read from UART")),
				on_write: Box::new(move |addr, val| {
					if addr == uart_addr {
						print!("{}", val as char);
						io::stdout().flush().unwrap();
					}
				}),
			},
		)
		.build();

	let mut cpu = WhiskerCpu::new(supported, mem);

	cpu.pc = bootrom_offset;
	cpu.registers.set(GPRegisterIndex::SP, (dram_base + dram_size) & !0xF);
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
