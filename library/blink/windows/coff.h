module blink.windows.coff;
// Raw borrowed AMD64 COFF views. The original input owns every byte.
// Derived tables belong to that same unchanged input; callers keep it alive
// and use section/symbol indices within those tables. No Frame is retained.
struct ReadBytes {address bytes;u64 length;};
struct ReadRange {ReadBytes bytes;u32 error;};
struct ReadBuild {ReadBytes bytes;u64 present;u32 error;};
struct ReadSymbols {ReadBytes strings;u64 offset;u64 count;u32 error;};
struct ReadSymbol {
    ReadBytes name;i64 section;u64 value;u64 storage;u64 function;
    u64 auxiliary;u64 selection;u64 owner;u64 next;u32 error;
};
struct ReadSection {
    ReadBytes name;ReadBytes bytes;u64 flags;u64 relocations;u64 relocation_count;u64 zero_fill;u32 error;
};
struct ReadRelocations {u64 offset;u64 count;u32 error;};
struct ReadRelocation {u64 offset;u64 symbol;u64 kind;u32 error;};
extern "C" ReadRange headers(ReadBytes input) on cpu;
extern "C" ReadBuild build_inputs(ReadBytes input,ReadBytes headers) on cpu;
extern "C" ReadSymbols symbols(ReadBytes input) on cpu;
extern "C" ReadSymbol symbol(ReadBytes input,ReadSymbols table,u64 index) on cpu;
extern "C" ReadRange next_export(ReadBytes input, ReadSymbols table, inout u64 index) on cpu;
extern "C" ReadSection section(ReadBytes input,ReadBytes headers,u64 index) on cpu;
extern "C" ReadRelocations relocations(ReadBytes input,u64 offset,u64 count,u64 flags) on cpu;
extern "C" ReadRelocation relocation(ReadBytes input,u64 offset,u64 index) on cpu;
