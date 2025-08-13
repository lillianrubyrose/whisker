#include <stdint.h>
#include <stdbool.h>
#include "whisker.h"
#include "riscv-csr.h"

int real_main();

#define MODE_S 0b01

__attribute__((aligned(4)))
void trap() {
    while(true) {}
}

int main() {
    // set M Previous Privilege mode to Supervisor, for mret.
    uint_xlen_t x = csr_read_mstatus();
    x &= ~MSTATUS_MPP_BIT_MASK;
    x |= MODE_S << MSTATUS_MPP_BIT_OFFSET;
    csr_write_mstatus(x);

    // set MEPC to return to real_main
    csr_write_mepc((uint64_t)real_main);

    csr_write_mtvec((uint_xlen_t) trap);

    __asm__ volatile("mret");

    while(true) {}
}

int real_main() {
    while(true) {
        whisker_write_uart("\nmeow from S mode\n");
        uint_xlen_t x = csr_read_mstatus();
        char str[21];
        int_to_string(x, str);
        whisker_write_uart(str);
    }
}
