#ifndef _SC16_STDIO_H
#define _SC16_STDIO_H

#include <stdarg.h>

#define EOF (-1)
#define BUFSIZ 256
#define stdin ((FILE *)1)
#define stdout ((FILE *)2)
#define stderr ((FILE *)3)

typedef struct __sc16_FILE FILE;

int fgetc(FILE *stream);
int fputc(int character, FILE *stream);
int fputs(const char *text, FILE *stream);
int getchar(void);
int putchar(int character);
int puts(const char *text);
int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int vprintf(const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);

#endif
