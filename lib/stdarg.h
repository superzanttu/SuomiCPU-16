#ifndef _SC16_STDARG_H
#define _SC16_STDARG_H

#include <stddef.h>

typedef char *va_list;

#define va_start(ap, last) ((ap) = (char *)&(last) + sizeof(last))
#define va_arg(ap, type) (*(type *)((ap = (ap + sizeof(type))) - sizeof(type)))
#define va_end(ap) ((void)0)

#endif
