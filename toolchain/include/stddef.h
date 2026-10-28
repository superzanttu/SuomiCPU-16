#ifndef _SC16_STDDEF_H
#define _SC16_STDDEF_H

typedef unsigned long size_t;
typedef long ptrdiff_t;
typedef int wchar_t;

#define NULL 0
#define offsetof(type, member) ((size_t)&(((type *)0)->member))

#endif
