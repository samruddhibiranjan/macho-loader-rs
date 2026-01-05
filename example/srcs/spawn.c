#include "spawn.h"
#include "vector.h"
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

pid_t spawn_from_fd(int fd, int ac, const uint8_t **av, const uint8_t **ep) {
  /*
   * boilerplate to setup the vector that holds the binary content.
   */
  vector vector;
  vector_allocator_t allocator = {
      .alloc = malloc,
      .release = free,
  };

  if (!vector_init(&vector, allocator, sizeof(char), 2048, NULL)) {
    (void)fprintf(stderr, "vector_init error: %s\n", strerror(errno));
    return -1;
  }

  /*
   * Creating a new process
   */
  pid_t child_pid = fork();
  switch (child_pid) {
  case -1:
    (void)fprintf(stderr, "fork error: %s\n", strerror(errno));
    vector_deinit(&vector);
    return -1;

  case 0:
    /*
     * The child process reads the content of the binary file into the vector.
     * We use a maximum size of 3MB
     */
    if (fd_read_to_vec(fd, &vector, 3000000) == -1) {
      (void)fprintf(stderr, "error: file is too large\n");
      vector_deinit(&vector);
      exit(EXIT_FAILURE);
    }
    /*
     * Call execvm exacly as we would call execve, passing it a pointer
     * to the start of the vector (the binary data), and the total size
     * of it
     */
    execvm(ac, av, ep, vector_first_to_ptr(&vector), vector_length(&vector));

    /*
     * If this is reached it must mean that the program executed successfully,
     * we can exit the child process
     */
    vector_deinit(&vector);
    exit(EXIT_SUCCESS);

  default: /* parent */
    vector_deinit(&vector);
    return child_pid;
  }
}

pid_t spawn(const char *filename, int ac, const uint8_t **av,
            const uint8_t **ep) {
  int fd;

  /*
   * We open the binary file
   */
  if (!file_open_readable(filename, &fd)) {
    (void)fprintf(stderr, "%s: %s\n", filename, strerror(errno));
    return -1;
  }

  /*
   * Call `spawn_from_fd` which will attempt to spawn
   * the program that we can read from the file descripto
   */
  pid_t pid = spawn_from_fd(fd, ac, av, ep);

  (void)close(fd);
  return pid;
}

/* Sets `fd` to point to the opened filed descriptor of
 * the file `fn`.
 *
 * Returns false if the file descriptor could not be opened
 */
bool file_open_readable(const char *fn, int *fd) {
  *fd = open(fn, O_RDONLY, 0666);
  return (*fd != -1);
}

/* Reads all content from the file descriptor into a vector.
 *
 * If the file content cannot fit in `max_size`, the functions
 * returns -1.
 */
ssize_t fd_read_to_vec(int fd, vector *vector, size_t max_size) {
#define READ_SIZE 4096 * 4
  while (true) {
    if (max_size < READ_SIZE) {
      return (-1);
    }

    vector_adjust_cap_if_full(vector, READ_SIZE);

    ssize_t count = read(fd, vector_uninitialized_data(vector), READ_SIZE);
    if (count == -1) {
      (void)fprintf(stderr, "read failed: %s\n", strerror(errno));
      return (-1);
    }

    if (count == 0) {
      break;
    }

    vector_append_from_capacity(vector, count);
    max_size -= count;
  }

  return (0);
}

/* Prints the signal `sig_code` as a readable string.
 */
void signal_error(int sig_code) {
  static const char *siglist[] = {"",
                                  "Hangup",
                                  "Interrupt",
                                  "Quit",
                                  "Illegal instruction",
                                  "BPT trace/trap",
                                  "ABORT instruction",
                                  "EMT instruction",
                                  "Floating point exception",
                                  "Killed",
                                  "Bus error",
                                  "Segmentation fault",
                                  "Bad system call",
                                  "Broken pipe",
                                  "Alarm clock",
                                  "Terminated",
                                  "Urgent IO condition",
                                  "Stopped (signal)",
                                  "Stopped",
                                  "Continue",
                                  "Child death or stop",
                                  "Stopped (tty input)",
                                  "Stopped (tty output)",
                                  "I/O ready",
                                  "CPU limit",
                                  "File limit",
                                  "Alarm (virtual)",
                                  "Alarm (profile)",
                                  "Window changed",
                                  "Information request",
                                  "User signal 1",
                                  "User signal 2"};

  if (sig_code != 13 && sig_code < 32)
    (void)fprintf(stderr, "%s: %d\n", siglist[sig_code], sig_code);
}
