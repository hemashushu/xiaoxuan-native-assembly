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

// import data
extern int normal_var;

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

    printf("thread: %ld >> init value: %d\n", tid, normal_var);
    sleep_100ms();

    normal_var += 11;
    printf("thread: %ld >> after inc 11: %d\n", tid, normal_var);
    sleep_100ms();

    normal_var = 13;
    printf("thread: %ld >> after reset to 13: %d\n", tid, normal_var);
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
    printf("init value: %d\n", normal_var);

    normal_var += 11;
    printf("after inc 11: %d\n", normal_var);

    normal_var = 13;
    printf("after reset to 13: %d\n", normal_var);
}

int main(int argc, char *argv[])
{
    printf("testing init a variable, and then inc it by 11, then reset it to 13.\n");
    printf("all tests operate on the same (single) variable.\n");
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