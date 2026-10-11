#include <stdio.h>

#define CONSOLE_DATA (*(volatile unsigned char *)0x43200UL)
#define CONSOLE_STATUS (*(volatile unsigned char *)0x43201UL)

int fgetc(FILE *stream)
{
    if (stream != stdin)
        return EOF;
    while (!(CONSOLE_STATUS & 1))
        ;
    return CONSOLE_DATA;
}

int fputc(int character, FILE *stream)
{
    if (stream != stdout && stream != stderr)
        return EOF;
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

            text = va_arg(arguments, const char *);
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
