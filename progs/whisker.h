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

static u8 *UART = (char *)0x10000000;

// str must be nul-terminated, and it should be free'd by the caller if
// necessary if str is not nul-terminated the program will go into an infinite
// loop, and read out of bounds memory, causing UB.
i32 whisker_strlen(u8 *str);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary
void whisker_write_uart(const u8 *str);

// u8* int_to_string(i32 value);

#endif
