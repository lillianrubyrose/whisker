#include <stdint.h>
#include <stdbool.h>
#include "whisker.h"
#include "riscv-csr.h"

void handle_uart() {
    bool data_avail;
    while((data_avail = *UART_LINE_STATUS & 1)) {
        char c = *UART_DATA;
        char str[2] = {0};
        str[0] = c;
        whisker_write_uart(str);
    }
}

__attribute__((naked, noreturn))
void trap() {
    __asm__("call handle_uart; csrw mip, 0; mret");
}

int main() {
  whisker_write_uart("Hello, World!");

  int64_t mrow = 7519;
  int64_t mrrp = -142;
  int64_t res = mrow * mrrp;

  char buf[21];
  int_to_string(res, buf);
  whisker_write_uart(buf);

  csr_write_mtvec((uint_xlen_t)trap);
  csr_set_bits_mstatus(MSTATUS_MIE_BIT_MASK);
  csr_set_bits_mie(MIE_MEI_BIT_MASK);

  *UART_INTERRUPT_ENABLE = 1;

  while(1){}
}
