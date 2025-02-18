#include "whisker.h"

i64 int_mul(i64 lhs, i64 rhs) {
    // get final sign and absolute values
    int sign = 0;
    if(lhs < 0){
        sign ^= 1;
        lhs = -lhs;
    }
    if(rhs < 0){
        sign^= 1;
        rhs = -rhs;
    }
    i64 acc = 0;
    while(rhs > 0){
        acc += lhs;
        rhs -= 1;
    }
    if(sign){
        acc = -acc;
    }
    return acc;
}

// dont pass in i64::MIN
void int_div(i64 lhs, i64 rhs, i64* quotient, i64* remainder){
    // get final sign for division, and get absolute values
    int sign = 0;
    if(lhs < 0){
        sign ^= 1;
        lhs = -lhs;
    }
    if(rhs < 0){
        sign^= 1;
        rhs = -rhs;
    }
    i64 quot = 0;
    i64 rem = lhs;
    while(rem >= rhs){
        rem -= rhs;
        quot += 1;
    }
    if(sign){
        quot = -quot;
    }
    *quotient = quot;
    *remainder = rem;
}

i32 whisker_strlen(u8 *str) {
  i32 len = 0;

  for (;;) {
    if (str[len] == '\0') {
      break;
    }
    len++;
  }

  return len;
}

void whisker_write_uart(const char *str) {
  i32 len = whisker_strlen((u8 *)str);
  for (i32 i = 0; i < len; i++) {
    *UART = str[i];
  }
}

void int_to_string(i64 val, char buf[21]){
    i64 idx = 0;
    i64 sign = val < 0;
    if(sign){
        buf[idx++] = '-';
        val = -val;
    }
    // build up the array from least significant digit to most
    do {
        i64 q, r;
        int_div(val, 10, &q, &r);
        buf[idx++] = r + '0';
        val = q;
    } while(val > 0);
    // idx now points to the byte after the written characters
    // reverse the digits
    for(i64 i = sign; i < idx / 2; i += 1){
        u8 tmp = buf[i];
        buf[i] = buf[idx - 1 - i];
        buf[idx - 1 - i] = tmp;
    }
    // write the null terminator
    buf[idx] = '\0';
}
