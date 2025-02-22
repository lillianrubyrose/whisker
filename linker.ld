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

    /* alignment is needed to be able to zero 8 bytes at a time */
    .bss : ALIGN(8) {
        _bss_start = .;
        *(.bss)
        _bss_end = .;

        . = ALIGN(16);
        _stack_bottom = .;
        . += 64 * 1024;
        _stack_top = .;
    } > dram

    /DISCARD/ : {
        *(.comment)
    }
}
