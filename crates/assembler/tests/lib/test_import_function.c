/**
 * Copyright (c) 2023 Hemashushu <hippospark@gmail.com>, All rights reserved.
 *
 * This Source Code Form is subject to the terms of
 * the Mozilla Public License version 2.0 and additional exceptions,
 * more details in file LICENSE and CONTRIBUTING.
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

extern int add(int, int);

int main(int argc, char *argv[])
{
    int result = add(11, 13);
    printf("11 + 13 = %d\n", result);

    exit(EXIT_SUCCESS);
}