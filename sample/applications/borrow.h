module applications.borrow;

struct Entry { u32 value; u32 tag; };
void increment(inout dataset<Entry> rows) on compute;
u32 main() on cpu;
