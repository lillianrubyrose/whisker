# whisker

WIP RISC-V emulator written in Rust made with love.

# Building programs

Use the `scripts/compile.sh` shell script, you must have riscv64-elf-gcc and riscv64-elf-objcopy available in your $PATH.

Usage: `scripts/compile.sh <name>`, the file with "<name>" must be present in the `progs` directory.

example: `scripts/compile.sh hello-uart.c`, it will then place it as `target/hella-uart.bin`
