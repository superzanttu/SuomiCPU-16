#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <limits.h>

#define MMIO(type, address) (*(volatile type *)(uintptr_t)(address))
#define UART(value) MMIO(uint8_t, UINT32_C(0x10000000)) = (value)
#define SLOT_COUNT 4
#define CAT_RAW(a, b) a##b
#define CAT(a, b) CAT_RAW(a, b)

#if __riscv_xlen != 64 || __SIZEOF_LONG__ != 8 || __SIZEOF_POINTER__ != 8
#error wrong target layout
#endif
#if UINT32_MAX + 1U != 0x100000000UL || LONG_MAX != INT64_MAX
#error wrong preprocessor integer width
#endif

static const uint32_t values[SLOT_COUNT] = { [3] = 42, [0] = 17 };
static uint64_t CAT(total, _bytes);

static bool publish(size_t slot, const void *data)
{
    if (slot >= SLOT_COUNT || data == NULL) return false;
    const uint32_t *value = data;
    MMIO(uint32_t, UINT32_C(0x80000000) + slot * sizeof(uint32_t)) = *value;
    total_bytes += sizeof(*value);
    return true;
}

int main(void)
{
    if (CHAR_BIT != 8 || CHAR_MIN != 0 || UINT_MAX != UINT32_MAX) return 1;
    if (sizeof(uint8_t) != 1 || sizeof(uint16_t) != 2 || sizeof(uint32_t) != 4) return 2;
    if (sizeof(uint64_t) != 8 || sizeof(size_t) != 8 || sizeof(ptrdiff_t) != 8) return 3;
    if (!publish(0, &values[0]) || !publish(3, &values[3])) return 4;
    if (publish(SLOT_COUNT, values) || publish(1, NULL)) return 5;
    if ((ptrdiff_t)(&values[3] - values) != 3 || INT8_MIN != -128) return 6;
    MMIO(uint64_t, UINT32_C(0x80000010)) = total_bytes;
    UART('H');
    UART(values[0]);
    UART(values[3]);
    UART(total_bytes);
    UART('\n');
    return values[0] + values[3];
}
