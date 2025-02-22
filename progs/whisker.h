#ifndef WHISKER_H
#define WHISKER_H
#include <stdint.h>

static char *UART = (char *)0x10000000;

// if lhs or rhs are int64_t::MIN, behavior is undefined
// if the computation of lhs * rhs overflows, behavior is undefined
int64_t int_mul(int64_t lhs, int64_t rhs);

void mulwide(int64_t lhs, int64_t rhs, int64_t *restrict hi, int64_t *restrict lo);

void div_10(int64_t lhs, int64_t *restrict quot, int64_t *restrict rem);

// lhs and rhs must not be int64_t::MIN or behavior is undefined
// if lhs or rhs are negative, the remainder will likely not be useful
void int_div(int64_t lhs, int64_t rhs, int64_t* quotient, int64_t* remainder);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary if str is not nul-terminated the program will go into an infinite
// loop, and read out of bounds memory, causing UB.
int32_t whisker_strlen(const char *str);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary
void whisker_write_uart(const char *str);

void int_to_string(int64_t val, char buf[21]);

#endif
