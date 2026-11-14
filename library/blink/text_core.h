module blink.text_core;

// input borrows count bytes; output has room for twice count bytes and does
// not overlap input. The synchronous encoder writes ASCII without retaining
// either address or owning a Frame.
extern "C" void write_hexadecimal(address input,u64 count,address output) on cpu;
