#include "whisker.h"

int32_t whisker_strlen(const char *str) {
  int32_t len = 0;

  for (;;) {
    if (str[len] == '\0') {
      break;
    }
    len++;
  }

  return len;
}

void whisker_write_uart(const char *str) {
  int32_t len = whisker_strlen(str);
  for (int32_t i = 0; i < len; i++) {
      // wait until the tx reg is empty
      while(*UART_LINE_CTRL & (1 << 5)){}
      *UART_DATA = str[i];
  }
}

// [chr for i in range(100) for chr in f"{i:02}"]
static const char digit_pairs[200] = {
    '0', '0', '0', '1', '0', '2', '0', '3', '0', '4', '0', '5', '0', '6', '0',
    '7', '0', '8', '0', '9', '1', '0', '1', '1', '1', '2', '1', '3', '1', '4',
    '1', '5', '1', '6', '1', '7', '1', '8', '1', '9', '2', '0', '2', '1', '2',
    '2', '2', '3', '2', '4', '2', '5', '2', '6', '2', '7', '2', '8', '2', '9',
    '3', '0', '3', '1', '3', '2', '3', '3', '3', '4', '3', '5', '3', '6', '3',
    '7', '3', '8', '3', '9', '4', '0', '4', '1', '4', '2', '4', '3', '4', '4',
    '4', '5', '4', '6', '4', '7', '4', '8', '4', '9', '5', '0', '5', '1', '5',
    '2', '5', '3', '5', '4', '5', '5', '5', '6', '5', '7', '5', '8', '5', '9',
    '6', '0', '6', '1', '6', '2', '6', '3', '6', '4', '6', '5', '6', '6', '6',
    '7', '6', '8', '6', '9', '7', '0', '7', '1', '7', '2', '7', '3', '7', '4',
    '7', '5', '7', '6', '7', '7', '7', '8', '7', '9', '8', '0', '8', '1', '8',
    '2', '8', '3', '8', '4', '8', '5', '8', '6', '8', '7', '8', '8', '8', '9',
    '9', '0', '9', '1', '9', '2', '9', '3', '9', '4', '9', '5', '9', '6', '9',
    '7', '9', '8', '9', '9'};

void int_to_string(const int64_t val, char buf[21]) {
  // is there a better way to do this lol
  char *buf_end = buf + 20;
  *buf_end = '\0';

  char *buf_cursor = buf_end;
  const bool neg = val < 0;
  uint64_t unsigned_val = neg ? -(uint64_t)val : (uint64_t)val;

  // 2 digits at a time, reduces the amount of div/mod
  while (unsigned_val >= 100) {
    const uint64_t rem = unsigned_val % 100;
    unsigned_val /= 100;
    buf_cursor -= 2;
    *(buf_cursor) = digit_pairs[rem * 2];
    *(buf_cursor + 1) = digit_pairs[rem * 2 + 1];
  }

  if (unsigned_val < 10) { // last digit
    buf_cursor -= 1;
    *buf_cursor = '0' + unsigned_val;
  } else { // or last two digits
    buf_cursor -= 2;
    *(buf_cursor) = digit_pairs[unsigned_val * 2];
    *(buf_cursor + 1) = digit_pairs[unsigned_val * 2 + 1];
  }

  if (neg) {
    buf_cursor -= 1;
    *buf_cursor = '-';
  }

  // memmove(buf, buf_cursor, buf_end - buf_cursor + 1)
  char *dst = buf;
  for (const char *src = buf_cursor; src <= buf_end; ++src, ++dst) {
    *dst = *src;
  }
}
