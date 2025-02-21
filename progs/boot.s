.section .bss
.align 16
_stack_bottom:
.skip 64 * 1024
_stack_top:

.section .text
.global _start
_start:
    la sp, _stack_top
    la t0, trap
    csrw 0x305, t0
    tail main
    # insurance in case main returns
    2: j 2b

trap:
    # this is just here to do something visible at the moment
    2: li x1, 0x99
    j 2b
