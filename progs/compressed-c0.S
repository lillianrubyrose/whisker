    .section .text
    .global _start

    .extern whisker_write_uart

_start:
    li x8, 0x00010000   # Load base address into x8
    c.lw x9, 0(x8)      # Load word from memory into x9
    c.sw x9, 4(x8)      # Store word from x9 into memory at x8 + 4

    #la a0, msg
    #call whisker_write_uart  # Call UART write function

    # Halt execution
    #j _start  # Infinite loop

    #.section .rodata
#msg:
    #.asciz "C.LW and C.SW executed\n"
