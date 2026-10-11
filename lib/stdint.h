#ifndef _SC16_STDINT_H
#define _SC16_STDINT_H

#include <limits.h>
#include <stddef.h>

typedef signed char int8_t;
typedef unsigned char uint8_t;
typedef int int16_t;
typedef unsigned int uint16_t;
typedef long int32_t;
typedef unsigned long uint32_t;

typedef int int_least8_t;
typedef unsigned int uint_least8_t;
typedef int int_least16_t;
typedef unsigned int uint_least16_t;
typedef long int_least32_t;
typedef unsigned long uint_least32_t;

typedef int int_fast8_t;
typedef unsigned int uint_fast8_t;
typedef int int_fast16_t;
typedef unsigned int uint_fast16_t;
typedef long int_fast32_t;
typedef unsigned long uint_fast32_t;

typedef long intptr_t;
typedef unsigned long uintptr_t;
typedef long intmax_t;
typedef unsigned long uintmax_t;

#define INT8_MIN (-127 - 1)
#define INT8_MAX 127
#define UINT8_MAX 255U
#define INT16_MIN INT_MIN
#define INT16_MAX INT_MAX
#define UINT16_MAX UINT_MAX
#define INT32_MIN LONG_MIN
#define INT32_MAX LONG_MAX
#define UINT32_MAX ULONG_MAX

#define INT_LEAST8_MIN INT_MIN
#define INT_LEAST8_MAX INT_MAX
#define UINT_LEAST8_MAX UINT_MAX
#define INT_LEAST16_MIN INT_MIN
#define INT_LEAST16_MAX INT_MAX
#define UINT_LEAST16_MAX UINT_MAX
#define INT_LEAST32_MIN LONG_MIN
#define INT_LEAST32_MAX LONG_MAX
#define UINT_LEAST32_MAX ULONG_MAX

#define INT_FAST8_MIN INT_MIN
#define INT_FAST8_MAX INT_MAX
#define UINT_FAST8_MAX UINT_MAX
#define INT_FAST16_MIN INT_MIN
#define INT_FAST16_MAX INT_MAX
#define UINT_FAST16_MAX UINT_MAX
#define INT_FAST32_MIN LONG_MIN
#define INT_FAST32_MAX LONG_MAX
#define UINT_FAST32_MAX ULONG_MAX

#define INTPTR_MIN LONG_MIN
#define INTPTR_MAX LONG_MAX
#define UINTPTR_MAX ULONG_MAX
#define INTMAX_MIN LONG_MIN
#define INTMAX_MAX LONG_MAX
#define UINTMAX_MAX ULONG_MAX

#define PTRDIFF_MIN LONG_MIN
#define PTRDIFF_MAX LONG_MAX
#define SIG_ATOMIC_MIN INT_MIN
#define SIG_ATOMIC_MAX INT_MAX
#define SIZE_MAX ULONG_MAX
#define WCHAR_MIN (-32768)
#define WCHAR_MAX 32767
#define WINT_MIN 0U
#define WINT_MAX UINT_MAX

#define INT8_C(value) value
#define UINT8_C(value) value##U
#define INT16_C(value) value
#define UINT16_C(value) value##U
#define INT32_C(value) value##L
#define UINT32_C(value) value##UL
#define INTMAX_C(value) value##L
#define UINTMAX_C(value) value##UL

#endif
