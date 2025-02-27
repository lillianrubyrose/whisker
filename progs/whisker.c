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
            [acc] "+&r" (acc),
            [tmp] "+&r" (tmp),
            [lhs] "+&r" (lhs),
            [rhs] "+&r" (rhs)
    );
    return acc;
}

void mulwide(int64_t lhs, int64_t rhs, int64_t *restrict hi, int64_t *restrict lo) {
    // calculates a 64x64=128 bit mul by doing 4 32x32=64 bit multiplications
    // https://web.archive.org/web/20250222014334/https://stackoverflow.com/questions/26852435/reasonably-portable-way-to-get-top-64-bits-from-64x64-bit-multiply/26855440#26855440
    uint64_t a = lhs >> 32;
    uint64_t b = lhs & 0xFFFFFFFF;
    uint64_t c = rhs >> 32;
    uint64_t d = rhs & 0xFFFFFFFF;

    uint64_t ac = int_mul(a, c);
    uint64_t bc = int_mul(b, c);
    uint64_t ad = int_mul(a, d);
    uint64_t bd = int_mul(b, d);

    uint64_t mid = (bd >> 32) + (bc & 0xFFFFFFFF) + (ad & 0xFFFFFFFF);
    *hi = ac + (bc >> 32) + (ad >> 32) + (mid >> 32);
    *lo = (mid << 32) | (bd & 0xffffffff);
}

void div_10(int64_t lhs, int64_t *restrict quot, int64_t *restrict rem) {
    int64_t hi;
    int64_t lo;
    mulwide(lhs, 0x6666666666666667, &hi, &lo);
    int64_t q = hi >> 2;
    if(lhs < 0){
        q += 1;
    }
    *quot = q;
    // multiply by 10 to get the remainder
    *rem = lhs - (((q << 2) + q) << 1);
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
        buf[idx++] = '0' + (val % 10);
        val /= 10;
    } while(val > 0);
    // idx now points to the byte after the written characters
    // reverse the digits, skipping the negative sign if it exists
    rev_arr(buf + sign, idx - sign);
    // write the null terminator
    buf[idx] = '\0';
}
