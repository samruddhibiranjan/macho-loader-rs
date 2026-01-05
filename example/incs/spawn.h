#ifndef __SPAWN_H__
#define __SPAWN_H__

#include "vector.h"
#include <stdint.h>
#include <stdbool.h>
#include <sys/types.h>

/* Executes the in-memory binary slice (`data`, `len`) with the
 * arguments `argc`, `argv` and `envp`.
 */
extern void execvm(uint32_t ac, const uint8_t **av, const uint8_t **ep,
                   const uint8_t *data, size_t len);

pid_t spawn(const char *filename, int ac, const uint8_t **av,
            const uint8_t **ep);

void signal_error(int sig_code);
bool realloc_internal(char **buf, size_t *cap, size_t len, size_t new_cap);
bool file_open_readable(const char *fn, int *fd);
ssize_t fd_read_to_vec(int fd, vector *vector, size_t max_size);

#endif /* __SPAWN_H__ */
