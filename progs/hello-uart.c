#include "whisker.h"

int _start() {
  whisker_write_uart("Hello, World!");
  while (true)
    ;
}
