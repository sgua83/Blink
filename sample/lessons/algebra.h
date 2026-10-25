module lessons.algebra;

struct Affine { u32 scale; u32 bias; };
struct Transform { Affine value; };

Affine identity() on compute;
Affine compose(Affine a, Affine b) on compute;
program chain(read dataset<Transform> input,
              write dataset<Transform> before,
              write dataset<Transform, 1> total) on compute;
program demo(inout dataset<Transform> input,
             write dataset<Transform> before,
             write dataset<Transform, 1> total) on compute;
