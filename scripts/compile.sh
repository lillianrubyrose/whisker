#!/bin/sh

if [ -z "$1" ]; then
    echo "Error: No input file provided"
    exit 1
fi

if command -v riscv64-elf-gcc >/dev/null 2>&1 && command -v riscv64-elf-objcopy >/dev/null 2>&1; then
    CC="riscv64-elf-gcc"
    OBJCOPY="riscv64-elf-objcopy"
elif command -v riscv64-unknown-linux-gnu-gcc >/dev/null 2>&1 && command -v riscv64-unknown-linux-gnu-objcopy >/dev/null 2>&1; then
    CC="riscv64-unknown-linux-gnu-gcc"
    OBJCOPY="riscv64-unknown-linux-gnu-objcopy"
else
    echo "Error: No suitable RISC-V toolchain found."
    exit 1
fi

base_name=$(basename "$1" | cut -d. -f1)
file="progs/$1"

if [ ! -f "$file" ]; then
    echo "Error: File '$file' does not exist"
    exit 1
fi

$CC -O0 -march=rv64id -mcmodel=medany -std=c23 -c "$file" -o target/$base_name.o -nostdlib -nodefaultlibs -fno-stack-protector
$CC -O0 -march=rv64id -mcmodel=medany -std=c23 -c progs/whisker.c -o target/whisker.o -nostdlib -nodefaultlibs -fno-stack-protector
$CC -O0 -T linker.ld -o target/$base_name.elf target/$base_name.o target/whisker.o -nostdlib -nodefaultlibs

$OBJCOPY -O binary target/$base_name.elf target/$base_name.bin

echo "Compiled '$file' as bootrom to 'target/$base_name.bin'"
