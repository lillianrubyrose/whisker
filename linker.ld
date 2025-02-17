ENTRY(_start)

MEMORY {
    bootrom (rwx) : ORIGIN = 0x20000000, LENGTH = 65K
    dram (rw)     : ORIGIN = 0x00010000, LENGTH = 0x400000
}

SECTIONS {
    .text : {
        *(.text)
    } > bootrom

    .rodata : {
        *(.rodata)
    } > bootrom

    .data : {
        *(.data)
    } > bootrom

    .bss : {
        *(.bss)
    } > dram

    /DISCARD/ : {
        *(.comment)
    }
}
