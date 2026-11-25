module bootstrap.relation_order;

struct Input {
    address rows;
    address dependencies;
    usize count;
};

struct Workspace {
    address states;
    address users;
    address ready;
};

struct Output {
    address ordered;
    address fixed_inputs;
};

extern "C" usize plan(Input input, Workspace work, Output output) on cpu;
