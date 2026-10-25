module book.handoff;
program finish(read dataset<BLTotal,1> left, read dataset<BLTotal,1> right,
    write dataset<BLTotal,1> combined) on compute {
    parallel (value in combined) {
        value.value=left[0].value+right[0].value;
    }
}
