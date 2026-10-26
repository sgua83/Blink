module beginner.loops;

program for_loop(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        u32 n = index() + 1;
        u32 total = 0;
        for (i in 1..n + 1) {
            total += i;
        }
        number = total;
    }
}

program while_loop(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        u32 n = index() + 1;
        u32 i = 1;
        u32 total = 0;
        while (i <= n) {
            total += i;
            i += 1;
        }
        number = total;
    }
}

program first_square(write dataset<u32> numbers) on compute {
    parallel (number in numbers) {
        u32 n = index() + 1;
        u32 first = 0;
        for (k in 1..10) {
            if (k * k >= n) {
                first = k;
                break;
            }
        }
        number = first;
    }
}
