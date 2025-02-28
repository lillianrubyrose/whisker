# whisker

A RISC-V emulator written in Rust, made with love. 💜

# Overview

whisker is a work-in-progress RISC-V emulator that aims to provide a complete and accurate implementation of the RV64 ISA.

# Development progress

| Date         | Milestone                                                  |
| ------------ | ---------------------------------------------------------- |
| Feb 13, 2025 | First commit                                               |
| Feb 15, 2025 | Most of RV32 Integer instruction set implemented           |
| Feb 24, 2025 | Most of RV32 & RV64 compressed instruction set implemented |
| Feb 27, 2025 | RV32 & RV64 atomic instruction set implemented             |
| Feb 27, 2025 | RV32 and RV64 multiply instruction set implemented         |

# Building programs

whisker uses a cargo script for compiling programs. You must have a RISC-V 64 build toolchain in your `$PATH`. The script searches for the following toolchains:
- `riscv64-elf-*`
- `riscv64-unknown-linux-gnu-*`
- `riscv64-unknown-elf-*`

If you are a NixOS user, you can use the flake devshell to have a reliable environment.

## Setup

1. First, compile the boot loader:
```sh
cargo cutie compile-boot-loader
```

2. Then compile a program:
```
cargo cutie compile <path to files...>
```

Compilation options:
- Pass compiler arguments with `-C="<args>"`:
	```sh
	cargo cutie compile program.c -C="-O3"
	```
- Specify ISA extensions with `-f`:
	- `f` - float
	- `m` - multiply
	- `a` - atomic
	- `c` - compressed

	Example for multiple extensions:
	```sh
	cargo cutie compile program.c -ff,m,a,c
	```
- For programs with a `main` routine instead of `_start`, include the runtime:
	```sh
	cargo cutie compile examples/runtime.s program.c`
	```

### Output files

All compiled binaries and object files are placed in the `target` directory.

### Development setup

For NixOS users, a devShell is provided through the Nix flake:
```sh
nix develop
```

For Arch Linux users, install the following packages:
```
TODO: Document
```
