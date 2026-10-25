module lessons.algebra;

Affine identity() on compute { return Affine(1, 0); }

Affine compose(Affine a, Affine b) on compute {
    return Affine(b.scale * a.scale, b.scale * a.bias + b.bias);
}

program chain(read dataset<Transform> input,
              write dataset<Transform> before,
              write dataset<Transform, 1> total) on compute {
    scan_exclusive(input.value, before.value, identity, compose);
    reduce(input.value, total.value, identity, compose);
}

program demo(inout dataset<Transform> input,
             write dataset<Transform> before,
             write dataset<Transform, 1> total) on compute {
    parallel (item in input) { item.value = Affine(2, 1); }
    chain(input, before, total);
}
