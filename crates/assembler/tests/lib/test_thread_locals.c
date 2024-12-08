/**
 * Copyright (c) 2023 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions,
 * more details in file LICENSE and CONTRIBUTING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>

extern int normal_var;
extern __thread int tls_var;

void sleep_100ms()
{
    struct timespec ts;
    ts.tv_sec = 0;
    ts.tv_nsec = 100 * 1000;
    nanosleep(&ts, NULL);
}

void *test_thread_start(void *arg)
{
    // pthread_t tid = pthread_self();
    long tid = (long)arg;

    for (int i = 0; i < 3; i++)
    {
        tls_var++;
        normal_var++;
        printf("thread id: %ld >> tls var: %d, normal var:%d\n", tid, tls_var, normal_var);
        sleep_100ms();
    }

    pthread_exit(NULL);
}

int main(int argc, char *argv[])
{
    int num_threads = 5;
    pthread_t threads[num_threads];

    for (int i = 0; i < num_threads; i++)
    {
        pthread_create(&threads[i], NULL, &test_thread_start, (void *)(long)i);
    }

    for (int i = 0; i < num_threads; i++)
    {
        pthread_join(threads[i], NULL);
    }

    exit(EXIT_SUCCESS);
}
