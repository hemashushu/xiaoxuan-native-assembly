/**
 * Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions,
 * more details in file LICENSE and CONTRIBUTING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <unistd.h>
#include <time.h>
#include <string.h>

extern void inc_tls(int);
extern int get_tls_var();
extern __thread int tls_var;

void sleep_100ms()
{
    struct timespec ts;
    ts.tv_sec = 0;
    ts.tv_nsec = 100 * 1000;
    nanosleep(&ts, NULL);
}

void *child_thread_start(void *arg)
{
    // pthread_t tid = pthread_self();
    long tid = (long)arg;

    printf("thread: %ld >> init value: %d\n", tid, tls_var);
    // printf("thread: %ld >> init value (read from lib): %d\n", tid, get_tls_var());
    sleep_100ms();

    inc_tls(11);
    printf("thread: %ld >> after inc 11: %d\n", tid, tls_var);
    // printf("thread: %ld >> after inc 11 (read from lib): %d\n", tid, get_tls_var());
    sleep_100ms();

    tls_var = 13;
    printf("thread: %ld >> after reset to 13: %d\n", tid, tls_var);
    // printf("thread: %ld >> after reset to 13 (read from lib): %d\n", tid, get_tls_var());
    sleep_100ms();

    pthread_exit(NULL);
}

void test_threads(void)
{
    int num_threads = 5;
    pthread_t threads[num_threads];

    for (int i = 0; i < num_threads; i++)
    {
        pthread_create(&threads[i], NULL, &child_thread_start, (void *)(long)i);
    }

    for (int i = 0; i < num_threads; i++)
    {
        pthread_join(threads[i], NULL);
    }
}

void test_single_thread(void)
{
    printf("init value: %d\n", tls_var);
    // printf("init value (read from lib): %d\n", get_tls_var());

    inc_tls(11);
    printf("after inc 11: %d\n", tls_var);
    // printf("after inc 11 (read from lib): %d\n", get_tls_var());

    tls_var = 13;
    printf("after reset to 13: %d\n", tls_var);
    // printf("after reset to 13 (read from lib): %d\n", get_tls_var());
}

int main(int argc, char *argv[])
{
    printf("testing init a variable, and then inc it by 11, then reset it to 13.\n");
    printf("all tests operate on the TLS variable (one per thread).\n");
    printf("runs with arg -t for multithread testing.\n");
    printf("\n");

    if (argc >= 2 &&
        strcmp(argv[1], "-t") == 0)
    {
        test_threads();
    }
    else
    {
        test_single_thread();
    }

    exit(EXIT_SUCCESS);
}