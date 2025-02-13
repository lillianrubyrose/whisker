#!/bin/sh
clang --target=riscv64 -march=rv64i progs/$1.s -c -o target/$1.o
riscv64-elf-objcopy -O binary --only-section=.text target/$1.o target/$1.bin
