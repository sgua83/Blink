module applications.algebra_gpu;

struct Affine { u32 scale; u32 bias; };
struct Transform { Affine value; };

Affine identity() on compute { return Affine(1, 0); }

Affine compose(Affine a, Affine b) on compute {
    return Affine(b.scale * a.scale, b.scale * a.bias + b.bias);
}

void accumulate_transforms(read dataset<Transform> input,
                           write dataset<Transform> before,
                           write dataset<Transform, 1> total) on compute {
    scan_exclusive(input.value, before.value, identity, compose);
    reduce(input.value, total.value, identity, compose);
}

u32 main() on cpu {
    dataset<Transform> input = dataset<Transform>(4);
    dataset<Transform> before = dataset<Transform>(4);
    dataset<Transform, 1> total = dataset<Transform>(1);

    parallel (item in input) { item.value = Affine(2, 1); }

    upload(input);
    upload(before);
    upload(total);
    target(gpu, accumulate_transforms(input, before, total));
    download(before);
    download(total);

    for (row in before) {
        if (!println(row.value.scale) || !println(row.value.bias)) { return 1; }
    }
    if (!println(total[0].value.scale) || !println(total[0].value.bias)) { return 1; }
    return 0;
}
