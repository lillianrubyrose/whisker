// library features
#![feature(assert_matches, duration_millis_float, debug_closure_helpers, cold_path)]

mod args;
mod cpu;
mod gdb;
mod insn;
mod insn16;
mod insn32;
mod interrupts;
mod mem;
mod regs;
mod riscv_tests;
mod soft;
mod ty;
mod util;
mod virtio;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("whisker only supports 64bit architectures");

use std::{
	fmt::Write as _,
	fs,
	io::{Cursor, Write, stdout},
	panic,
	path::{Path, PathBuf},
	sync::Arc,
	time::Instant,
};

use ::tracing::level_filters::LevelFilter;
use elfie::{ElfFile, ElfType, Endianness, ISA, ProgramHeaderType};
use gdbstub::{conn::ConnectionExt, stub::GdbStub};
use spin::Mutex;
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::{
	args::{CliCommand, KernelData},
	cpu::{WhiskerCpu, WhiskerExecState, WhiskerExecStatus, csr},
	gdb::WhiskerEventLoop,
	interrupts::{PLIC_BASE, PLIC_LEN, PlatformInterruptController},
	mem::{
		AccessAttrs, AccessKind, MemoryBuilder, MemoryRegion,
		mmio::{
			MMIOKind, UART, UART_BASE,
			clint::{CLINT_BASE, CLINT_SIZE, Clint},
			goldfish_rtc::{GOLDFISH_RTC_BASE, GOLDFISH_RTC_SIZE, GoldfishRTC},
			shutdown::ShutdownDevice,
			virtio_block::{VIRTIO_BLOCK_BASE, VirtioBlockDevice},
		},
	},
	ty::{FPRegisterIndex, GPRegisterIndex, RiscvExtensions},
};

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
	pub use crate::{debug, error, info, trace, warn};
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

	let command = args::get_command();
	match command {
		CliCommand::Run {
			bootrom,
			bootloader,
			kernel_data,
			fs_img,
			logfile,
			use_gdb,
		} => {
			// FIXME: get this from cli or something
			const NUM_HARTS: u16 = 1;
			let cpu = init_cpu(bootrom, bootloader, kernel_data, logfile, NUM_HARTS, fs_img.as_deref());
			if use_gdb {
				run_gdb(cpu);
			} else {
				run_normal(cpu);
			}
		}
		CliCommand::GenerateGdbXML => {
			let xml = generate_gdb_xml();
			fs::write("rv64.xml", xml.as_bytes()).expect("failed to write rv64.xml");
		}
	}
}

// THESE MUST BE IN SYNC WITH LINKER SCRIPTS
const BOOTROM_OFFSET: u64 = 0x0000_1000;
const BOOTROM_LEN: u64 = 0x0000_1000;
const DRAM_BASE: u64 = 0x8000_0000;
const DRAM_SIZE: u64 = 0x1_0000_0000;

