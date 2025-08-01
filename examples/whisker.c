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

void rev_arr(char arr[], int64_t len){
    for(int64_t i = 0; i < len / 2; i += 1){
        char tmp = arr[i];
        arr[i] = arr[len - 1 - i];
        arr[len - 1 - i] = tmp;
    }
}

void int_to_string(int64_t val, char buf[21]){
    int64_t idx = 0;
    int64_t sign = val < 0;
    if(sign){
        buf[idx++] = '-';
        val = -val;
    }
    // build up the array from least significant digit to most
    do {
        buf[idx++] = '0' + (val % 10);
        val /= 10;
    } while(val > 0);
    // idx now points to the byte after the written characters
    // reverse the digits, skipping the negative sign if it exists
    rev_arr(buf + sign, idx - sign);
    // write the null terminator
    buf[idx] = '\0';
}
