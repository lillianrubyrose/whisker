use clap::{Parser, Subcommand};
use cmd_lib::run_cmd;
use std::{path::PathBuf, process::Command};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	Compile {
		/// Input file name
		input: String,
	},
}

fn main() {
	let args = Args::parse();
	match args.command {
		Commands::Compile { input } => compile(&input),
	}
}

fn find_command(options: &[&'static str]) -> Option<&'static str> {
	for opt in options.iter() {
		// This only checks if the command is available in PATH, not if it returns OK status
		if Command::new(opt).output().is_ok() {
			return Some(opt);
		}
	}

	None
}

fn compile(input: &str) {
	let base_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
	let progs_dir = base_dir.join("progs");
	let target_dir = base_dir.join("target");

	let program_path = progs_dir.join(input);
	if !PathBuf::from(&program_path).exists() {
		eprintln!("Error: File '{}' does not exist", program_path.to_string_lossy());
		std::process::exit(1);
	}

	let Some(cc) = find_command(&[
		"riscv64-elf-gcc",
		"riscv64-unknown-linux-gnu-gcc",
		"riscv64-unknown-elf-gcc",
	]) else {
		eprintln!("Error: No suitable RISC-V toolchain found (Missing GCC).");
		std::process::exit(1);
	};
	let Some(objcopy) = find_command(&[
		"riscv64-elf-objcopy",
		"riscv64-unknown-linux-gnu-objcopy",
		"riscv64-unknown-elf-objcopy",
	]) else {
		eprintln!("Error: No suitable RISC-V toolchain found (Missing objcopy).");
		std::process::exit(1);
	};

	let base_name = PathBuf::from(input)
		.file_stem()
		.expect("Invalid file name")
		.to_string_lossy()
		.to_string();

	if let Err(e) = run_cmd!(
		"$cc" -O0 -march=rv64id -mcmodel=medany -std=c23 -c "$program_path" -o "$target_dir/$base_name.o" -nostdlib -nodefaultlibs -fno-stack-protector;
		"$cc" -O0 -march=rv64id -mcmodel=medany -std=c23 -c "$progs_dir/whisker.c" -o "$target_dir/whisker.o" -nostdlib -nodefaultlibs -fno-stack-protector;
		"$cc" -O0 -T "linker.ld" -o "$target_dir/$base_name.elf" "$target_dir/$base_name.o" "$target_dir/whisker.o" -nostdlib -nodefaultlibs;
		"$objcopy" -O binary "$target_dir/$base_name.elf" "$target_dir/$base_name.bin";
	) {
		eprintln!("Command execution failed: {}", e);
		std::process::exit(1);
	}

	println!(
		"Compiled '{input}' as bootrom to '{}/{base_name}.bin'",
		target_dir.to_string_lossy()
	);
}
