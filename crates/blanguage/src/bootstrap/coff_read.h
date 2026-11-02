module bootstrap.coff_read;
// Borrowed Windows x64 C ABI views; the input snapshot owns every byte.
struct ReadBytes {address bytes;u64 length;};
struct ReadRange {ReadBytes bytes;u32 error;};
struct ReadBuild {ReadBytes bytes;u64 present;u32 error;};
struct ReadSymbols {ReadBytes strings;u64 offset;u64 count;u32 error;};
struct ReadSymbol {
    ReadBytes name;i64 section;u64 value;u64 storage;u64 function;
    u64 auxiliary;u64 selection;u64 owner;u64 next;u32 error;
};
struct ReadSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u32 error;
};
struct ReadRelocations {u64 offset;u64 count;u32 error;};
struct ReadRelocation {u64 offset;u64 symbol;u64 kind;u32 error;};
extern "C" ReadRange headers(ReadBytes input) on cpu;
extern "C" ReadBuild build_inputs(ReadBytes input,ReadBytes headers) on cpu;
extern "C" ReadSymbols symbols(ReadBytes input) on cpu;
extern "C" ReadSymbol symbol(ReadBytes input,ReadSymbols table,u64 index) on cpu;
extern "C" ReadSection section(ReadBytes input,ReadBytes headers,u64 index) on cpu;
extern "C" ReadRelocations relocations(ReadBytes input,u64 offset,u64 count,u64 flags) on cpu;
extern "C" ReadRelocation relocation(ReadBytes input,u64 offset,u64 index) on cpu;
