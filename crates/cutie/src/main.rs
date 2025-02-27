use std::collections::HashSet;
use std::process::exit;
use std::{path::PathBuf, process::Command};

use clap::{Parser, Subcommand};
use tracing::level_filters::LevelFilter;
use tracing::*;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ISAExtension {
	Compressed,
	Float,
	Atomic,
	Multiplication,
}

impl ISAExtension {
	pub fn to_char(self) -> char {
		match self {
			ISAExtension::Compressed => 'c',
			ISAExtension::Float => 'f',
			ISAExtension::Atomic => 'a',
			ISAExtension::Multiplication => 'm',
		}
	}

	pub fn parse(str: &str) -> Result<Self, String> {
		match str.to_lowercase().as_str() {
			"c" => Ok(Self::Compressed),
			"f" => Ok(Self::Float),
			"a" => Ok(Self::Atomic),
			"m" => Ok(Self::Multiplication),
			_ => Err(format!("Invalid extension: {str}")),
		}
	}
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	Compile {
		#[arg(short, default_value_t = String::from("boot.bin"))]
		out: String,
		files: Vec<PathBuf>,
		#[arg(short = 'f', long = "flags", value_delimiter = ',', value_parser = ISAExtension::parse)]
		extensions: Vec<ISAExtension>,
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
	let args = Args::parse();
	match args.command {
		Commands::Compile { out, files, extensions } => compile(out, files, flatten_to_set(extensions)),
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

fn flatten_to_set<T: Eq + std::hash::Hash>(mut vec: Vec<T>) -> HashSet<T> {
	let mut set = HashSet::with_capacity(vec.len());
	set.extend(vec.drain(..));
	set
}

fn compile(out_name: String, files: Vec<PathBuf>, extensions: HashSet<ISAExtension>) {
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

		// This is the base ISA + D, GCC needs D even when it doesn't emit D instructions for some reason
		let mut march = String::from("rv64id");
		for ele in &extensions {
			march.push(ele.to_char());
		}
		info!("compiling with march: {march}");

		let mut cmd = Command::new(cc);
		cmd.args([&format!("-march={march}"), "-c", "-std=c23", "-O0", "-Wall", "-Wextra"])
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
