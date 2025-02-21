#include "whisker.h"

int64_t int_mul(int64_t lhs, int64_t rhs) {
    // considers each bit in rhs, and if it is set, adds
    // lhs * bit_value to the accumulator
    // this works for signed values since they are represented with 2s compliment.
    // however this is implemented in assembly to get those semantics
    // and to improve speed of the algorithm even without optimizations.
    int64_t tmp = 0;
    int64_t acc = 0;
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

// dont pass in int64_t::MIN
void int_div(int64_t lhs, int64_t rhs, int64_t* quotient, int64_t* remainder){
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
    int64_t quot = 0;
    int64_t rem = lhs;
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

int32_t whisker_strlen(const char *str) {
  int32_t len = 0;

  for (;;) {
    if (str[len] == '\0') {
      break;
    }
    len++;
  }

  return len;
}

void whisker_write_uart(const char *str) {
  int32_t len = whisker_strlen(str);
  for (int32_t i = 0; i < len; i++) {
    *UART = str[i];
  }
}

void rev_arr(char arr[], int64_t len){
    for(int64_t i = 0; i < len / 2; i += 1){
        char tmp = arr[i];
        arr[i] = arr[len - 1 - i];
        arr[len - 1 - i] = tmp;
    }
}

void int_to_string(int64_t val, char buf[21]){
    int64_t idx = 0;
    int64_t sign = val < 0;
    if(sign){
        buf[idx++] = '-';
        val = -val;
    }
    // build up the array from least significant digit to most
    do {
        int64_t q, r;
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
