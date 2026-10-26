#include <stdio.h>
#include <errno.h>
#include <sc16_syscall.h>

int errno;

int __stdio_mode_flags(const char *mode);

struct __sc16_FILE {
    int __opaque;
};

#define SC16_FILE_TOKEN_BASE 256

static int __stdio_handles[FOPEN_MAX - 3];
static int __stdio_flags[FOPEN_MAX - 3];
static int __stdio_eof[FOPEN_MAX - 3];
static int __stdio_errors[FOPEN_MAX - 3];
static int __stdio_pushback[FOPEN_MAX - 3];

static int __stdio_file_slot(FILE *stream)
{
    int slot;

    slot = (int)stream - SC16_FILE_TOKEN_BASE;
    if (slot < 0 || slot >= FOPEN_MAX - 3 || __stdio_handles[slot] == 0)
        return -1;
    return slot;
}

static void __stdio_set_error(long result)
{
    if (result == -2L)
        errno = ENOENT;
    else if (result == -4L)
        errno = EINTR;
    else if (result == -5L)
        errno = EIO;
    else if (result == -9L)
        errno = EBADF;
    else if (result == -12L)
        errno = ENOMEM;
    else if (result == -13L)
        errno = EACCES;
    else if (result == -22L)
        errno = EINVAL;
    else if (result == -24L)
        errno = EMFILE;
    else if (result == -38L)
        errno = ENOSYS;
    else if (result == -75L)
        errno = EOVERFLOW;
    else
        errno = EIO;
}

FILE *fopen(const char *filename, const char *mode)
{
    int slot;
    int flags;
    int handle;

    if (filename == NULL) {
        errno = EINVAL;
        return 0;
    }
    flags = __stdio_mode_flags(mode);
    if (flags < 0) {
        errno = EINVAL;
        return 0;
    }
    slot = 0;
    while (slot < FOPEN_MAX - 3 && __stdio_handles[slot] != 0)
        ++slot;
    if (slot >= FOPEN_MAX - 3) {
        errno = EMFILE;
        return 0;
    }
    handle = sc16_open(filename, (unsigned int)flags);
    if (handle < 0) {
        __stdio_set_error(handle);
        return 0;
    }
    __stdio_handles[slot] = handle;
    __stdio_flags[slot] = flags;
    __stdio_eof[slot] = 0;
    __stdio_errors[slot] = 0;
    __stdio_pushback[slot] = EOF;
    return (FILE *)(slot + SC16_FILE_TOKEN_BASE);
}

FILE *freopen(const char *filename, const char *mode, FILE *stream)
{
    if (fclose(stream) == EOF)
        return 0;
    return fopen(filename, mode);
}

int fclose(FILE *stream)
{
    int slot;
    int result;

    slot = __stdio_file_slot(stream);
    if (slot < 0) {
        errno = EBADF;
        return EOF;
    }
    result = sc16_close(__stdio_handles[slot]);
    if (result < 0) {
        __stdio_set_error(result);
        __stdio_errors[slot] = 1;
        return EOF;
    }
    __stdio_handles[slot] = 0;
    __stdio_flags[slot] = 0;
    __stdio_eof[slot] = 0;
    __stdio_errors[slot] = 0;
    __stdio_pushback[slot] = EOF;
    return 0;
}

int fgetc(FILE *stream)
{
    unsigned char character;
    int slot;
    int result;

    slot = __stdio_file_slot(stream);
    if (slot < 0 || !(__stdio_flags[slot] & SC16_OPEN_READ)) {
        errno = EBADF;
        if (slot >= 0)
            __stdio_errors[slot] = 1;
        return EOF;
    }
    if (__stdio_pushback[slot] != EOF) {
        result = __stdio_pushback[slot];
        __stdio_pushback[slot] = EOF;
        return result;
    }
    result = sc16_read(__stdio_handles[slot], &character, 1);
    if (result < 0) {
        __stdio_set_error(result);
        __stdio_errors[slot] = 1;
        return EOF;
    }
    if (result == 0) {
        __stdio_eof[slot] = 1;
        return EOF;
    }
    return character;
}

int fputc(int character, FILE *stream)
{
    unsigned char byte;
    int slot;
    int result;

    slot = __stdio_file_slot(stream);
    if (slot < 0 || !(__stdio_flags[slot] & SC16_OPEN_WRITE)) {
        errno = EBADF;
        if (slot >= 0)
            __stdio_errors[slot] = 1;
        return EOF;
    }
    byte = (unsigned char)character;
    result = sc16_write(__stdio_handles[slot], &byte, 1);
    if (result == 1)
        return byte;
    if (result < 0)
        __stdio_set_error(result);
    __stdio_errors[slot] = 1;
    return EOF;
}

