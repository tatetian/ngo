#include <sys/syscall.h>
#include <sys/time.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <spawn.h>
#include <string.h>
#include <pthread.h>

#define KB              (1024UL)
#define MB              (1024UL * 1024UL)
#define GB              (1024UL * 1024UL * 1024UL)

#define MIN(x, y)       ((x) <= (y) ? (x) : (y))

static int produce_bytes(int fd, size_t buf_size, size_t remain_nbytes) {
    void *buf = malloc(buf_size);
    if (buf == NULL) {
        printf("ERROR: failed to allocate buffer\n");
        return -1;
    }
    memset(buf, 0, buf_size);

    while (remain_nbytes > 0) {
        size_t len = MIN(buf_size, remain_nbytes);
        if ((len = write(fd, buf, len)) < 0) {
            printf("ERROR: failed to write to pipe\n");
            return -1;
        }
        remain_nbytes -= len;
    }

    free(buf);
    return 0;
}

static int consume_bytes(int fd, size_t buf_size, size_t remain_nbytes) {
    void *buf = malloc(buf_size);
    if (buf == NULL) {
        printf("ERROR: failed to allocate buffer\n");
        return -1;
    }
    memset(buf, 0, buf_size);

    while (remain_nbytes > 0) {
        size_t len = MIN(buf_size, remain_nbytes);
        if ((len = read(fd, buf, len)) < 0) {
            printf("ERROR: failed to write to pipe\n");
            return -1;
        }
        remain_nbytes -= len;
    }

    free(buf);
    return 0;
}

struct consumer_arg {
    int fd;
    size_t buf_size;
    size_t total_nbytes;
};

static void *consumer_fn(void *_arg) {
    struct consumer_arg *arg = _arg;
    int fd = arg->fd;
    size_t buf_size = arg->buf_size;
    size_t total_nbytes = arg->total_nbytes;
    consume_bytes(fd, buf_size, total_nbytes);
    return NULL;
}

int main(int argc, const char *argv[]) {
    // Create pipe
    int pipe_fds[2];
    if (pipe(pipe_fds) < 0) {
        printf("ERROR: failed to create a pipe\n");
        return -1;
    }
    int pipe_rd_fd = pipe_fds[0];
    int pipe_wr_fd = pipe_fds[1];

    size_t total_nbytes = 8 * GB;
    size_t buf_size = 1 * MB;

    pthread_t consumer_thread;
    struct consumer_arg consumer_arg = {
        pipe_rd_fd, buf_size, total_nbytes,
    };
    if (pthread_create(&consumer_thread, NULL, consumer_fn, (void *)&consumer_arg) != 0) {
        printf("ERROR: failed to create thread\n");
        return -1;
    }

    // Start the timer
    struct timeval tv_start, tv_end;
    gettimeofday(&tv_start, NULL);

    if (produce_bytes(pipe_wr_fd, buf_size, total_nbytes) < 0) {
        printf("ERROR: failed to produce bytes\n");
        return -1;
    }

    if (pthread_join(consumer_thread, NULL) != 0) {
        printf("ERROR: failed to join the consumer thread\n");
        return -1;
    }

    // Stop the timer
    gettimeofday(&tv_end, NULL);

    // Calculate the throughput
    double total_s = (tv_end.tv_sec - tv_start.tv_sec)
                     + (double)(tv_end.tv_usec - tv_start.tv_usec) / 1000000;
    if (total_s < 1.0) {
        printf("WARNING: run long enough to get meaningful results\n");
        if (total_s == 0) { return 0; }
    }
    double total_mb = (double)total_nbytes / MB;
    double throughput = total_mb / total_s;
    printf("Throughput of pipe is %.2f MB/s\n", throughput);
    return 0;
}
