#include "whisker.h"

int _start() {
  whisker_write_uart("Hello, World!");

  i64 mrow = 7519;
  i64 mrrp = -142;
  i64 res = int_mul(mrow, mrrp);

  char buf[21];
  int_to_string(res, buf);
  whisker_write_uart(buf);

  while (true)
    ;
}
