module lessons.scan;

program seed(write dataset<Count> input) on compute {
    parallel (item in input) {
        let k = index() % 4;
        item.value = k == 0 ? 3 : (k == 1 ? 1 : (k == 2 ? 4 : 2));
    }
}

program offsets(read dataset<Count> input, write dataset<Count> output) on compute {
    scan_exclusive_sum(input.value, output.value);
}

program sum(read dataset<Count> input, write dataset<Count, 1> total) on compute {
    reduce_sum(input.value, total.value);
}

program demo_sum(inout dataset<Count> input, write dataset<Count, 1> total) on compute {
    seed(input);
    sum(input, total);
}

program demo(inout dataset<Count> input, write dataset<Count> output) on compute {
    seed(input);
    offsets(input, output);
}
