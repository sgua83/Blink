module bootstrap.coff;
import bootstrap.bindings;
// Windows x64 C ABI; all borrowed-view fields occupy eight bytes.
const u64 SYMBOL_BYTES=80u64;
const u64 COFF_RELOCATION_RECORD_BYTES=10u64;
struct CoffSection {
    address name;u64 name_length;u64 byte_length;u64 flags;
    u64 relocation_count;u64 uninitialized;u64 offset;u64 relocation_offset;
    address relocations;address associated;u64 associated_count;u64 owner;u64 symbol;
    address body;u64 body_kind;
};
struct CoffSymbol {
    address name;u64 name_length;u64 section;u64 value;
    u64 function;u64 external;u64 auxiliary;u64 owner;u64 generated;u64 source;
};
struct CoffInput {address sections;u64 section_count;address symbols;u64 symbol_count;};
struct CoffLayout {u64 length;u64 symbols;u64 strings;u64 symbol_count;u32 error;};
struct CoffBindingSource {
    address module_name;u64 module_name_length;address sections;address nodes;u64 names_root;u64 imports_root;
    cfn<BindingRelocation,address,u64> read_relocation;
};
struct CoffSource {
    BindingBytes name;address sections;u64 section_count;u64 bound_count;u64 definition_count;
    cfn<CoffSection,address> next_section;
    cfn<BindingDefinition,address> next_definition;
    cfn<BindingRelocation,address,u64> read_relocation;
};
struct CoffPlan {CoffInput input;CoffLayout layout;CoffBindingSource binding;};
struct CoffWork {
    address context;
    cfn<BindingStorage,address,u64,u64> reserve;
    address data_name;cfn<address,address,u64> name;
    cfn<void,address,CoffPlan> consume;
    address import_context;cfn<void,address,BindingKey> emit_import;
};
extern "C" u32 with_output(CoffSource source,CoffWork work) on cpu
    requires nonzero(source.next_definition) requires nonzero(work.reserve)
    requires nonzero(work.name) requires nonzero(work.consume);
CoffSection section_at(address sections,u64 index) on cpu;
void section_put(address sections,u64 index,CoffSection value) on cpu;
extern "C" u32 needs_body(u64 flags,u64 relocation_count) on cpu;
extern "C" void write(read CoffPlan plan,address output) on cpu;
extern "C" void host_headers(address output, u64 sections, u64 symbols, u64 cache_end, u64 cache_length) on cpu;

struct NativeObjectPlace { usize section; u32 present; };
enum NativeObjectTargetKind:u32 { Done=0; Function=1; External=2; Print=3; };
struct NativeObjectTarget {
    address symbol;
    address payload;
    address output;
    NativeObjectPlace defined;
    NativeObjectTargetKind kind;
};
struct NativeObjectTargetHost {
    cfn<NativeObjectTarget,address> next;
    cfn<u32,address,address> imported;
    cfn<usize,address,address,address> external;
    cfn<usize,address,address,address> print;
};
extern "C" address resolve_native_object_targets(address context, NativeObjectTargetHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.imported)
    requires nonzero(host.external) requires nonzero(host.print);

struct NativeObjectImage { usize section; usize fields; };
struct NativeObjectFixup { usize offset; address name; NativeObjectPlace place; };
struct NativeObjectFixupHost {
    cfn<NativeObjectImage,address,usize> image;
    cfn<NativeObjectFixup,address,usize,usize> field;
    cfn<void,address,usize,usize,usize> section;
    cfn<void,address,usize,usize,address> symbol;
};
extern "C" void bind_native_object_fixups(address context, usize images, NativeObjectFixupHost host) on cpu
    requires nonzero(host.image) requires nonzero(host.field)
    requires nonzero(host.section) requires nonzero(host.symbol);