#[allow(clippy::too_many_arguments)]
fn init_cpu(
	mut bootrom: Vec<u8>,
	bootloader: Option<Vec<u8>>,
	kernel: Option<KernelData>,
	logfile: Option<PathBuf>,
	num_harts: u16,
	fs_img: Option<&Path>,
) -> WhiskerCpu {
	const ACCESS_MAX_U64: u8 = core::mem::size_of::<u64>() as u8;

	assert!(
		bootrom.len() <= BOOTROM_LEN as usize,
		"bootrom must not be more than {:#X} bytes (was {:#X})",
		BOOTROM_LEN,
		bootrom.len()
	);
	bootrom.resize(BOOTROM_LEN as usize, 0);

	// we support RV64GC which is IMAFDC_Zicsr_Zifencei
	// on top of that, S mode and U mode are supported
	let supported = RiscvExtensions::INTEGER
		| RiscvExtensions::MULTIPLY
		| RiscvExtensions::ATOMIC
		| RiscvExtensions::FLOAT
		| RiscvExtensions::DOUBLE
		| RiscvExtensions::COMPRESSED
		| RiscvExtensions::SUPERVISOR
		| RiscvExtensions::USER_MODE;

	let (interrupt_tx, interrupt_controller) = PlatformInterruptController::new(num_harts);
	let clint = Arc::new(Mutex::new(Clint::new()));
	let goldfish_rtc = Arc::new(Mutex::new(GoldfishRTC::new(interrupt_tx.clone())));
	let uart = UART::init(interrupt_tx.clone());
	let shutdown = Arc::new(Mutex::new(ShutdownDevice));

	let mem_builder = MemoryBuilder::default()
		// FIXME: maybe model the bootrom as an IO region so it can be RX instead of RWX
		.add_region(MemoryRegion::new_main_mem(
			BOOTROM_OFFSET,
			BOOTROM_LEN,
			bootrom.into_boxed_slice(),
			AccessAttrs::new(ACCESS_MAX_U64, AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC),
		))
		.add_region(MemoryRegion::new_mmio(
			UART_BASE,
			0x1000,
			MMIOKind::UART,
			uart,
			AccessAttrs::new(1, AccessKind::READ | AccessKind::WRITE),
		))
		.add_region(MemoryRegion::new_mmio(
			PLIC_BASE,
			PLIC_LEN,
			MMIOKind::PLIC,
			interrupt_controller.clone(),
			AccessAttrs::new(4, AccessKind::READ | AccessKind::WRITE),
		))
		// FIXME: hook this up to actual clock and timers
		.add_region(MemoryRegion::new_mmio(
			CLINT_BASE,
			CLINT_SIZE,
			MMIOKind::Clint,
			clint.clone(),
			AccessAttrs::new(8, AccessKind::READ | AccessKind::WRITE),
		))
		.add_region(MemoryRegion::new_mmio(
			0x100000,
			0x1000,
			MMIOKind::Shutdown,
			shutdown,
			AccessAttrs::new(8, AccessKind::READ | AccessKind::WRITE),
		))
		.add_region(MemoryRegion::new_mmio(
			GOLDFISH_RTC_BASE,
			GOLDFISH_RTC_SIZE,
			MMIOKind::GoldfishRTC,
			goldfish_rtc.clone(),
			AccessAttrs::new(4, AccessKind::READ | AccessKind::WRITE),
		));

	let mut main_mem = vec![0_u8; DRAM_SIZE as usize].into_boxed_slice();

	let mut tohost_addr = None;

	if let Some(bootloader) = bootloader {
		#[allow(clippy::collapsible_if, reason = "makes the side effects of load_elf more apparent")]
		if let Some(addr) = load_elf(bootloader.as_slice(), &mut main_mem, "bootloader") {
			let old_tohost_addr = tohost_addr.replace(addr);
			assert!(old_tohost_addr.is_none(), "tohost addr already present");
		}
	}

	match kernel {
		Some(KernelData::Raw(raw, offset)) => {
			let offset = offset.map_or(0, |off| {
				assert!(off >= DRAM_BASE, "raw kernel offset must be within main memory");
				off - DRAM_BASE
			}) as usize;
			main_mem[offset..][..raw.len()].copy_from_slice(raw.as_slice());
		}
		Some(KernelData::Elf(elf)) => {
			if let Some(addr) = load_elf(elf.as_slice(), &mut main_mem, "kernel") {
				let old_tohost_addr = tohost_addr.replace(addr);
				assert!(old_tohost_addr.is_none(), "tohost addr already present");
			}
		}
		None => {}
	}

	let dtb = include_bytes!("../../../assets/whisker.dtb");
	// let dtb = fs::read("assets/whisker.dtb").unwrap();
	assert!(dtb.len() > 0, "potentially corrupt dtb");
	let dtb_ptr = 0xF000_0000;
	main_mem[(dtb_ptr - DRAM_BASE as usize)..][..dtb.len()].copy_from_slice(dtb.as_slice());

	let memory = mem_builder
		.add_region(MemoryRegion::new_main_mem(
			DRAM_BASE,
			DRAM_SIZE,
			main_mem,
			AccessAttrs::new(
				ACCESS_MAX_U64,
				AccessKind::READ | AccessKind::WRITE | AccessKind::EXEC | AccessKind::ATOMIC,
			),
		))
		.build();

	let memory = Arc::new(memory);

	if let Some(fs_img) = fs_img {
		let virtio_block = VirtioBlockDevice::init(memory.clone(), fs_img, interrupt_tx);
		memory.add_region(MemoryRegion::new_mmio(
			VIRTIO_BLOCK_BASE,
			0x1000,
			MMIOKind::VirtioBlock,
			virtio_block,
			AccessAttrs::new(4, AccessKind::READ | AccessKind::WRITE),
		));
	}

	let mut cpu = WhiskerCpu::new(
		supported,
		logfile,
		num_harts,
		BOOTROM_OFFSET,
		memory,
		interrupt_controller,
		clint,
		goldfish_rtc,
	);
	cpu.tohost_addr = tohost_addr;
	for (hart_id, hart) in cpu.harts.iter_mut().enumerate() {
		hart.registers.set(GPRegisterIndex::new(10).unwrap(), hart_id as u64);
		hart.registers.set(GPRegisterIndex::new(11).unwrap(), dtb_ptr as u64);
	}
	cpu
}

