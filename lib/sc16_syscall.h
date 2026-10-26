#ifndef SC16_SYSCALL_H
#define SC16_SYSCALL_H

#define SC16_SYSCALL_ABI_VERSION 1

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

long sc16_getpid(void);
void sc16_exit(int status);
int sc16_yield(void);
long sc16_spawn(const char *path);
long sc16_mmap(unsigned long length, unsigned int permissions, void *hint);
int sc16_open(const char *path, unsigned int flags);
int sc16_read(int handle, void *buffer, unsigned int length);
int sc16_write(int handle, const void *buffer, unsigned int length);
int sc16_close(int handle);
long sc16_seek(int handle, long offset, int origin);
int sc16_sleep(unsigned long ticks);
int sc16_wait(unsigned long child_pid);
long sc16_semop(unsigned int operation, unsigned long semaphore_id,
                unsigned int initial_value);

#endif
