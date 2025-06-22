.section .text

.pushsection .text.entry
.global _start
_start:
    # zero bss segment
    la a0, _bss_start
    la a1, _bss_end
__zero_bss:
    bgeu a0, a1, __call_main
    sd zero, (a0)
    addi a0, a0, 8
    j __zero_bss
__call_main:
    la sp, _stack_top
    call main
    # insurance for if main returns
__main_ret_trap: j __main_ret_trap

.popsection