fn load_elf(data: &[u8], main_mem: &mut Box<[u8]>, kind: &'static str) -> Option<u64> {
	let elf =
		ElfFile::parse(Cursor::new(&data)).unwrap_or_else(|err| panic!("could not parse {} ELF file: {}", kind, err));

	assert_eq!(elf.isa, ISA::RiscV, "Only RISC-V architecture ELF files are supported");
	// assert_eq!(elf.class, Class::X64, "Only 64-bit ELF files are supported");
	assert_eq!(
		elf.endianness,
		Endianness::Little,
		"Only little-endian ELF files are supported"
	);

	// base for PIE ELF
	let pie_base = DRAM_BASE;

	for program_header in &elf.program_headers {
		if program_header.ty == ProgramHeaderType::PT_LOAD {
			let file_offset = program_header.offset as usize;

			// FIXME: hack to get PIE working, but this is probably wrong
			let mut phys_addr = program_header.physical_address;
			if elf.ty == ElfType::Dynamic {
				phys_addr += pie_base;
			}

			let mem_offset = (phys_addr - DRAM_BASE) as usize;
			let len = program_header.size_in_file as usize;

			let file_data = &data[file_offset..(file_offset + program_header.size_in_file as usize)];

			main_mem[mem_offset..][..len].copy_from_slice(file_data);

			info!(
				"Loaded ELF segment: paddr={:#x}, size={:#x}, file_size={:#x}",
				phys_addr, program_header.size_in_memory, program_header.size_in_file
			);
		}
	}

	elf.section(".tohost").map(|section| section.virtual_address)
}

