ENTRY(__bootrom_start)

MEMORY {
    bootrom (rwx) : ORIGIN = 0x00001000, LENGTH = 65K
    dram (rwx)     : ORIGIN = 0x80000000, LENGTH = 0x10000000
}

SECTIONS {
    .text : {
        *(.text.entry)
        *(.text.*)
    } > bootrom

    .rodata : {
        *(.rodata)
    } > bootrom

    .data : {
        *(.data)
    } > bootrom

    .bss : {
    } > bootrom

    /DISCARD/ : {
        *(.comment)
    }
}

_user_start = ORIGIN(dram);
