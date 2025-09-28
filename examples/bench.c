void integer_ops(int loops) {
    unsigned long long a = 1234567890123456789ULL;
    unsigned long long b = 9876543210987654321ULL;
    unsigned long long result;

    for (int i = 0; i < loops; i++) {
        result = a + b;
        result = result - a;
        result = a * 2;
        result = b / 3;
        result = a % 7;
    }
}

void bitwise_ops(int loops) {
    unsigned long long a = 0xAAAAAAAAAAAAAAAAULL;
    unsigned long long b = 0x5555555555555555ULL;
    unsigned long long result;

    for (int i = 0; i < loops; i++) {
        result = a & b;
        result = a | b;
        result = a ^ b;
        result = ~a;
        result = a << 3;
        result = b >> 5;
    }
}

#define SIEVE_SIZE 1024 * 16
char sieve[SIEVE_SIZE];

void sieve_impl() {
    for (int i = 2; i < SIEVE_SIZE; ++i) {
        sieve[i] = 1;
    }

    for (int p = 2; p * p < SIEVE_SIZE; ++p) {
        if (sieve[p]) {
            for (int i = p * p; i < SIEVE_SIZE; i += p) {
                sieve[i] = 0;
            }
        }
    }
}

int main() {
    integer_ops(200000);
    bitwise_ops(200000);
    sieve_impl();
}
