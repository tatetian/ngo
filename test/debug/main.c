#include <sys/mman.h>
#include <sys/syscall.h>
#include <stdio.h>
#include <stdint.h>
#include "test.h"

int test_libos_internal_benchmark() {
#define SYSCALL_DEBUG (364)
    int cmd_id = 1; // bench channel throughput
    syscall(SYSCALL_DEBUG, cmd_id);
    return 0;
}

// ============================================================================
// Test suite main
// ============================================================================
static test_case_t test_cases[] = {
    TEST_CASE(test_libos_internal_benchmark),
};

int main(int argc, const char *argv[]) {
    return test_suite_run(test_cases, ARRAY_SIZE(test_cases));
}
