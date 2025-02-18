#ifndef WHISKER_H
#define WHISKER_H

typedef unsigned char      u8;
typedef unsigned short     u16;
typedef unsigned int       u32;
typedef unsigned long long u64;

typedef signed char        i8;
typedef signed short       i16;
typedef signed int         i32;
typedef signed long long   i64;

static char *UART = (char *)0x10000000;

// if lhs or rhs are i64::MIN, behavior is undefined
// if the computation of lhs * rhs overflows, behavior is undefined
i64 int_mul(i64 lhs, i64 rhs);

// lhs and rhs must not be i64::MIN or behavior is undefined
// if lhs or rhs are negative, the remainder will likely not be useful
void int_div(i64 lhs, i64 rhs, i64* quotient, i64* remainder);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary if str is not nul-terminated the program will go into an infinite
// loop, and read out of bounds memory, causing UB.
i32 whisker_strlen(u8 *str);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary
void whisker_write_uart(const char *str);

void int_to_string(i64 val, char buf[21]);

#endif
