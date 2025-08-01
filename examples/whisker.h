#ifndef WHISKER_H
#define WHISKER_H
#include <stdint.h>


#define UART_BASE ((volatile char *)0x10000000)
#define UART_DATA UART_BASE
#define UART_DIVISOR_LSB UART_BASE
#define UART_INTERRUPT_ENABLE (UART_BASE + 1)
#define UART_DIVISOR_MSB (UART_BASE + 1)
#define UART_INTERRUPT_ID (UART_BASE + 2)
#define UART_FIFO_CTRL (UART_BASE + 2)
#define UART_LINE_CTRL (UART_BASE + 3)
#define UART_LINE_STATUS (UART_BASE + 5)
#define UART_MODEM_STATUS (UART_BASE + 6)
#define UART_SCRATCH (UART_BASE + 7)

#define UART_DATA_AVAILABLE_BIT (1 << 0)
#define UART_OVERRUN_ERR_BIT (1 << 1)
#define UART_PARITY_ERR_BIT (1 << 2)
#define UART_FRAMING_ERR_BIT (1 << 3)
#define UART_BRK_BIT (1 << 4)
#define UART_THR_EMPTY_BIT (1 << 5)
#define UART_THR_IDLE_BIT (1 << 6)
#define UART_FIFO_ERR_BIT (1 << 7)

// str must be nul-terminated, and it should be free'd by the caller if
// necessary if str is not nul-terminated the program will go into an infinite
// loop, and read out of bounds memory, causing UB.
int32_t whisker_strlen(const char *str);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary
void whisker_write_uart(const char *str);

void int_to_string(int64_t val, char buf[21]);

#endif
