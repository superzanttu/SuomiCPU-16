#include <stdio.h>
#include <errno.h>
#include <limits.h>
#include <stdlib.h>
#include <ctype.h>

#define CONSOLE_DATA (*(volatile unsigned char *)0x43200UL)
#define CONSOLE_STATUS (*(volatile unsigned char *)0x43201UL)

static int __sc16_console_error;
static int __sc16_pushback_character = EOF;

int fgetc(FILE *stream)
{
    if (stream != stdin) {
        __sc16_console_error = 1;
        errno = EBADF;
        return EOF;
    }
    if (__sc16_pushback_character != EOF) {
        int character;

        character = __sc16_pushback_character;
        __sc16_pushback_character = EOF;
        return character;
    }
    while (!(CONSOLE_STATUS & 1))
        ;
    return CONSOLE_DATA;
}

char *fgets(char *text, int count, FILE *stream)
{
    int character;
    int index;

    if (text == NULL || count <= 0) {
        errno = EINVAL;
        return NULL;
    }
    if (count == 1) {
        text[0] = '\0';
        return text;
    }
    index = 0;
    while (index < count - 1) {
        character = fgetc(stream);
        if (character == EOF) {
            if (index == 0)
                return NULL;
            break;
        }
        text[index++] = (char)character;
        if (character == '\n')
            break;
    }
    text[index] = '\0';
    return text;
}

char *gets(char *text)
{
    int character;
    int index;

    if (text == NULL) {
        errno = EINVAL;
        return NULL;
    }
    index = 0;
    while ((character = fgetc(stdin)) != EOF && character != '\n')
        text[index++] = (char)character;
    if (character == EOF && index == 0)
        return NULL;
    text[index] = '\0';
    return text;
}

int fputc(int character, FILE *stream)
{
    if (stream != stdout && stream != stderr) {
        __sc16_console_error = 1;
        errno = EBADF;
        return EOF;
    }
    while (!(CONSOLE_STATUS & 2))
        ;
    CONSOLE_DATA = (unsigned char)character;
    return (unsigned char)character;
}

int fputs(const char *text, FILE *stream)
{
    int count;

    count = 0;
    while (*text) {
        if (fputc((unsigned char)*text, stream) == EOF)
            return EOF;
        ++text;
        ++count;
    }
    return count;
}

int getchar(void)
{
    return fgetc(stdin);
}

int putchar(int character)
{
    return fputc(character, stdout);
}

int puts(const char *text)
{
    int count;

    count = fputs(text, stdout);
    if (count == EOF || putchar('\n') == EOF)
        return EOF;
    return count + 1;
}

static int __stdio_text(FILE *stream, const char *text, int length,
                        int width, int left, int pad)
{
    int count;
    int index;

    count = 0;
    if (!left) {
        for (index = length; index < width; ++index) {
            if (fputc(pad, stream) == EOF)
                return EOF;
            ++count;
        }
    }
    for (index = 0; index < length; ++index) {
        if (fputc((unsigned char)text[index], stream) == EOF)
            return EOF;
        ++count;
    }
    if (left) {
        for (index = length; index < width; ++index) {
            if (fputc(' ', stream) == EOF)
                return EOF;
            ++count;
        }
    }
    return count;
}

static int __stdio_number(FILE *stream, unsigned long value, int negative,
                          unsigned int base, int uppercase, int width,
                          int precision, int left, int zero, int alternate,
                          int show_sign, int blank_sign)
{
    char digits[33];
    char field[40];
    const char *alphabet;
    int digit_count;
    int field_count;
    int zeros;
    int padding;
    int index;
    int count;

    alphabet = uppercase ? "0123456789ABCDEF" : "0123456789abcdef";
    digit_count = 0;
    if (value != 0 || precision != 0) {
        do {
            digits[digit_count++] = alphabet[value % base];
            value /= base;
        } while (value != 0);
    }

    field_count = 0;
    if (negative)
        field[field_count++] = '-';
    else if (show_sign)
        field[field_count++] = '+';
    else if (blank_sign)
        field[field_count++] = ' ';
    if (alternate && base == 16) {
        field[field_count++] = '0';
        field[field_count++] = uppercase ? 'X' : 'x';
    }
    if (alternate && base == 8 &&
        (digit_count == 0 || digits[digit_count - 1] != '0'))
        field[field_count++] = '0';

    zeros = precision > digit_count ? precision - digit_count : 0;
    padding = width - field_count - zeros - digit_count;
    if (padding < 0)
        padding = 0;
    count = 0;
    if (!left && !(zero && precision < 0)) {
        for (index = 0; index < padding; ++index) {
            if (fputc(' ', stream) == EOF)
                return EOF;
            ++count;
        }
    }
    for (index = 0; index < field_count; ++index) {
        if (fputc(field[index], stream) == EOF)
            return EOF;
        ++count;
    }
    if (!left && zero && precision < 0) {
        for (index = 0; index < padding; ++index) {
            if (fputc('0', stream) == EOF)
                return EOF;
            ++count;
        }
    }
    for (index = 0; index < zeros; ++index) {
        if (fputc('0', stream) == EOF)
            return EOF;
        ++count;
    }
    while (digit_count > 0) {
        if (fputc(digits[--digit_count], stream) == EOF)
            return EOF;
        ++count;
    }
    if (left) {
        for (index = 0; index < padding; ++index) {
            if (fputc(' ', stream) == EOF)
                return EOF;
            ++count;
        }
    }
    return count;
}

