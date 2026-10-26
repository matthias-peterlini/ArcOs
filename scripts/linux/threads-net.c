#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>
#include <poll.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>

static int counter = 0;
static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cv = PTHREAD_COND_INITIALIZER;
static int done = 0;

static void *worker(void *arg) {
    for (int i = 0; i < 100000; i++) {
        pthread_mutex_lock(&mu);
        counter++;
        pthread_mutex_unlock(&mu);
    }
    pthread_mutex_lock(&mu);
    done++;
    pthread_cond_signal(&cv);
    pthread_mutex_unlock(&mu);
    return arg;
}

int main(int argc, char **argv) {
    pthread_t t[4];
    for (long i = 0; i < 4; i++) pthread_create(&t[i], 0, worker, (void *)i);
    pthread_mutex_lock(&mu);
    while (done < 4) pthread_cond_wait(&cv, &mu);
    pthread_mutex_unlock(&mu);
    for (int i = 0; i < 4; i++) { void *r; pthread_join(t[i], &r); }
    printf("ct: threads counter=%d\n", counter);

    int p[2];
    pipe(p);
    write(p[1], "pipe-ok", 7);
    struct pollfd pf = { p[0], POLLIN, 0 };
    int n = poll(&pf, 1, 1000);
    char buf[64] = {0};
    read(p[0], buf, sizeof buf - 1);
    printf("ct: poll=%d revents=%d data=%s\n", n, pf.revents, buf);

    FILE *f = fopen("mm.txt", "w"); fputs("mapped file contents", f); fclose(f);
    int fd = open("mm.txt", O_RDONLY);
    char *m = mmap(0, 4096, PROT_READ, MAP_PRIVATE, fd, 0);
    printf("ct: mmap=%.20s\n", m);
    char *big = mmap(0, 256 << 20, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    big[0] = 1; big[(256 << 20) - 1] = 2;
    printf("ct: big reservation ok %d\n", big[0] + big[(256 << 20) - 1]);

    if (argc > 2) {
        int s = socket(AF_INET, SOCK_STREAM, 0);
        struct sockaddr_in a = {0};
        a.sin_family = AF_INET; a.sin_port = htons(atoi(argv[2]));
        inet_pton(AF_INET, argv[1], &a.sin_addr);
        if (connect(s, (struct sockaddr *)&a, sizeof a) == 0) {
            const char *req = "GET /hello.txt HTTP/1.0\r\n\r\n";
            send(s, req, strlen(req), 0);
            char r[512] = {0}; int k, tot = 0;
            while ((k = recv(s, r + tot, sizeof r - 1 - tot, 0)) > 0) tot += k;
            char *body = strstr(r, "\r\n\r\n");
            printf("ct: http body=%s", body ? body + 4 : "(none)\n");
        } else perror("ct: connect");
        close(s);
        struct addrinfo *res;
        int e = getaddrinfo("localhost", "80", 0, &res);
        printf("ct: getaddrinfo localhost=%d\n", e);
    }
    return 0;
}
