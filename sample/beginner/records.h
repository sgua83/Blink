module beginner.records;

struct Student {
    u32 score;
    bool passed;
};
program run(write dataset<Student> students) on compute;
