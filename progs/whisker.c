#include "whisker.h"

i64 int_mul(i64 lhs, i64 rhs) {
    // considers each bit in rhs, and if it is set, adds
    // lhs * bit_value to the accumulator
    // this works for signed values since they are represented with 2s compliment.
    // however this is implemented in assembly to get those semantics
    // and to improve speed of the algorithm even without optimizations.
    i64 tmp = 0;
    i64 acc = 0;
    __asm__(
        "2: andi %[tmp], %[rhs], 1 \n\t"
        "beqz %[tmp], 3f \n\t" // no bit set
        "add %[acc], %[acc], %[lhs] \n\t"
        "3: srli %[rhs], %[rhs], 1 \n\t" // move to next bit
        "slli %[lhs], %[lhs], 1 \n\t" // increase the value to add for that bit
        "bnez %[rhs], 2b \n\t"
        :
            [acc] "=&r" (acc), // written before reading next loop, must be early clobber
            [tmp] "=&r" (tmp), // tmp are always early clobber
            [lhs] "+r" (lhs),  // read and written
            [rhs] "+r" (rhs)   // read and written
    );
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

void rev_arr(char arr[], i64 len){
    for(i64 i = 0; i < len / 2; i += 1){
        u8 tmp = arr[i];
        arr[i] = arr[len - 1 - i];
        arr[len - 1 - i] = tmp;
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
    // reverse the digits, skipping the negative sign if it exists
    rev_arr(buf + sign, idx - sign);
    // write the null terminator
    buf[idx] = '\0';
}
