#include "whisker.h"

#define DRAM_BASE 0x80000000

volatile uint32_t* atomic_word = (volatile uint32_t*)(DRAM_BASE + 0x2048);

void print_string(const char* str) {
    whisker_write_uart(str);
}

void print_int(int64_t value) {
    char buffer[21];
    int_to_string(value, buffer);
    whisker_write_uart(buffer);
}

void print_header(const char* test_name) {
    print_string("\n=== Testing ");
    print_string(test_name);
    print_string(" ===\n");
}

void print_summary(const char* operation, uint32_t original, uint32_t result, uint32_t final) {
    print_string("  Operation: ");
    print_string(operation);
    print_string("\n  Original value: ");
    print_int(original);
    print_string("\n  Returned value: ");
    print_int(result);
    print_string("\n  Final memory value: ");
    print_int(final);
    print_string("\n  Result explanation: ");
}

// Test AMOSWAP.W
void test_amoswap() {
    print_header("AMOSWAP.W");
    print_string("Atomically swaps a register value with a memory value\n");

    uint32_t initial = 100;
    uint32_t swap_val = 200;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic swap
    __asm__ volatile(
        "amoswap.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(swap_val)
        : "memory"
    );

    print_summary("AMOSWAP.W", initial, result, *atomic_word);
    print_string("Swapped value 200 with memory, returning original value 100\n");
}

// Test AMOADD.W
void test_amoadd() {
    print_header("AMOADD.W");
    print_string("Atomically adds a register value to a memory value\n");

    uint32_t initial = 100;
    uint32_t add_val = 50;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic add
    __asm__ volatile(
        "amoadd.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(add_val)
        : "memory"
    );

    print_summary("AMOADD.W", initial, result, *atomic_word);
    print_string("Added 50 to memory value 100, resulting in 150, returned original value\n");
}

// Test AMOXOR.W
void test_amoxor() {
    print_header("AMOXOR.W");
    print_string("Atomically performs bitwise XOR between register and memory\n");

    uint32_t initial = 100;  // 0x64
    uint32_t xor_val = 110;  // 0x6E
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic XOR
    __asm__ volatile(
        "amoxor.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(xor_val)
        : "memory"
    );

    print_summary("AMOXOR.W", initial, result, *atomic_word);
    print_string("XOR of 0x64 (100) and 0x6E (110) is 0x0A (10)\n");
}

// Test AMOAND.W
void test_amoand() {
    print_header("AMOAND.W");
    print_string("Atomically performs bitwise AND between register and memory\n");

    uint32_t initial = 100;  // 0x64
    uint32_t and_val = 110;  // 0x6E
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic AND
    __asm__ volatile(
        "amoand.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(and_val)
        : "memory"
    );

    print_summary("AMOAND.W", initial, result, *atomic_word);
    print_string("AND of 0x64 (100) and 0x6E (110) is 0x64 (100)\n");
}

// Test AMOOR.W
void test_amoor() {
    print_header("AMOOR.W");
    print_string("Atomically performs bitwise OR between register and memory\n");

    uint32_t initial = 110;  // 0x6E
    uint32_t or_val = 1;     // 0x01
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic OR
    __asm__ volatile(
        "amoor.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(or_val)
        : "memory"
    );

    print_summary("AMOOR.W", initial, result, *atomic_word);
    print_string("OR of 0x6E (110) and 0x01 (1) is 0x6F (111)\n");
}

// Test AMOMIN.W
void test_amomin() {
    print_header("AMOMIN.W");
    print_string("Atomically computes minimum of register and memory (signed)\n");

    uint32_t initial = 100;
    uint32_t min_val = 50;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MIN (signed)
    __asm__ volatile(
        "amomin.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(min_val)
        : "memory"
    );

    print_summary("AMOMIN.W (case 1)", initial, result, *atomic_word);
    print_string("Minimum of 100 and 50 is 50, memory updated to minimum value\n");

    // Now test with a value that won't be the minimum
    initial = 50;
    min_val = 100;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MIN (signed)
    __asm__ volatile(
        "amomin.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(min_val)
        : "memory"
    );

    print_summary("AMOMIN.W (case 2)", initial, result, *atomic_word);
    print_string("Minimum of 50 and 100 is 50, memory unchanged\n");
}

