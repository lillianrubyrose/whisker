    .section .text
    .global _start

    .extern whisker_write_uart

_start:
    li x8, 0x00010000
    c.lw x9, 0(x8)
    c.sw x9, 4(x8)

    la a0, msg
    call whisker_write_uart

_end:
    j _end

    .section .rodata
msg:
    .asciz "C.LW and C.SW executed\n"
