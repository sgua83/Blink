module bootstrap.archive;
// Windows x64 C ABI: six and three 8-byte fields, respectively.
const u64 MEMBER_BYTES=48u64;
const u64 SYMBOL_BYTES=24u64;
struct ArchiveMember {
    address name;u64 name_length;
    address bytes;u64 byte_length;
    u64 offset;u64 name_offset;
};
struct ArchiveSymbol {address name;u64 name_length;u64 member;};
struct ArchiveInput {address members;u64 member_count;address symbols;u64 symbol_count;};
struct ArchiveLayout {u64 length;u64 first;u64 second;u64 names;u32 error;};
extern "C" ArchiveLayout layout(ArchiveInput input) on cpu;
extern "C" void encode(ArchiveInput input,ArchiveLayout layout,address out) on cpu;
