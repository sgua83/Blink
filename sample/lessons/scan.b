module lessons.scan;

void seed(write dataset<Count> input) on compute {
    parallel (item in input) {
        let k = index() % 4;
        item.value = k == 0 ? 3 : (k == 1 ? 1 : (k == 2 ? 4 : 2));
    }
}

void offsets(read dataset<Count> input, write dataset<Count> output) on compute {
    scan_exclusive_sum(input.value, output.value);
}

void sum(read dataset<Count> input, write dataset<Count, 1> total) on compute {
    reduce_sum(input.value, total.value);
}

void demo_sum(inout dataset<Count> input, write dataset<Count, 1> total) on compute {
    seed(input);
    sum(input, total);
}

void demo(inout dataset<Count> input, write dataset<Count> output) on compute {
    seed(input);
    offsets(input, output);
}
