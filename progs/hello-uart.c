#include "whisker.h"

int _start() {
  whisker_write_uart("Hello, World!");

  u8 buf[21];
  int_to_string(2147599, buf);
  whisker_write_uart(buf);

  while (true)
    ;
}
