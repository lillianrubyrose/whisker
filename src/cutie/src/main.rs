use std::collections::HashSet;
use std::path::Path;
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
	pub fn all() -> HashSet<Self> {
		HashSet::from([Self::Compressed, Self::Float, Self::Atomic, Self::Multiplication])
	}

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
	/// a shortcut for compiling the boot loader with its default settings:
	/// - compile src/boot/boot.s to boot.bin with linker script src/boot/boot.ld
	/// -  no extensions enabled, no compile args
	CompileBootLoader,
	/// a shortcut for compiling whisker.c to a static library
	CompileWhiskerLib,
	Compile {
		#[arg(short, long, default_value_t = String::from("kernel.bin"))]
		out: String,
		#[arg(long, short = 'T', default_value = String::from("examples/kernel.ld"))]
		linker_script: PathBuf,
		#[arg(short = 'f', long, value_delimiter = ',', value_parser = ISAExtension::parse)]
		extensions: Vec<ISAExtension>,
		#[arg(long, short = 'C')]
		compile_args: Vec<String>,

		files: Vec<PathBuf>,
	},
	CompileStaticLib {
		#[arg(short, long, default_value_t = String::from("libmylib.a"))]
		out: String,
		#[arg(short = 'f', long, value_delimiter = ',', value_parser = ISAExtension::parse)]
		extensions: Vec<ISAExtension>,
		#[arg(long, short = 'C')]
		compile_args: Vec<String>,

		files: Vec<PathBuf>,
	},

	Objcopy {
		elf: PathBuf,
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
		Commands::Compile {
			out,
			files,
			linker_script,
			extensions,
			compile_args,
		} => {
			let objs = compile(files.as_slice(), flatten_to_set(extensions), compile_args.as_slice());
			let elf = link_to_elf(objs.as_slice(), linker_script.as_path());
			copy_to_flat_bin(&elf, out.as_str());
		}
		Commands::CompileStaticLib {
			files,
			extensions,
			out,
			compile_args,
		} => {
			let objs = compile(files.as_slice(), flatten_to_set(extensions), compile_args.as_slice());
			create_staticlib(objs.as_slice(), out.as_str());
		}
		Commands::Objcopy { elf } => {
			let mut out_bin_name = elf.file_stem().unwrap().to_string_lossy().into_owned();
			out_bin_name.push_str(".bin");
			copy_to_flat_bin(elf.as_path(), &out_bin_name);
		}

		// shortcuts/special cases
		Commands::CompileBootLoader => {
			let bootloader_name = "boot.bin";
			let bootloader_path = PathBuf::from("src/boot/boot.s");
			let linker_script = PathBuf::from("src/boot/boot.ld");
			let objs = compile(&[bootloader_path], HashSet::new(), &[]);
			let elf = link_to_elf(objs.as_slice(), linker_script.as_path());
			copy_to_flat_bin(&elf, bootloader_name);
		}
		Commands::CompileWhiskerLib => {
			let whisker_path = PathBuf::from("examples/whisker.c");
			let objs = compile(&[whisker_path], ISAExtension::all(), &[]);
			create_staticlib(objs.as_slice(), "libwhisker.a");
		}
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

/// compiles all files in `files` with the given arguments
/// returns a list of the compiled object files
fn compile(files: &[PathBuf], extensions: HashSet<ISAExtension>, compile_args: &[String]) -> Vec<PathBuf> {
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
		cmd.args([
			&format!("-march={march}"),
			"-mcmodel=medany",
			"-c",
			"-std=c23",
			"-O0",
			"-Wall",
			"-Wpedantic",
			"-Wextra",
		])
		.arg(file)
		.arg("-o")
		.arg(&out_path)
		.args(["-ffreestanding", "-fno-stack-protector"])
		.args(compile_args);
		let output = cmd.output().unwrap();
		if !output.status.success() {
			error!("failed to compile: {}", String::from_utf8_lossy(&output.stderr));
			exit(1);
		}
		out_files.push(out_path);
	}
	out_files
}

fn link_to_elf(files: &[PathBuf], linker_script: &Path) -> PathBuf {
	let base_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
	let target_dir = base_dir.join("target");

	let Some(cc) = find_command(&[
		"riscv64-elf-gcc",
		"riscv64-unknown-linux-gnu-gcc",
		"riscv64-unknown-elf-gcc",
	]) else {
		eprintln!("Error: No suitable RISC-V toolchain found (Missing GCC).");
		std::process::exit(1);
	};

	for file in files.iter() {
		info!("linking `{}`", file.strip_prefix(&target_dir).unwrap().display());
	}

	let linked_path = target_dir.join("out.elf");
	let mut cmd = Command::new(cc);
	cmd.args([
		"-mcmodel=medany",
		"-nostdlib",
		"-Wl,--fatal-warnings",
		"-Wl,--no-warn-rwx-segments", // this is not ideal, but we do it anyway
		"-o",
	])
	.arg(&linked_path)
	.arg("-T")
	.arg(linker_script)
	.args(files);
	let output = cmd.output().unwrap();
	if !output.status.success() {
		error!("failed to link: {}", String::from_utf8_lossy(&output.stderr));
		exit(1);
	}

	if !output.stdout.is_empty() {
		info!("linker stdout:\n{}", String::from_utf8_lossy(output.stdout.as_slice()));
	}
	if !output.stderr.is_empty() {
		warn!("linker stderr:\n{}", String::from_utf8_lossy(output.stderr.as_slice()));
	}

	linked_path
}

fn copy_to_flat_bin(linked_path: &Path, out_bin_name: &str) {
	let base_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
	let target_dir = base_dir.join("target");

	let Some(objcopy) = find_command(&[
		"riscv64-elf-objcopy",
		"riscv64-unknown-linux-gnu-objcopy",
		"riscv64-unknown-elf-objcopy",
	]) else {
		eprintln!("Error: No suitable RISC-V toolchain found (Missing objcopy).");
		std::process::exit(1);
	};

	// =======================
	// copying to flat binary
	// =======================
	info!("copying to flat binary...");
	let out_path = target_dir.join(out_bin_name);
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

fn create_staticlib(files: &[PathBuf], out_lib_name: &str) {
	let base_dir = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
	let target_dir = base_dir.join("target");

	let Some(ar) = find_command(&[
		"riscv64-elf-ar",
		"riscv64-unknown-linux-gnu-ar",
		"riscv64-unknown-elf-ar",
	]) else {
		eprintln!("Error: No suitable RISC-V toolchain found (Missing ar).");
		std::process::exit(1);
	};

	for file in files.iter() {
		info!("archiving `{}`", file.strip_prefix(&target_dir).unwrap().display());
	}

	let out_lib_path = target_dir.join(out_lib_name);
	let mut cmd = Command::new(ar);
	cmd.arg("rcs").arg(&out_lib_path).args(files);
	let output = cmd.output().unwrap();
	if !output.status.success() {
		error!("failed to create archive: {}", String::from_utf8_lossy(&output.stderr));
		exit(1);
	}

	if !output.stdout.is_empty() {
		info!("ar stdout:\n{}", String::from_utf8_lossy(output.stdout.as_slice()));
	}
	if !output.stderr.is_empty() {
		warn!("ar stderr:\n{}", String::from_utf8_lossy(output.stderr.as_slice()));
	}

	info!(
		"DONE! output library at `{}`",
		out_lib_path.strip_prefix(target_dir).unwrap().display()
	);
}
