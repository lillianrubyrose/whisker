#!/bin/sh

if [ -z "$1" ]; then
    echo "Error: No input file provided"
    exit 1
fi

base_name=$(basename "$1" | cut -d. -f1)
file="progs/$1"

if [ ! -f "$file" ]; then
    echo "Error: File '$file' does not exist"
    exit 1
fi

riscv64-elf-gcc -O0 -march=rv64id -mcmodel=medany -std=c23 -c "$file" -o target/$base_name.o -nostdlib -nodefaultlibs
riscv64-elf-gcc -O0 -march=rv64id -mcmodel=medany -std=c23 -c progs/whisker.c -o target/whisker.o -nostdlib -nodefaultlibs
riscv64-elf-gcc -O0 -T linker.ld -o target/$base_name.elf target/$base_name.o target/whisker.o -nostdlib -nodefaultlibs

riscv64-elf-objcopy -O binary target/$base_name.elf target/$base_name.bin

echo "Compiled '$file' as bootrom to 'target/$base_name.bin'"
