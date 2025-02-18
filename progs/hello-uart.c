#include "whisker.h"

int _start() {
  whisker_write_uart("Hello, World!");

  float a = 123.f;
  float b = 100.f;
  a = a - b;

  while (true)
    ;
}
