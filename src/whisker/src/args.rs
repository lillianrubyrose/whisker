use std::{fs, path::PathBuf};

use clap::{ArgGroup, Command, arg, command, value_parser};

#[derive(Debug)]
pub enum KernelData {
	Elf(Vec<u8>),
	Raw(Vec<u8>, Option<u64>),
}

#[derive(Debug)]
pub enum CliCommand {
	Run {
		bootrom: Vec<u8>,
		bootloader: Option<Vec<u8>>,
		kernel_data: Option<KernelData>,

		fs_img: Option<PathBuf>,

		logfile: Option<PathBuf>,
		use_gdb: bool,
	},
	GenerateGdbXML,
}

pub fn get_command() -> CliCommand {
	let cmd = command!().propagate_version(true).subcommand_required(true).subcommand(
		Command::new("run")
			.about("run the emulator")
			.arg(arg!(--bootrom <PATH>).value_parser(value_parser!(PathBuf)).required(true))
			.arg(arg!(--bootloader <PATH> "first stage bootloader").value_parser(value_parser!(PathBuf)).long_help(
				r#"an ELF file containing the first stage of non-builtin code.
this is typically a bootloader or SBI or similar software, however it may be used to boot "bare metal" programs.
always loaded at the start of RAM: 0x8000_0000."#,
			))
			.arg(
				arg!(--kernel <PATH> "second stage kernel or raw kernel image")
				.value_parser(value_parser!(PathBuf))
				.long_help(
					"either an ELF file containing a second stage kernel, to be jumped to by the bootloader, or a raw kernel image if --raw-kernel is set.",
				),
			)
			.group(
				ArgGroup::new("input_files")
					.required(true)
					.multiple(true)
					.args(["bootloader", "kernel"]),
			)
			.arg(
				arg!(--"raw-kernel" "assume the kernel is a raw binary")
					.requires("kernel")
					.long_help(
						r"use the passed `kernel` arg as a raw binary, rather than parsing as an ELF file.
you want to use this when booting a linux kernel.",
					),
			)
			.arg(
				arg!(--"kernel-offset" <ADDR> "address to load raw kernel files")
				    .value_parser(clap_num::maybe_hex::<u64>)
				    .requires("raw-kernel")
				    .long_help(r"the base address at which the the kernel file should be loaded at.
must be within the range of main memory.
for use with OpenSBI in FW_JUMP mode, this should be 0x8020_0000.")
			)
			.arg(arg!(--"use-gdb" "opens a gdb stub listening on port 2424"))
			.arg(arg!(--"logfile" <PATH>).value_parser(value_parser!(PathBuf)))
			.arg(arg!(--"fs-img" <PATH>).value_parser(value_parser!(PathBuf)))
	).subcommand(Command::new("generate-gdb-xml"));

	let matches = cmd.get_matches();

	match matches.subcommand() {
		Some(("generate-gdb-xml", _)) => CliCommand::GenerateGdbXML,
		Some(("run", args)) => {
			let bootrom_path = args.get_one::<PathBuf>("bootrom").expect("bootrom arg is required");
			let bootrom = fs::read(bootrom_path)
				.unwrap_or_else(|e| panic!("could not read bootrom data from {}: {}", bootrom_path.display(), e));

			let bootloader = args.get_one::<PathBuf>("bootloader").map(|path| {
				fs::read(path)
					.unwrap_or_else(|e| panic!("could not read bootloader data from {}: {}", path.display(), e))
			});

			let kernel_data = args
				.get_one::<PathBuf>("kernel")
				.map(|path| {
					fs::read(path)
						.unwrap_or_else(|e| panic!("could not read bootloader data from {}: {}", path.display(), e))
				})
				.map(|data| {
					if args.get_flag("raw-kernel") {
						let offset = args.get_one::<u64>("kernel-offset").copied();
						KernelData::Raw(data, offset)
					} else {
						KernelData::Elf(data)
					}
				});

			assert!(
				bootloader.is_some() || kernel_data.is_some(),
				"somehow neither bootloader nor kernel were passed (should be prevented by clap)"
			);

			let fs_img = args.get_one::<PathBuf>("fs-img").cloned();
			let logfile = args.get_one::<PathBuf>("logfile").cloned();
			let use_gdb = args.get_flag("use-gdb");

			CliCommand::Run {
				bootrom,
				bootloader,
				kernel_data,
				fs_img,
				logfile,
				use_gdb,
			}
		}
		_ => unreachable!(),
	}
}
