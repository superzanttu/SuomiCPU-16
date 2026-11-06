#include <stdint.h>
#include <stddef.h>

typedef int (*operation)(int, int);

static int add(int left, int right) { return left + right; }
static int subtract(int left, int right) { return left - right; }
static int multiply(int left, int right) { return left * right; }

static operation operations[] = { [2] = multiply, [0] = &add, subtract, [3] = 0 };
static unsigned int calls;

static int dispatch(operation callback, int left, int right)
{
    if (callback == 0) return -1;
    ++calls;
    return callback(left, right);
}

int main(void)
{
    operation selected = operations[0];
    int total = dispatch(selected, 17, 25);
    selected = 1 ? operations[1] : 0;
    total += dispatch(selected, 17, 5);
    total += dispatch(operations[2], 3, 4);
    if (dispatch(operations[3], 1, 2) != -1) return 1;
    operations[1] = add;
    total += (*operations[1])(1, 2);
    *(volatile uint32_t *)UINT32_C(0x80000000) = total;
    *(volatile uint32_t *)UINT32_C(0x80000004) = calls;
    *(volatile uintptr_t *)UINT32_C(0x80000008) = (uintptr_t)operations;
    *(volatile unsigned char *)UINT32_C(0x10000000) = 'D';
    *(volatile unsigned char *)UINT32_C(0x10000000) = total;
    *(volatile unsigned char *)UINT32_C(0x10000000) = calls;
    *(volatile unsigned char *)UINT32_C(0x10000000) = '\n';
    return total;
}
