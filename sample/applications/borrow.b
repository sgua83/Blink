module applications.borrow;

void increment(inout dataset<Entry> rows) on compute {
    parallel (row in rows) { row.value += 1; }
}
void prepare(write dataset<Entry> rows) on cpu {
    for (row in rows) {
        row.value = ordinal(row);
        row.tag = ordinal(row) ^ 305419896;
    }
}
void advance(inout dataset<Entry> rows) on cpu { target(cpu, increment(rows)); }
u32 total(read dataset<Entry> rows) on cpu {
    u32 result = 0;
    for (row in rows) { result += row.value; }
    return result;
}
u32 main() on cpu {
    let rows = dataset<Entry>(1000000);
    prepare(rows);
    advance(rows);
    if (!println(total(rows)) || !println(rows.length)) { return 1; }
    return 0;
}
