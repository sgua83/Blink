module bootstrap.native_import;
import bootstrap.bindings;

struct NativeImportInput {BindingBytes dll;BindingBytes name;BindingBytes runtime;};
struct NativeImportOutput {
    address sections;address lengths;address relocations;address associated;address names;
};
struct NativeImportPlan {u64 slot_start;u64 slot_end;u64 member;u64 hint;u64 sections;u64 error;};
extern "C" NativeImportPlan plan_import(NativeImportInput input,NativeImportOutput output) on cpu;
extern "C" void encode_member_name(NativeImportInput input,NativeImportPlan plan,address output) on cpu;
extern "C" void encode_section(NativeImportInput input,NativeImportPlan plan,u64 section,u64 length,address output) on cpu;
