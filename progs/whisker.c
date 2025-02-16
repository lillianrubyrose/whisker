#include "whisker.h"

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

void whisker_write_uart(const u8 *str) {
  i32 len = whisker_strlen((u8 *)str);
  for (i32 i = 0; i < len; i++) {
    *UART = str[i];
  }
}

// u8* int_to_string(i32 value) {
//     u8 buffer[256];

//     int i = 0;

//     do {
//         buffer[i++] = (value % 10) + '0';
//         value /= 10;
//     } while (value > 0);

//     buffer[i] = '\0';

//     for (int j = 0, k = i - 1; j < k; j++, k--) {
//         char tmp = buffer[j];
//         buffer[j] = buffer[k];
//         buffer[k] = tmp;
//     }

//     return buffer;
// }
