.section .text

.pushsection .text.entry
.global __bootrom_start
__bootrom_start:
    # set up a default trap handler that just spins
    la t0, trap
    csrw 0x305, t0

    tail _user_start
.popsection

trap:
    # this is just here to do something visible at the moment
    2: li x1, 0x99
    j 2b