int vfprintf(FILE *stream, const char *format, va_list arguments)
{
    int count;

    count = 0;
    while (*format) {
        int width;
        int precision;
        int left;
        int zero;
        int alternate;
        int show_sign;
        int blank_sign;
        int long_value;
        int character;
        int result;
        char conversion;

        if (*format != '%') {
            if (fputc((unsigned char)*format++, stream) == EOF)
                return EOF;
            ++count;
            continue;
        }
        ++format;
        if (*format == '%') {
            if (fputc('%', stream) == EOF)
                return EOF;
            ++format;
            ++count;
            continue;
        }

        left = 0;
        zero = 0;
        alternate = 0;
        show_sign = 0;
        blank_sign = 0;
        while (*format == '-' || *format == '+' || *format == ' ' ||
               *format == '#' || *format == '0') {
            if (*format == '-')
                left = 1;
            else if (*format == '+')
                show_sign = 1;
            else if (*format == ' ')
                blank_sign = 1;
            else if (*format == '#')
                alternate = 1;
            else
                zero = 1;
            ++format;
        }

        width = 0;
        if (*format == '*') {
            width = va_arg(arguments, int);
            ++format;
            if (width < 0) {
                left = 1;
                width = -width;
            }
        } else {
            while (*format >= '0' && *format <= '9') {
                width = width * 10 + (*format++ - '0');
                if (width > 255)
                    return EOF;
            }
        }

        precision = -1;
        if (*format == '.') {
            ++format;
            precision = 0;
            if (*format == '*') {
                precision = va_arg(arguments, int);
                ++format;
                if (precision < 0)
                    precision = -1;
            } else {
                while (*format >= '0' && *format <= '9') {
                    precision = precision * 10 + (*format++ - '0');
                    if (precision > 255)
                        return EOF;
                }
            }
        }

        long_value = 0;
        if (*format == 'l') {
            long_value = 1;
            ++format;
        } else if (*format == 'h') {
            ++format;
        }
        conversion = *format;
        if (!conversion)
            return EOF;
        ++format;

        if (conversion == 's') {
            const char *text;
            int length;

            text = va_arg(arguments, char *);
            if (!text)
                text = "(null)";
            length = 0;
            while (text[length] && (precision < 0 || length < precision))
                ++length;
            result = __stdio_text(stream, text, length, width, left, ' ');
        } else if (conversion == 'c') {
            char text[1];

            text[0] = (char)va_arg(arguments, int);
            result = __stdio_text(stream, text, 1, width, left, ' ');
        } else if (conversion == 'd' || conversion == 'i') {
            long signed_value;
            unsigned long magnitude;
            int negative;

            signed_value = long_value ? va_arg(arguments, long) :
                                       (long)va_arg(arguments, int);
            negative = signed_value < 0;
            magnitude = (unsigned long)(negative ? -signed_value : signed_value);
            result = __stdio_number(stream, magnitude, negative, 10, 0, width,
                                    precision, left, zero, 0, show_sign,
                                    blank_sign && !show_sign);
        } else if (conversion == 'u' || conversion == 'o' ||
                   conversion == 'x' || conversion == 'X') {
            unsigned long value;
            unsigned int base;

            if (long_value)
                value = va_arg(arguments, unsigned long);
            else
                value = (unsigned int)va_arg(arguments, unsigned int);
            base = conversion == 'o' ? 8 :
                   (conversion == 'x' || conversion == 'X' ? 16 : 10);
            result = __stdio_number(stream, value, 0, base, conversion == 'X',
                                    width, precision, left, zero, alternate, 0, 0);
        } else if (conversion == 'p') {
            unsigned long value;

            value = (unsigned long)va_arg(arguments, void *);
            result = __stdio_number(stream, value, 0, 16, 0, width, precision,
                                    left, zero, 1, 0, 0);
        } else if (conversion == 'n') {
            if (long_value)
                *va_arg(arguments, long *) = count;
            else
                *va_arg(arguments, int *) = count;
            result = 0;
        } else {
            return EOF;
        }
        if (result == EOF)
            return EOF;
        count += result;
    }
    return count;
}

