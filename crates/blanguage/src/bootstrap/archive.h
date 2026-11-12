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

struct ArchiveRange {u64 start;u64 length;};
struct ArchiveReadMember {u32 offset;ArchiveRange name;ArchiveRange body;};
struct ArchiveReadSymbol {u32 member;ArchiveRange name;};
struct ArchiveReadResult {u32 error;u64 at;};
extern "C" bool has_signature(address bytes, u64 length) on cpu;
extern "C" ArchiveReadResult decode(address bytes,u64 length,address context,
    cfn<u32,address,ArchiveReadMember> member,cfn<u32,address,ArchiveReadSymbol> symbol) on cpu
    requires nonzero(member) requires nonzero(symbol);

// One B-owned library directory. Its byte ranges borrow the caller's original
// immutable archive until library_destroy. No Rust container crosses this ABI.
struct ArchiveMemberState {ArchiveReadMember member;u32 available;};
struct ArchiveSelection {u32 result;u64 index;};
extern "C" address library_create(address bytes) on cpu;
extern "C" u32 library_add_member(address library,ArchiveReadMember member) on cpu;
extern "C" u32 library_add_symbol(address library,ArchiveReadSymbol symbol) on cpu;
extern "C" u64 library_member_count(address library) on cpu;
extern "C" ArchiveMemberState library_member(address library,u64 index) on cpu;
extern "C" ArchiveSelection library_take(address library,address name,u64 length) on cpu;
extern "C" void library_destroy(address library) on cpu;
