module beginner.records;

program run(write dataset<Student> students) on compute {
    parallel (student in students) {
        student.score = 45 + index() * 15;
        student.passed = student.score >= 60;
    }
}