int vprintf(const char *format, va_list arguments)
{
    return vfprintf(stdout, format, arguments);
}

int ungetc(int character, FILE *stream)
{
    if (stream != stdin || character == EOF ||
        __sc16_pushback_character != EOF) {
        if (stream != stdin)
            errno = EBADF;
        return EOF;
    }
    __sc16_pushback_character = (unsigned char)character;
    return __sc16_pushback_character;
}

int fprintf(FILE *stream, const char *format, ...)
{
    va_list arguments;
    int result;

    va_start(arguments, format);
    result = vfprintf(stream, format, arguments);
    va_end(arguments);
    return result;
}

int printf(const char *format, ...)
{
    va_list arguments;
    int result;

    va_start(arguments, format);
    result = vfprintf(stdout, format, arguments);
    va_end(arguments);
    return result;
}

int fflush(FILE *stream)
{
    if (stream == NULL || stream == stdout || stream == stderr)
        return 0;
    errno = EBADF;
    __sc16_console_error = 1;
    return EOF;
}

FILE *fopen(const char *filename, const char *mode)
{
    (void)filename;
    (void)mode;
    errno = ENOSYS;
    return NULL;
}

FILE *freopen(const char *filename, const char *mode, FILE *stream)
{
    (void)filename;
    (void)mode;
    (void)stream;
    errno = ENOSYS;
    return NULL;
}

int fclose(FILE *stream)
{
    if (stream == stdin || stream == stdout || stream == stderr) {
        errno = EBADF;
        return EOF;
    }
    errno = EBADF;
    return EOF;
}

size_t fread(void *pointer, size_t size, size_t count, FILE *stream)
{
    unsigned char *bytes;
    size_t total;
    size_t index;
    int character;

    if (size == 0 || count == 0)
        return 0;
    if (pointer == NULL || count > ULONG_MAX / size) {
        errno = EINVAL;
        return 0;
    }
    if (stream != stdin) {
        errno = EBADF;
        __sc16_console_error = 1;
        return 0;
    }
    total = size * count;
    bytes = (unsigned char *)pointer;
    for (index = 0; index < total; ++index) {
        character = fgetc(stream);
        if (character == EOF)
            break;
        bytes[index] = (unsigned char)character;
    }
    return index / size;
}

size_t fwrite(const void *pointer, size_t size, size_t count, FILE *stream)
{
    const unsigned char *bytes;
    size_t total;
    size_t index;

    if (size == 0 || count == 0)
        return 0;
    if (pointer == NULL || count > ULONG_MAX / size) {
        errno = EINVAL;
        return 0;
    }
    if (stream != stdout && stream != stderr) {
        errno = EBADF;
        __sc16_console_error = 1;
        return 0;
    }
    total = size * count;
    bytes = (const unsigned char *)pointer;
    for (index = 0; index < total; ++index) {
        if (fputc(bytes[index], stream) == EOF)
            break;
    }
    return index / size;
}

int fseek(FILE *stream, long offset, int origin)
{
    (void)offset;
    (void)origin;
    if (stream == stdin || stream == stdout || stream == stderr)
        errno = ESPIPE;
    else
        errno = EBADF;
    return -1;
}

long ftell(FILE *stream)
{
    if (stream == stdin || stream == stdout || stream == stderr)
        errno = ESPIPE;
    else
        errno = EBADF;
    return -1L;
}

void rewind(FILE *stream)
{
    (void)fseek(stream, 0L, SEEK_SET);
    clearerr(stream);
}

int fgetpos(FILE *stream, fpos_t *position)
{
    (void)position;
    (void)ftell(stream);
    return -1;
}

int fsetpos(FILE *stream, const fpos_t *position)
{
    if (position == NULL) {
        errno = EINVAL;
        return -1;
    }
    return fseek(stream, *position, SEEK_SET);
}

