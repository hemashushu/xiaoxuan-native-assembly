/**
 * Copyright (c) 2024 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions,
 * more details in file LICENSE and CONTRIBUTING.
 */

// #include <stdio.h>
// #include <pthread.h>

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

// compile this file with the following command to generate a shared library:
//
// `$ gcc -Wall -g -fpic -shared -Wl,-soname,libtest0.so.1 -o libtest0.so.1.0.0 libtest0.c`
//
// then create soname and link-name by creating symbolic links
// to this shared library:
//
// - `ln -s libtest0.so.1.0.0 libtest0.so.1`
// - `ln -s libtest0.so.1.0.0 libtest0.so`

/**
 *
 * for unit tests of 'code_generator.rs/utils.rs'
 *
 */

__thread int tls_var = 0;
int normal_var = 0;

// static void *inc_tls_internal(void *arg)
// {
//     pthread_t tid = pthread_self();
//     int increment = *((int *)arg);
//     tls_var += increment;
//
//     printf("child thread created, thread-id: %ld, increment: %d, current tls value: %d\n", tid, increment, tls_var);
//     pthread_exit(NULL);
// }
//
// void inc_tls_in_a_new_thread(int increment)
// {
//     pthread_t tid;
//     pthread_create(&tid, NULL, &inc_tls_internal, &increment);
//     pthread_join(tid, NULL);
// }

void inc_normal(int increment) {
    normal_var+=increment;
}

int read_normal() {
    return normal_var;
}

void inc_tls(int increment) {
    tls_var+=increment;
}

int read_tls() {
    return tls_var;
}