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
__shutdown:
    li t0, 0x100000
    li t1, 0x5555
    sw t1, 0(t0)
__main_ret_trap:
    wfi
    j __main_ret_trap

.popsection
