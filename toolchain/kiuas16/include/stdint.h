#ifndef _KIUAS16_STDINT_H
#define _KIUAS16_STDINT_H

#include <limits.h>

typedef signed char int8_t;
typedef unsigned char uint8_t;
typedef short int16_t;
typedef unsigned short uint16_t;
typedef int int32_t;
typedef unsigned int uint32_t;
typedef long int64_t;
typedef unsigned long uint64_t;

typedef int8_t int_least8_t;
typedef uint8_t uint_least8_t;
typedef int16_t int_least16_t;
typedef uint16_t uint_least16_t;
typedef int32_t int_least32_t;
typedef uint32_t uint_least32_t;
typedef int64_t int_least64_t;
typedef uint64_t uint_least64_t;

typedef long int_fast8_t;
typedef unsigned long uint_fast8_t;
typedef long int_fast16_t;
typedef unsigned long uint_fast16_t;
typedef long int_fast32_t;
typedef unsigned long uint_fast32_t;
typedef long int_fast64_t;
typedef unsigned long uint_fast64_t;

typedef long intptr_t;
typedef unsigned long uintptr_t;
typedef long intmax_t;
typedef unsigned long uintmax_t;

#define INT8_MIN SCHAR_MIN
#define INT8_MAX SCHAR_MAX
#define UINT8_MAX UCHAR_MAX
#define INT16_MIN SHRT_MIN
#define INT16_MAX SHRT_MAX
#define UINT16_MAX USHRT_MAX
#define INT32_MIN INT_MIN
#define INT32_MAX INT_MAX
#define UINT32_MAX UINT_MAX
#define INT64_MIN LONG_MIN
#define INT64_MAX LONG_MAX
#define UINT64_MAX ULONG_MAX

#define INT_LEAST8_MIN INT8_MIN
#define INT_LEAST8_MAX INT8_MAX
#define UINT_LEAST8_MAX UINT8_MAX
#define INT_LEAST16_MIN INT16_MIN
#define INT_LEAST16_MAX INT16_MAX
#define UINT_LEAST16_MAX UINT16_MAX
#define INT_LEAST32_MIN INT32_MIN
#define INT_LEAST32_MAX INT32_MAX
#define UINT_LEAST32_MAX UINT32_MAX
#define INT_LEAST64_MIN INT64_MIN
#define INT_LEAST64_MAX INT64_MAX
#define UINT_LEAST64_MAX UINT64_MAX

#define INT_FAST8_MIN LONG_MIN
#define INT_FAST8_MAX LONG_MAX
#define UINT_FAST8_MAX ULONG_MAX
#define INT_FAST16_MIN LONG_MIN
#define INT_FAST16_MAX LONG_MAX
#define UINT_FAST16_MAX ULONG_MAX
#define INT_FAST32_MIN LONG_MIN
#define INT_FAST32_MAX LONG_MAX
#define UINT_FAST32_MAX ULONG_MAX
#define INT_FAST64_MIN LONG_MIN
#define INT_FAST64_MAX LONG_MAX
#define UINT_FAST64_MAX ULONG_MAX

#define INTPTR_MIN LONG_MIN
#define INTPTR_MAX LONG_MAX
#define UINTPTR_MAX ULONG_MAX
#define INTMAX_MIN LONG_MIN
#define INTMAX_MAX LONG_MAX
#define UINTMAX_MAX ULONG_MAX
#define PTRDIFF_MIN LONG_MIN
#define PTRDIFF_MAX LONG_MAX
#define SIZE_MAX ULONG_MAX

#define INT8_C(value) value
#define UINT8_C(value) value
#define INT16_C(value) value
#define UINT16_C(value) value
#define INT32_C(value) value
#define UINT32_C(value) value##U
#define INT64_C(value) value##L
#define UINT64_C(value) value##UL
#define INTMAX_C(value) value##L
#define UINTMAX_C(value) value##UL

#endif
