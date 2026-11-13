module bootstrap.build_digest;

// The caller borrows the input and owns the 32 output bytes.
extern "C" void sha256(address input,u64 length,address output) on cpu;
