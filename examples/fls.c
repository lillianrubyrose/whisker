#include <stdint.h>
#include <stdbool.h>
#include "whisker.h"

static __attribute__((noinline)) int generic_fls(unsigned int x)
{
	int r = 32;

	if (!x)
		return 0;
	if (!(x & 0xffff0000u)) {
		x <<= 16;
		r -= 16;
	}
	if (!(x & 0xff000000u)) {
		x <<= 8;
		r -= 8;
	}
	if (!(x & 0xf0000000u)) {
		x <<= 4;
		r -= 4;
	}
	if (!(x & 0xc0000000u)) {
		x <<= 2;
		r -= 2;
	}
	if (!(x & 0x80000000u)) {
		x <<= 1;
		r -= 1;
	}
	return r;
}

int main() {
    char str[21];

    int val = 4095;
    __asm__ volatile("nop" : "+r" (val));

    int ret = generic_fls(val);
    int_to_string(ret, str);

    whisker_write_uart("fls(4095) == ");
    whisker_write_uart(str);
    whisker_write_uart(" (should be 12)\n");


    while(true){}
}