size_t fread(void *pointer, size_t size, size_t count, FILE *stream)
{
    unsigned char *bytes;
    size_t total;
    size_t index;
    size_t request;
    int slot;
    int result;

    if (size == 0 || count == 0)
        return 0;
    if (pointer == NULL || count > 0xffffffffUL / size) {
        errno = EINVAL;
        return 0;
    }
    slot = __stdio_file_slot(stream);
    if (slot < 0 || !(__stdio_flags[slot] & SC16_OPEN_READ)) {
        errno = EBADF;
        if (slot >= 0)
            __stdio_errors[slot] = 1;
        return 0;
    }
    total = size * count;
    bytes = (unsigned char *)pointer;
    index = 0;
    if (__stdio_pushback[slot] != EOF && total != 0) {
        bytes[index++] = (unsigned char)__stdio_pushback[slot];
        __stdio_pushback[slot] = EOF;
    }
    while (index < total) {
        request = total - index;
        if (request > 4096)
            request = 4096;
        if (index == 0)
            result = sc16_read(__stdio_handles[slot], pointer,
                               (unsigned int)request);
        else
            result = sc16_read(__stdio_handles[slot], bytes + index,
                               (unsigned int)request);
        if (result < 0) {
            __stdio_set_error(result);
            __stdio_errors[slot] = 1;
            break;
        }
        if (result == 0) {
            __stdio_eof[slot] = 1;
            break;
        }
        index += (size_t)result;
        if ((size_t)result < request) {
            __stdio_eof[slot] = 1;
            break;
        }
    }
    return index / size;
}

size_t fwrite(const void *pointer, size_t size, size_t count, FILE *stream)
{
    const unsigned char *bytes;
    size_t total;
    size_t index;
    size_t request;
    int slot;
    int result;

    if (size == 0 || count == 0)
        return 0;
    if (pointer == NULL || count > 0xffffffffUL / size) {
        errno = EINVAL;
        return 0;
    }
    slot = __stdio_file_slot(stream);
    if (slot < 0 || !(__stdio_flags[slot] & SC16_OPEN_WRITE)) {
        errno = EBADF;
        if (slot >= 0)
            __stdio_errors[slot] = 1;
        return 0;
    }
    total = size * count;
    bytes = (const unsigned char *)pointer;
    index = 0;
    while (index < total) {
        request = total - index;
        if (request > 4096)
            request = 4096;
        if (index == 0)
            result = sc16_write(__stdio_handles[slot], pointer,
                                (unsigned int)request);
        else
            result = sc16_write(__stdio_handles[slot], bytes + index,
                                (unsigned int)request);
        if (result < 0) {
            __stdio_set_error(result);
            __stdio_errors[slot] = 1;
            break;
        }
        if (result == 0) {
            errno = EIO;
            __stdio_errors[slot] = 1;
            break;
        }
        index += (size_t)result;
        if ((size_t)result < request)
            break;
    }
    return index / size;
}

int fseek(FILE *stream, long offset, int origin)
{
    int slot;
    long position;

    slot = __stdio_file_slot(stream);
    if (slot < 0) {
        errno = EBADF;
        return EOF;
    }
    position = sc16_seek(__stdio_handles[slot], offset, origin);
    if (position < 0L) {
        __stdio_set_error(position);
        __stdio_errors[slot] = 1;
        return EOF;
    }
    __stdio_eof[slot] = 0;
    __stdio_pushback[slot] = EOF;
    return 0;
}

long ftell(FILE *stream)
{
    int slot;
    long position;

    slot = __stdio_file_slot(stream);
    if (slot < 0) {
        errno = EBADF;
        return 0xffffffffUL;
    }
    position = sc16_seek(__stdio_handles[slot], 0L, SEEK_CUR);
    if (position < 0L) {
        __stdio_set_error(position);
        __stdio_errors[slot] = 1;
    }
    return position;
}

int ungetc(int character, FILE *stream)
{
    int slot;

    if (character == EOF)
        return EOF;
    slot = __stdio_file_slot(stream);
    if (slot < 0 || !(__stdio_flags[slot] & SC16_OPEN_READ)) {
        errno = EBADF;
        return EOF;
    }
    if (__stdio_pushback[slot] != EOF)
        return EOF;
    __stdio_pushback[slot] = (unsigned char)character;
    __stdio_eof[slot] = 0;
    return __stdio_pushback[slot];
}

void clearerr(FILE *stream)
{
    int slot;

    slot = __stdio_file_slot(stream);
    if (slot >= 0) {
        __stdio_eof[slot] = 0;
        __stdio_errors[slot] = 0;
    }
}

int feof(FILE *stream)
{
    int slot;

    slot = __stdio_file_slot(stream);
    return slot >= 0 ? __stdio_eof[slot] : 0;
}

int ferror(FILE *stream)
{
    int slot;

    slot = __stdio_file_slot(stream);
    return slot >= 0 ? __stdio_errors[slot] : 0;
}

int remove(const char *filename)
{
    errno = ENOSYS;
    return 0xffff;
}
