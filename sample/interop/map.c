#include "map.abi.h"
#include <stdio.h>

int main(void) {
    float values[] = {1, 2, 3, 4};
    map_handle *program = NULL;
    char error[4096];
    if (map_create(0, &program, error, sizeof(error))) {
        fprintf(stderr, "%s\n", error);
        return 1;
    }
    int result = map_run(program, values, 4, error, sizeof(error));
    map_destroy(program);
    if (result) {
        fprintf(stderr, "%s\n", error);
        return 1;
    }
    for (int i = 0; i < 4; ++i) printf("%.0f ", values[i]);
    puts("");
    return 0;
}