fn run_gdb(mut cpu: WhiskerCpu) {
	let conn: Box<dyn ConnectionExt<Error = std::io::Error>> = Box::new(gdb::wait_for_tcp().expect("listener to bind"));
	let gdb = GdbStub::new(conn);
	match gdb.run_blocking::<WhiskerEventLoop>(&mut cpu) {
		Ok(dc_reason) => match dc_reason {
			gdbstub::stub::DisconnectReason::TargetExited(result) => {
				error!("Target exited: {result}");
			}
			gdbstub::stub::DisconnectReason::TargetTerminated(signal) => {
				error!("Target terminated: {signal:?}");
			}
			gdbstub::stub::DisconnectReason::Disconnect => {
				cpu.harts
					.iter_mut()
					.for_each(|hart| hart.exec_state = WhiskerExecState::Running);
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
	cpu.harts
		.iter_mut()
		.for_each(|hart| hart.exec_state = WhiskerExecState::Running);

	let t = Instant::now();

	loop {
		// FIXME: handle this better
		#[allow(unused_must_use)]
		if let Err(WhiskerExecStatus::MMIOShutdown) = cpu.execute_one() {
			println!("shutdown after {:?}", t.elapsed());
			break;
		}

		if let Some(cmd) = cpu.check_tohost() {
			// FIXME: This currently panics in debug mode due to the bitfield checks causing shl overflow
			if let Some(chr) = cmd.get_print_char() {
				// TODO: We should handle this like we do UART probably
				print!("{}", chr);
				stdout().flush().unwrap();
			} else if let Some(passed) = cmd.passed_test() {
				assert!(passed, "{}", cpu.harts[0].dump());
				break;
			}
		}
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

#[cfg(test)]
mod tests {
	use std::{
		fs,
		io::{Write, stdout},
		path::PathBuf,
	};

	use crate::{
		cpu::{WhiskerCpu, WhiskerExecState},
		init_cpu,
	};

	fn run_test(cpu: &mut WhiskerCpu, test_name: &str) -> Result<(), String> {
		cpu.harts
			.iter_mut()
			.for_each(|hart| hart.exec_state = WhiskerExecState::Running);
		loop {
			#[allow(unused_must_use)]
			cpu.execute_one();

			if let Some(cmd) = cpu.check_tohost() {
				if let Some(chr) = cmd.get_print_char() {
					print!("{}", chr);
					stdout().flush().unwrap();
				} else if let Some(passed) = cmd.passed_test() {
					return if passed {
						println!("[PASS] {test_name}");
						Ok(())
					} else {
						println!("[FAIL] {test_name}");
						let dump = cpu
							.harts
							.iter_mut()
							.map(|v| v.dump())
							.fold(String::new(), |acc, v| format!("{acc}\n{v}"));
						Err(format!("{test_name}\n{dump}"))
					};
				}
			}
		}
	}

	fn run_isa_tests(prefix: &str, excludes: &[&str], fail_dumps: &mut Vec<String>, successes: &mut i32) {
		let bootrom = fs::read(
			PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
				.join("target")
				.join("boot.bin"),
		)
		.expect("unable to read bootrom at target/boot.bin");
		let isa_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"))
			.join("riscv-tests")
			.join("isa");
		let dir = std::fs::read_dir(isa_dir).expect("Failed to read riscv-tests/isa directory");

		for ele in dir {
			let ele = ele.unwrap();
			if ele.file_type().unwrap().is_dir() {
				continue;
			}

			let name = ele.file_name();
			let name_str = name.to_string_lossy();
			if name_str.contains('.')
				|| !name_str.starts_with(prefix)
				|| excludes.iter().any(|exclude| name_str.contains(exclude))
			{
				continue;
			}

			let bootloader_path = ele.path();
			let bootloader = fs::read(&bootloader_path)
				.unwrap_or_else(|e| panic!("unable to read test {}: {}", bootloader_path.display(), e));

			let mut cpu = init_cpu(bootrom.clone(), Some(bootloader), None, None, 1, None);
			assert!(
				cpu.tohost_addr.is_some(),
				"tohost addr was not set for test {}",
				name_str
			);

			if let Err(msg) = run_test(&mut cpu, &name_str) {
				fail_dumps.push(msg);
			} else {
				*successes += 1;
			}
		}
	}

	#[test]
	fn isa_tests() {
		let mut failures = Vec::new();
		let mut successes = 0;

		run_isa_tests("rv32ua-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64ua-p", &[], &mut failures, &mut successes);

		println!();

		run_isa_tests("rv32uc-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64uc-p", &[], &mut failures, &mut successes);

		println!();

		run_isa_tests("rv32ui-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv32si-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv32mi-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64ui-p", &["ma_data"], &mut failures, &mut successes);
		run_isa_tests("rv64si-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64mi-p", &["illegal"], &mut failures, &mut successes);

		println!();

		run_isa_tests("rv32um-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64um-p", &[], &mut failures, &mut successes);

		println!();

		run_isa_tests("rv32uf-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64uf-p", &[], &mut failures, &mut successes);

		run_isa_tests("rv32ud-p", &[], &mut failures, &mut successes);
		run_isa_tests("rv64ud-p", &[], &mut failures, &mut successes);

		println!();

		if !failures.is_empty() {
			eprintln!("\n\n\tFailures\n");
			for failure in &failures {
				eprintln!("{failure}\n");
			}
		}

		println!(
			"{}/{} succeeded.",
			successes,
			successes + failures.len().cast_signed() as i32
		);
		assert!(failures.is_empty());
	}
}
