ENTRY(_start)

MEMORY {
    dram (rwx)     : ORIGIN = 0x80000000, LENGTH = 0x10000000
}

SECTIONS {
    .text : {
        /* ensure that the entry point comes first */
        *(.text.entry)
        /* .text and any subsections should be here */
        *(.text.*)
    } > dram

    .rodata : {
        *(.rodata)
    } > dram

    .data : {
        *(.data)
    } > dram

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