void clearerr(FILE *stream)
{
    if (stream == stdin || stream == stdout || stream == stderr)
        __sc16_console_error = 0;
}

int feof(FILE *stream)
{
    (void)stream;
    return 0;
}

int ferror(FILE *stream)
{
    if (stream != stdin && stream != stdout && stream != stderr)
        return 1;
    return __sc16_console_error;
}

void perror(const char *prefix)
{
    if (prefix != NULL && *prefix != '\0') {
        (void)fputs(prefix, stderr);
        (void)fputs(": ", stderr);
    }
    (void)fputs(errno == ENOSYS ? "operation unavailable\n" :
                errno == EBADF ? "bad stream\n" :
                errno == EINVAL ? "invalid argument\n" :
                errno == ENOMEM ? "out of memory\n" :
                errno == ESPIPE ? "stream is not seekable\n" :
                "I/O error\n", stderr);
}

int remove(const char *filename)
{
    (void)filename;
    errno = ENOSYS;
    return -1;
}

int rename(const char *old_name, const char *new_name)
{
    (void)old_name;
    (void)new_name;
    errno = ENOSYS;
    return -1;
}

FILE *tmpfile(void)
{
    errno = ENOSYS;
    return NULL;
}

char *tmpnam(char *name)
{
    (void)name;
    errno = ENOSYS;
    return NULL;
}

void setbuf(FILE *stream, char *buffer)
{
    (void)setvbuf(stream, buffer,
                  buffer == NULL ? _IONBF : _IOFBF,
                  buffer == NULL ? 0UL : BUFSIZ);
}

int setvbuf(FILE *stream, char *buffer, int mode, size_t size)
{
    (void)buffer;
    (void)size;
    if (stream != stdin && stream != stdout && stream != stderr) {
        errno = EBADF;
        return -1;
    }
    if (mode != _IOFBF && mode != _IOLBF && mode != _IONBF) {
        errno = EINVAL;
        return -1;
    }
    if (mode != _IONBF) {
        errno = ENOSYS;
        return -1;
    }
    return 0;
}

int scanf(const char *format, ...)
{
    va_list arguments;
    int result;

    va_start(arguments, format);
    result = vfscanf(stdin, format, arguments);
    va_end(arguments);
    return result;
}

int fscanf(FILE *stream, const char *format, ...)
{
    va_list arguments;
    int result;

    va_start(arguments, format);
    result = vfscanf(stream, format, arguments);
    va_end(arguments);
    return result;
}

int vscanf(const char *format, va_list arguments)
{
    return vfscanf(stdin, format, arguments);
}

static int __stdio_scan_token(FILE *stream, char *text, int width,
                              int skip_space, int *consumed)
{
    int character;
    int length;

    if (skip_space) {
        do {
            character = fgetc(stream);
            if (character != EOF)
                ++*consumed;
        } while (character != EOF && isspace(character));
    } else {
        character = fgetc(stream);
        if (character != EOF)
            ++*consumed;
    }
    if (character == EOF)
        return 0;
    length = 0;
    while (character != EOF && length < width && length < 63 &&
           !isspace(character)) {
        text[length++] = (char)character;
        if (length == width || length == 63)
            break;
        character = fgetc(stream);
        if (character != EOF)
            ++*consumed;
    }
    if (character != EOF && length < width && length < 63 &&
        isspace(character)) {
        (void)ungetc(character, stream);
        --*consumed;
    }
    text[length] = '\0';
    return length != 0;
}

