use std::process::exit;
use std::{path::PathBuf, process::Command};

use clap::{Parser, Subcommand};
use cmd_lib::run_cmd;
use log::*;

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
	Compile2 {
		#[arg(short, default_value_t = String::from("boot.bin"))]
		out: String,
		files: Vec<PathBuf>,
	},
}

fn main() {
	env_logger::init();
	let args = Args::parse();
	match args.command {
		Commands::Compile { input } => compile(&input),
		Commands::Compile2 { out, files } => compile2(out, files),
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

fn compile2(out_name: String, files: Vec<PathBuf>) {
	if files.is_empty() {
		error!("no input files given");
		exit(1)
	}

	let base_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
	let target_dir = base_dir.join("target");

	let mut any_missing = false;
	for file in files.iter() {
		let full = base_dir.join(file);
		if !full.exists() {
			any_missing = true;
			error!("file `{}` does not exist", full.display());
		}
	}
	if any_missing {
		exit(1);
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

	let mut out_files = Vec::new();
	for file in files.iter() {
		info!("compiling {}", file.display());
		let file = base_dir.join(file);
		match file.extension() {
			Some(ext) => {
				if !(ext.eq_ignore_ascii_case("s") || ext.eq_ignore_ascii_case("asm") || ext.eq_ignore_ascii_case("c"))
				{
					error!("unsupported file extension {}", ext.to_string_lossy());
					exit(1)
				}
			}
			None => {
				error!("could not determine extension of file `{}`", file.display());
				exit(1)
			}
		};

		let out_path = target_dir.join(file.file_stem().unwrap()).with_extension("o");

		let mut cmd = Command::new(cc);
		cmd.args(["-march=rv64idc", "-c", "-std=c23", "-O0", "-Wall", "-Wextra"])
			.arg(file)
			.arg("-o")
			.arg(&out_path)
			.args(["-ffreestanding", "-fno-stack-protector"]);
		let output = cmd.output().unwrap();
		if !output.status.success() {
			error!("failed to compile: {}", String::from_utf8_lossy(&output.stderr));
			exit(1);
		}
		out_files.push(out_path);
	}

	// ========
	// LINKING
	// ========
	for file in out_files.iter() {
		info!("linking `{}`", file.strip_prefix(&target_dir).unwrap().display());
	}

	let linked_path = target_dir.join("out.elf");
	let mut cmd = Command::new(cc);
	cmd.args(["-T", "linker.ld", "-nostdlib", "-o"])
		.arg(&linked_path)
		.args(out_files);
	let output = cmd.output().unwrap();
	if !output.status.success() {
		error!("failed to link: {}", String::from_utf8_lossy(&output.stderr));
		exit(1);
	}

	// =======================
	// copying to flat binary
	// =======================
	info!("copying to flat binary...");
	let out_path = target_dir.join(out_name);
	let mut cmd = Command::new(objcopy);
	cmd.args(["-O", "binary"]).arg(linked_path).arg(&out_path);
	let output = cmd.output().unwrap();
	if !output.status.success() {
		error!("failed to copy: {}", String::from_utf8_lossy(&output.stderr));
		exit(1);
	}

	info!(
		"DONE! output binary at `{}`",
		out_path.strip_prefix(target_dir).unwrap().display()
	);
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
		"$cc" -O0 -march=rv64idc -mcmodel=medany -std=c23 -c "$program_path" -o "$target_dir/$base_name.o" -nostdlib -nodefaultlibs -fno-stack-protector;
		"$cc" -O0 -march=rv64idc -mcmodel=medany -std=c23 -c "$progs_dir/whisker.c" -o "$target_dir/whisker.o" -nostdlib -nodefaultlibs -fno-stack-protector;
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
