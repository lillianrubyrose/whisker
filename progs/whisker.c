#include "whisker.h"

int whisker_strlen(char *str) {
  int len = 0;

  for (;;) {
    if (str[len] == '\0') {
      break;
    }
    len++;
  }

  return len;
}

void whisker_write_uart(const char *str) {
  int len = whisker_strlen((char *)str);
  for (int i = 0; i < len; i++) {
    *UART = str[i];
  }
}