int vfscanf(FILE *stream, const char *format, va_list arguments)
{
    int assignments;
    int consumed;

    if (stream != stdin) {
        errno = EBADF;
        return EOF;
    }
    if (format == NULL) {
        errno = EINVAL;
        return EOF;
    }
    assignments = 0;
    consumed = 0;
    while (*format != '\0') {
        int suppress;
        int width;
        int long_value;
        char conversion;
        char token[64];
        int length;

        if (isspace((unsigned char)*format)) {
            while (isspace((unsigned char)*format))
                ++format;
            do {
                conversion = (char)fgetc(stream);
                ++consumed;
            } while (conversion != EOF && isspace((unsigned char)conversion));
            if (conversion != EOF) {
                (void)ungetc(conversion, stream);
                --consumed;
            }
            continue;
        }
        if (*format != '%') {
            int character;

            character = fgetc(stream);
            ++consumed;
            if (character != (unsigned char)*format)
                return assignments;
            ++format;
            continue;
        }
        ++format;
        if (*format == '%') {
            int character;

            character = fgetc(stream);
            ++consumed;
            if (character != '%')
                return assignments;
            ++format;
            continue;
        }
        suppress = 0;
        if (*format == '*') {
            suppress = 1;
            ++format;
        }
        width = 0;
        while (*format >= '0' && *format <= '9') {
            width = width * 10 + (*format++ - '0');
            if (width > 255) {
                errno = EINVAL;
                return assignments;
            }
        }
        if (width == 0)
            width = 63;
        long_value = 0;
        if (*format == 'h') {
            ++format;
        } else if (*format == 'l' || *format == 'L') {
            long_value = 1;
            ++format;
        }
        conversion = *format++;
        if (conversion == '\0') {
            errno = EINVAL;
            return assignments;
        }
        if (conversion == 'c') {
            char *destination;
            int index;

            if (!suppress)
                destination = va_arg(arguments, char *);
            for (index = 0; index < width; ++index) {
                int character;

                character = fgetc(stream);
                if (character == EOF)
                    return assignments == 0 ? EOF : assignments;
                ++consumed;
                if (!suppress)
                    destination[index] = (char)character;
            }
            if (!suppress)
                ++assignments;
            continue;
        }
        if (conversion == 's' || conversion == 'd' || conversion == 'i' ||
            conversion == 'u' || conversion == 'o' || conversion == 'x' ||
            conversion == 'X' || conversion == 'p' || conversion == 'f' ||
            conversion == 'e' || conversion == 'E' || conversion == 'g' ||
            conversion == 'G') {
            int token_width;

            token_width = width;
            if (conversion == 's' || conversion == 'd' ||
                conversion == 'i' || conversion == 'u' ||
                conversion == 'o' || conversion == 'x' ||
                conversion == 'X' || conversion == 'p') {
                if (!__stdio_scan_token(stream, token, token_width, 1,
                                        &consumed)) {
                    return assignments == 0 ? EOF : assignments;
                }
                length = (int)strlen(token);
                if (conversion == 's') {
                    char *destination;
                    int index;

                    if (!suppress) {
                        destination = va_arg(arguments, char *);
                        for (index = 0; index <= length; ++index)
                            destination[index] = token[index];
                        ++assignments;
                    }
                } else if (conversion == 'p') {
                    char *end;
                    unsigned long value;

                    value = strtoul(token, &end, 0);
                    if (end == token || *end != '\0')
                        return assignments;
                    if (!suppress) {
                        *va_arg(arguments, void **) = (void *)value;
                        ++assignments;
                    }
                } else if (conversion == 'd' || conversion == 'i') {
                    char *end;
                    long value;

                    value = strtol(token, &end,
                                   conversion == 'i' ? 0 : 10);
                    if (end == token || *end != '\0')
                        return assignments;
                    if (!suppress) {
                        if (long_value)
                            *va_arg(arguments, long *) = value;
                        else
                            *va_arg(arguments, int *) = (int)value;
                        ++assignments;
                    }
                } else {
                    char *end;
                    int base;
                    unsigned long value;

                    base = conversion == 'o' ? 8 :
                           (conversion == 'x' || conversion == 'X' ? 16 : 10);
                    value = strtoul(token, &end, base);
                    if (end == token || *end != '\0')
                        return assignments;
                    if (!suppress) {
                        if (long_value)
                            *va_arg(arguments, unsigned long *) = value;
                        else
                            *va_arg(arguments, unsigned int *) =
                                (unsigned int)value;
                        ++assignments;
                    }
                }
            } else {
                char *end;
                double value;

                if (!__stdio_scan_token(stream, token, token_width, 1,
                                        &consumed))
                    return assignments == 0 ? EOF : assignments;
                length = (int)strlen(token);
                value = strtod(token, &end);
                if (end == token || *end != '\0')
                    return assignments;
                if (!suppress) {
                    if (long_value)
                        *va_arg(arguments, double *) = value;
                    else
                        *va_arg(arguments, float *) = (float)value;
                    ++assignments;
                }
            }
            continue;
        }
        if (conversion == 'n') {
            if (!suppress) {
                if (long_value)
                    *va_arg(arguments, long *) = consumed;
                else
                    *va_arg(arguments, int *) = consumed;
            }
            continue;
        }
        errno = EINVAL;
        return assignments;
    }
    return assignments;
}
