module lessons.map;

program seed(write dataset<f32> values) on compute {
    parallel (value in values) {
        value = f32(index() + 1);
    }
}

program scale(inout dataset<f32> values) on compute {
    parallel (value in values) {
        value = value * 2.0 + 1.0;
    }
}

program demo(inout dataset<f32> values) on compute {
    seed(values);
    scale(values);
}
