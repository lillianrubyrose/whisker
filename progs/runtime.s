.section .text
.global _start
_start:
    # zero bss segment
    la a0, _bss_start
    la a1, _bss_end
_zero_bss:
    bgeu a0, a1, 2f
    sd zero, (a0)
    addi a0, a0, 8
    j _zero_bss
2:

    la sp, _stack_top
    call main
    # insurance for if main returns
    9: j 9b
