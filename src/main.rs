mod cpu;
mod csr;
mod gdb;
mod insn;
mod mem;
mod ty;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::fs;
use std::path::PathBuf;

use clap::{command, Parser, Subcommand};
use gdbstub::conn::ConnectionExt;
use gdbstub::stub::GdbStub;

use crate::cpu::{WhiskerCpu, WhiskerExecState};
use crate::gdb::WhiskerEventLoop;
use crate::ty::RegisterIndex;

#[derive(Debug, Parser)]
#[command(version)]
struct CliArgs {
	#[arg(short = 'g', long)]
	gdb: bool,

	#[command(subcommand)]
	command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
	Run {
		#[arg()]
		bootrom: PathBuf,
		#[arg(long, default_value_t = 0)]
		bootrom_offset: u64,
	},
}

fn main() {
	env_logger::init();
	let cli = CliArgs::parse();

	match cli.command {
		Commands::Run {
			bootrom,
			bootrom_offset,
		} => {
			let cpu = init_cpu(bootrom, bootrom_offset);
			if cli.gdb {
				run_gdb(cpu);
			} else {
				run_normal(cpu);
			}
		}
	}
}

fn init_cpu(bootrom: PathBuf, bootrom_offset: u64) -> WhiskerCpu {
	let mut cpu = WhiskerCpu::default();

	let prog = fs::read(&bootrom).unwrap_or_else(|_| panic!("could not read bootrom file {}", bootrom.display()));
	cpu.mem.write_slice(bootrom_offset, prog.as_slice());

	cpu.registers.pc = bootrom_offset;
	cpu.registers.set(RegisterIndex::SP, 0x8000);
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
		cpu.execute_one();
	}
}
