#include "whisker.h"
#include <stdint.h>

int main() {
  whisker_write_uart("Hello, World!");

  int64_t mrow = 7519;
  int64_t mrrp = -142;
  int64_t res = mrow * mrrp;

  char buf[21];
  int_to_string(res, buf);
  whisker_write_uart(buf);

  while (true)
    ;
}
