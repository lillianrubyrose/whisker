.section .text
.global _start
_start:
    la sp, _stack_top
    # zero bss segment
    la a0, _bss_start
    la a1, _bss_end
_zero_bss:
    bgeu a0, a1, 2f
    sd zero, (a0)
    addi a0, a0, 8
    j _zero_bss
2:

    # set up a default trap handler that just spins
    la t0, trap
    csrw 0x305, t0

    call main
    # insurance in case main returns
    9: j 9b

trap:
    # this is just here to do something visible at the moment
    2: li x1, 0x99
    j 2b
