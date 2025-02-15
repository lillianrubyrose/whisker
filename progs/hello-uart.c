#include "whisker.h"

int main() {
  whisker_write_uart("Hello, World!");
  while (true)
    ;
}
