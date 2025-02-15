#ifndef WHISKER_H
#define WHISKER_H

static char *UART = (char *)0x10000000;

// str must be nul-terminated, and it should be free'd by the caller if
// necessary if str is not nul-terminated the program will go into an infinite
// loop, and read out of bounds memory, causing UB.
int whisker_strlen(char *str);

// str must be nul-terminated, and it should be free'd by the caller if
// necessary
void whisker_write_uart(const char *str);

#endif