// Test AMOMAX.W
void test_amomax() {
    print_header("AMOMAX.W");
    print_string("Atomically computes maximum of register and memory (signed)\n");

    uint32_t initial = 100;
    int32_t max_val = 200;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MAX (signed)
    __asm__ volatile(
        "amomax.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(max_val)
        : "memory"
    );

    print_summary("AMOMAX.W (case 1)", initial, result, *atomic_word);
    print_string("Maximum of 100 and 200 is 200, memory updated to maximum value\n");

    // Now test with a value that won't be the maximum
    initial = 300;
    max_val = 200;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MAX (signed)
    __asm__ volatile(
        "amomax.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(max_val)
        : "memory"
    );

    print_summary("AMOMAX.W (case 2)", initial, result, *atomic_word);
    print_string("Maximum of 300 and 200 is 300, memory unchanged\n");
}

// Test AMOMINU.W
void test_amominu() {
    print_header("AMOMINU.W");
    print_string("Atomically computes minimum of register and memory (unsigned)\n");

    uint32_t initial = 150;
    uint32_t min_val = 100;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MIN (unsigned)
    __asm__ volatile(
        "amominu.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(min_val)
        : "memory"
    );

    print_summary("AMOMINU.W", initial, result, *atomic_word);
    print_string("Unsigned minimum of 150 and 100 is 100, memory updated to minimum value\n");
}

// Test AMOMAXU.W
void test_amomaxu() {
    print_header("AMOMAXU.W");
    print_string("Atomically computes maximum of register and memory (unsigned)\n");

    uint32_t initial = 100;
    uint32_t max_val = 200;
    uint32_t result;

    // Set initial value
    *atomic_word = initial;

    // Perform atomic MAX (unsigned)
    __asm__ volatile(
        "amomaxu.w %0, %2, (%1)"
        : "=r"(result)
        : "r"(atomic_word), "r"(max_val)
        : "memory"
    );

    print_summary("AMOMAXU.W", initial, result, *atomic_word);
    print_string("Unsigned maximum of 100 and 200 is 200, memory updated to maximum value\n");
}

// Test LR/SC sequence
bool test_lr_sc() {
    print_header("LR/SC (Load-Reserved/Store-Conditional)");
    print_string("Tests atomic memory update using load-reserved and store-conditional\n");

    uint32_t initial = 69;
    uint32_t new_val = 420;
    uint32_t loaded;
    uint32_t success;

    // Set initial value
    *atomic_word = initial;

    // Load-reserved
    __asm__ volatile(
        "lr.w %0, (%1)"
        : "=r"(loaded)
        : "r"(atomic_word)
        : "memory"
    );

    // Check loaded value
    if (loaded != initial) {
        print_string("  ERROR: LR.W loaded incorrect value\n");
        return false;
    }

    // Store-conditional (should succeed)
    __asm__ volatile(
        "sc.w %0, %2, (%1)"
        : "=r"(success)
        : "r"(atomic_word), "r"(new_val)
        : "memory"
    );

    print_string("  LR operation loaded value: ");
    print_int(loaded);
    print_string("\n  SC operation success flag (0=success): ");
    print_int(success);
    print_string("\n  Final memory value: ");
    print_int(*atomic_word);
    print_string("\n  Explanation: ");

    if (success == 0 && *atomic_word == new_val) {
        print_string("Successfully performed atomic update from 69 to 420\n");
        print_string("  SC returned 0 indicating successful conditional store\n");
        return true;
    } else {
        print_string("Failed to perform atomic update\n");
        if (success != 0) {
            print_string("  SC returned non-zero indicating reservation was lost\n");
        }
        if (*atomic_word != new_val) {
            print_string("  Memory value was not updated as expected\n");
        }
        return false;
    }
}

int main() {
    print_string("==================================================\n");
    print_string("Starting RV32A atomic instruction tests\n");
    print_string("==================================================\n");

    test_amoswap();
    test_amoadd();
    test_amoxor();
    test_amoand();
    test_amoor();
    test_amomin();
    test_amomax();
    test_amominu();
    test_amomaxu();

    bool lr_sc_success = test_lr_sc();

    print_string("\n==================================================\n");
    print_string("Test Summary\n");
    print_string("==================================================\n");

    if (lr_sc_success) {
        print_string("LR/SC test: PASSED\n");
    } else {
        print_string("LR/SC test: FAILED\n");
    }

    print_string("All atomic instruction tests completed.\n");

    return 0;
}
