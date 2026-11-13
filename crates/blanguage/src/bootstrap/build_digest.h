module bootstrap.build_digest;

// The caller borrows the input and owns the 32 output bytes.
extern "C" void sha256(address input,u64 length,address output) on cpu;

struct DigestField { address bytes; u64 length; };
extern "C" void sha256_fields(address fields,u64 count,address output) on cpu;

// One caller-owned state per input stream. Update borrows the bytes only for the call.
struct DigestState {
    array<u32,8> words;
    array<u64,16> tail;
    u64 pending;
    u64 length;
};
extern "C" DigestState sha256_start() on cpu;
extern "C" void sha256_update(inout DigestState state,address input,u64 length) on cpu;
extern "C" void sha256_finish(inout DigestState state,address output) on cpu;
