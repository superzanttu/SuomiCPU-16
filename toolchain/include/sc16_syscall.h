#ifndef SC16_SYSCALL_H
#define SC16_SYSCALL_H

#define SC16_SYSCALL_ABI_VERSION 1
#define SC16_SYSCALL_GRAPHICS 24
#define SC16_GRAPHICS_PARAMETERS_SIZE 10
#define SC16_MAX_ARGUMENTS 16
#define SC16_MAX_ARGUMENT_BYTES 1024

#define SC16_PROT_READ 1
#define SC16_PROT_WRITE 2
#define SC16_PROT_EXEC 4

#define SC16_OPEN_READ 1
#define SC16_OPEN_WRITE 2
#define SC16_OPEN_CREATE 4
#define SC16_OPEN_TRUNCATE 8
#define SC16_OPEN_APPEND 16

#define SC16_SEM_CREATE 0
#define SC16_SEM_WAIT 1
#define SC16_SEM_POST 2
#define SC16_SEM_DESTROY 3

struct sc16_graphics_parameters {
    unsigned char color;
    unsigned char mode;
    unsigned int x;
    unsigned int y;
    unsigned int width;
    unsigned int height;
};

long sc16_getpid(void);
void sc16_exit(int status);
int sc16_yield(void);
long sc16_spawn(const char *path);
long sc16_spawn_arg(const char *path, unsigned long argument);
long sc16_spawn_exec(const char *path, const char *arguments,
                     unsigned int length, unsigned int count);
int sc16_argument_count(void);
int sc16_argument(unsigned int index, char *buffer, unsigned int capacity);
long sc16_startup_argument(void);
long sc16_monotonic_ms(void);
int sc16_process_info(unsigned int index, char *buffer, unsigned int capacity);
int sc16_process_status(unsigned int index, char *buffer, unsigned int capacity);
long sc16_mmap(unsigned long length, unsigned int permissions, void *hint);
int sc16_open(const char *path, unsigned int flags);
int sc16_read(int handle, void *buffer, unsigned int length);
int sc16_write(int handle, const void *buffer, unsigned int length);
int sc16_close(int handle);
long sc16_seek(int handle, long offset, int origin);
int sc16_sleep(unsigned long ticks);
int sc16_sleep_ms(unsigned long milliseconds);
int sc16_wait(unsigned long child_pid);
int sc16_trywait(unsigned long child_pid);
long sc16_semop(unsigned int operation, unsigned long semaphore_id,
                unsigned int initial_value);
int sc16_graphics(unsigned int operation,
                  const struct sc16_graphics_parameters *parameters,
                  const void *data, unsigned int length);

#endif
