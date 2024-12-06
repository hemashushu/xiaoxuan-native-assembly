/**
 * Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions,
 * more details in file LICENSE and CONTRIBUTING.
 */

#include <stdio.h>
#include <pthread.h>

/**
 * about the keywords 'extern' and 'static' in C language:
 *
 * ## declare but not define a function:
 *
 * - extern int add(int, int);
 * - int add(int, int);
 *
 * note that in the include file (*.h), the keyword 'extern' can be omitted.
 *
 * ## declare but not define a global variable:
 *
 * - extern int sum;
 *
 * ## define a (global) function or variable:
 *
 * - int add(...) {...}
 * - int sum;           // section '.bss'
 * - int sum = 123;     // section '.data'
 *
 * ## define a file-scope function or variable:
 *
 * - static int add(...) {...}
 * - static int sum;
 *
 * ## define a function-scope static variable:
 *
 * int add(...) {
 *      static int sum;         // section '.bss'
 *      static int sum = 123;   // section '.data'
 * }
 *
 * ref:
 * - https://www.geeksforgeeks.org/understanding-extern-keyword-in-c/
 * - https://www.geeksforgeeks.org/static-variables-in-c/
 * - https://www.geeksforgeeks.org/storage-classes-in-c/
 */

int add(int left, int right)
{
    return left + right;
}

void *get_func_add_address()
{
    return add;
}

// compile this file with the command:
// `$ gcc -Wall -g -fpic -shared -Wl,-soname,libtest0.so.1 -o libtest0.so.1.0.0 libtest0.c`
//
// it is recommended to create a symbolic link to this shared library:
// `ln -s libtest0.so.1.0.0 libtest0.so.1`
// `ln -s libtest0.so.1.0.0 libtest0.so`

__thread int tls_var = 0;
int normal_var = 0;

void inc_normal(int num)
{
    tls_var += num;
    normal_var += num;
}

static void *inc_tls_internal(void *arg)
{
    pthread_t tid = pthread_self();
    int num = *((int *)arg);
    tls_var += num;
    printf("child thread created, pid: %ld, inc: %d, current tls var: %d\n", tid, num, tls_var);
    pthread_exit(NULL);
}

void inc_tls(int num)
{
    pthread_t tid;
    pthread_create(&tid, NULL, &inc_tls_internal, &num);
    pthread_join(tid, NULL);
}

int get_tls_var()
{
    return tls_var;
}

int get_normal_var()
{
    return normal_var;
}