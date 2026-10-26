module beginner.functions;

u32 add_bonus(u32 score, u32 bonus) on compute {
    return score + bonus;
}

u32 level_of(u32 score) on compute {
    if (score >= 90) {
        return 3;
    }
    if (score >= 75) {
        return 2;
    }
    if (score >= 60) {
        return 1;
    }
    return 0;
}

program run(write dataset<u32> levels) on compute {
    parallel (level in levels) {
        u32 score = 45 + index() * 15;
        level = level_of(add_bonus(score, 5));
    }
}
