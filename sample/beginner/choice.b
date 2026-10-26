module beginner.choice;

program run(write dataset<u32> levels) on compute {
    parallel (level in levels) {
        u32 score = 45 + index() * 15;
        if (score >= 90) {
            level = 3;
        } else if (score >= 75) {
            level = 2;
        } else if (score >= 60) {
            level = 1;
        } else {
            level = 0;
        }
    }
}
