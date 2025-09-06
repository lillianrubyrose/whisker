.section .text

.pushsection .text.entry
.global __bootrom_start
# NOTE: upon entry:
# a0 contains the boot hartid
# a1 contains a pointer to the DTB
__bootrom_start:
    # set up a default trap handler that just spins
    la t0, trap
    csrw 0x305, t0

    tail _user_start
.popsection

.align 4
trap:
    # this is just here to do something visible at the moment
    2: li x1, 0x99
    j 2b
