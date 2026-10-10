#include <stddef.h>

char *strtok_state;

void *memchr(const void *s, int c, size_t n)
{
    const unsigned char *p;
    size_t i;
    p = (const unsigned char *)s;
    for (i = 0; i < n; ++i) {
        if (p[i] == (unsigned char)c)
            return (void *)(p + i);
    }
    return NULL;
}

int memcmp(const void *s1, const void *s2, size_t n)
{
    const unsigned char *a;
    const unsigned char *b;
    size_t i;
    a = (const unsigned char *)s1;
    b = (const unsigned char *)s2;
    for (i = 0; i < n; ++i) {
        if (a[i] != b[i])
            return (int)a[i] - (int)b[i];
    }
    return 0;
}

void *memcpy(void *s1, const void *s2, size_t n)
{
    unsigned char *d;
    const unsigned char *s;
    size_t i;
    d = (unsigned char *)s1;
    s = (const unsigned char *)s2;
    for (i = 0; i < n; ++i)
        d[i] = s[i];
    return s1;
}

void *memmove(void *s1, const void *s2, size_t n)
{
    unsigned char *d;
    const unsigned char *s;
    size_t i;
    d = (unsigned char *)s1;
    s = (const unsigned char *)s2;
    if (d < s) {
        for (i = 0; i < n; ++i)
            d[i] = s[i];
    } else if (d > s) {
        i = n;
        while (i != 0) {
            --i;
            d[i] = s[i];
        }
    }
    return s1;
}

void *memset(void *s, int c, size_t n)
{
    unsigned char *p;
    size_t i;
    p = (unsigned char *)s;
    for (i = 0; i < n; ++i)
        p[i] = (unsigned char)c;
    return s;
}

char *strcat(char *s1, const char *s2)
{
    char *d;
    d = s1 + strlen(s1);
    while (*s2 != '\0') {
        *d = *s2;
        ++d;
        ++s2;
    }
    *d = '\0';
    return s1;
}

char *strchr(const char *s, int c)
{
    char wanted;
    wanted = (char)c;
    for (;;) {
        if (*s == wanted)
            return (char *)s;
        if (*s == '\0')
            return NULL;
        ++s;
    }
}

int strcmp(const char *s1, const char *s2)
{
    while (*s1 != '\0' && *s1 == *s2) {
        ++s1;
        ++s2;
    }
    return (int)(unsigned char)*s1 - (int)(unsigned char)*s2;
}

int strcoll(const char *s1, const char *s2)
{
    return strcmp(s1, s2);
}

char *strcpy(char *s1, const char *s2)
{
    char *result;
    result = s1;
    while (*s2 != '\0') {
        *s1 = *s2;
        ++s1;
        ++s2;
    }
    *s1 = '\0';
    return result;
}

size_t strcspn(const char *s1, const char *s2)
{
    size_t n;
    n = 0;
    while (s1[n] != '\0' && strchr(s2, s1[n]) == NULL)
        ++n;
    return n;
}

char *strerror(int errnum)
{
    if (errnum == 33)
        return "domain error";
    if (errnum == 34)
        return "range error";
    return "unknown error";
}

size_t strlen(const char *s)
{
    size_t n;
    n = 0;
    while (s[n] != '\0')
        ++n;
    return n;
}

char *strncat(char *s1, const char *s2, size_t n)
{
    char *result;
    result = s1;
    while (*s1 != '\0')
        ++s1;
    while (n != 0 && *s2 != '\0') {
        *s1 = *s2;
        ++s1;
        ++s2;
        --n;
    }
    *s1 = '\0';
    return result;
}

int strncmp(const char *s1, const char *s2, size_t n)
{
    while (n != 0 && *s1 != '\0' && *s1 == *s2) {
        ++s1;
        ++s2;
        --n;
    }
    if (n == 0)
        return 0;
    return (int)(unsigned char)*s1 - (int)(unsigned char)*s2;
}

char *strncpy(char *s1, const char *s2, size_t n)
{
    char *result;
    size_t i;
    result = s1;
    i = 0;
    while (i < n && s2[i] != '\0') {
        s1[i] = s2[i];
        ++i;
    }
    while (i < n)
        s1[i++] = '\0';
    return result;
}

char *strpbrk(const char *s1, const char *s2)
{
    while (*s1 != '\0') {
        if (strchr(s2, *s1) != NULL)
            return (char *)s1;
        ++s1;
    }
    return NULL;
}

char *strrchr(const char *s, int c)
{
    const char *last;
    char wanted;
    last = NULL;
    wanted = (char)c;
    for (;;) {
        if (*s == wanted)
            last = s;
        if (*s == '\0')
            return (char *)last;
        ++s;
    }
}

size_t strspn(const char *s1, const char *s2)
{
    size_t n;
    n = 0;
    while (s1[n] != '\0' && strchr(s2, s1[n]) != NULL)
        ++n;
    return n;
}

char *strstr(const char *s1, const char *s2)
{
    const char *start;
    const char *a;
    const char *b;
    if (*s2 == '\0')
        return (char *)s1;
    for (start = s1; *start != '\0'; ++start) {
        a = start;
        b = s2;
        while (*a != '\0' && *b != '\0' && *a == *b) {
            ++a;
            ++b;
        }
        if (*b == '\0')
            return (char *)start;
    }
    return NULL;
}

char *strtok(char *s1, const char *s2)
{
    char *start;
    if (s1 != NULL)
        strtok_state = s1;
    else if (strtok_state == NULL)
        return NULL;
    while (*strtok_state != '\0' && strchr(s2, *strtok_state) != NULL)
        ++strtok_state;
    if (*strtok_state == '\0') {
        strtok_state = NULL;
        return NULL;
    }
    start = strtok_state;
    while (*strtok_state != '\0' && strchr(s2, *strtok_state) == NULL)
        ++strtok_state;
    if (*strtok_state != '\0') {
        *strtok_state = '\0';
        ++strtok_state;
    } else {
        strtok_state = NULL;
    }
    return start;
}

size_t strxfrm(char *s1, const char *s2, size_t n)
{
    size_t length;
    size_t i;
    length = strlen(s2);
    if (n != 0) {
        i = 0;
        while (i + 1 < n && s2[i] != '\0') {
            s1[i] = s2[i];
            ++i;
        }
        if (i < n)
            s1[i] = '\0';
    }
    return length;
}
