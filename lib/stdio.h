#ifndef _SC16_STDIO_H
#define _SC16_STDIO_H

#include <stdarg.h>

#define EOF (-1)
#define BUFSIZ 256
#define FILENAME_MAX 32
#define FOPEN_MAX 8
#define L_tmpnam 16
#define TMP_MAX 25
#define _IOFBF 0
#define _IOLBF 1
#define _IONBF 2
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#define stdin ((FILE *)1)
#define stdout ((FILE *)2)
#define stderr ((FILE *)3)

typedef struct __sc16_FILE FILE;
typedef long fpos_t;

int fgetc(FILE *stream);
char *fgets(char *text, int count, FILE *stream);
char *gets(char *text);
int fputc(int character, FILE *stream);
int fputs(const char *text, FILE *stream);
int ungetc(int character, FILE *stream);
int getchar(void);
int putchar(int character);
int puts(const char *text);
int printf(const char *format, ...);
int fprintf(FILE *stream, const char *format, ...);
int vprintf(const char *format, va_list arguments);
int vfprintf(FILE *stream, const char *format, va_list arguments);
int scanf(const char *format, ...);
int fscanf(FILE *stream, const char *format, ...);
int vscanf(const char *format, va_list arguments);
int vfscanf(FILE *stream, const char *format, va_list arguments);
int fflush(FILE *stream);
FILE *fopen(const char *filename, const char *mode);
FILE *freopen(const char *filename, const char *mode, FILE *stream);
int fclose(FILE *stream);
size_t fread(void *pointer, size_t size, size_t count, FILE *stream);
size_t fwrite(const void *pointer, size_t size, size_t count, FILE *stream);
int fseek(FILE *stream, long offset, int origin);
long ftell(FILE *stream);
void rewind(FILE *stream);
int fgetpos(FILE *stream, fpos_t *position);
int fsetpos(FILE *stream, const fpos_t *position);
void clearerr(FILE *stream);
int feof(FILE *stream);
int ferror(FILE *stream);
void perror(const char *prefix);
int remove(const char *filename);
int rename(const char *old_name, const char *new_name);
FILE *tmpfile(void);
char *tmpnam(char *name);
void setbuf(FILE *stream, char *buffer);
int setvbuf(FILE *stream, char *buffer, int mode, size_t size);

#endif
