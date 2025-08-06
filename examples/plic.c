#include <stdint.h>
#include <stdbool.h>
#include "whisker.h"
#include "riscv-csr.h"


#define UART_IRQ 10

void do_trap() {
    // claim an IRQ
    uint32_t irq = *(uint32_t*)(PLIC_BASE + 0x200000 + 0x1000 * 0 + 4);
    whisker_write_uart("irq claimed:");
    char str[21];
    int_to_string(irq, str);
    whisker_write_uart(str);
    whisker_write_uart("\n");

    if(irq != 10){
        whisker_write_uart("unknown IRQ: ");
        int_to_string(irq, str);
        whisker_write_uart(str);
        whisker_write_uart("\n");
        while(true){}
    }

    bool data_avail;
    while((data_avail = *UART_LINE_STATUS & 1)) {
        char c = *UART_DATA;
        char str[2] = {0};
        str[0] = c;
        whisker_write_uart(str);
    }

    // complete IRQ 10
    *(uint32_t*)(PLIC_BASE + 0x200000 + 0x1000 * 0 + 4) = 10;
}

__attribute__((naked, noreturn))
void trap() {
    __asm__("call do_trap; mret");
}

int main() {
  whisker_write_uart("PLIC tests");

  // set uart priority to 1
  *(uint32_t*)(PLIC_BASE + 4*UART_IRQ) = 1;
  // enable uart for context 0
  *(uint32_t*)(PLIC_BASE + 0x2000 + 0x80 * 0 + UART_IRQ/32) = 1 << UART_IRQ;
  // set threshold for context 0 to 0
  *(uint32_t*)(PLIC_BASE + 0x200000 + 0x1000 * 0) = 0;

  csr_write_mtvec((uint_xlen_t)trap);
  csr_set_bits_mstatus(MSTATUS_MIE_BIT_MASK);
  csr_set_bits_mie(MIE_MEI_BIT_MASK);

  *UART_INTERRUPT_ENABLE = 1;

  while(1){}
}
