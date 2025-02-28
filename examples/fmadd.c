#include "whisker.h"

int main() {
    float a = 2.f;
    float b = 3.f;
    float c = 4.f;
    float result;

    __asm__(
        "fmadd.s %0, %1, %2, %3"
        : "=f"(result)
        : "f"(a), "f"(b), "f"(c)
    );

    if(result == 10) {
        whisker_write_uart("fmadd.s is correct");
    } else {
        whisker_write_uart("fmadd.s is wrong");
    }

    while(true) {}
}
