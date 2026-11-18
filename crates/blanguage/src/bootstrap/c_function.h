module bootstrap.c_function;

enum Access:u32 { Value=0; Read=1; Write=2; InOut=3; Atomic=4; };
struct Text { address bytes; u64 length; };
struct Type { address reference; Text name; u64 arguments; u32 wrappers; u32 decorations; };
struct Parameter { Access access; Type value; };
struct Layout { u32 dataset; u32 owned; u32 views; u32 empty; };
enum Problem:u32 { MissingResult=0; TypeArgument=1; Position=2; Resource=3; EmptyOrView=4; };
enum BindingNeed:u32 { Done=0; Result=1; AppendParameter=2; ReplaceParameter=3; };
struct BindingCursor { address source; address current; u64 count; u64 index; Access access; u32 mode; u32 stage; };
struct BindingHost {
    cfn<Type,address,u64> argument;
    cfn<void,address,u64> reserve;
    cfn<Parameter,address,u64> parameter;
    cfn<void,address,Problem> fail;
};
enum SignatureNeed:u32 { Done=0; Parameter=1; Result=2; ResultName=3; };
struct ResultShape { Text name; u64 arguments; u32 wrappers; u32 decorations; u32 declaration; };
struct SignatureCursor { u64 count; u64 index; Layout layout; Access access; u32 stage; u32 never; };
struct SignatureHost {
    cfn<void,address> append_value;
    cfn<void,address,Access> append_reference;
    cfn<void,address> use_result;
    cfn<void,address,Problem> fail;
};
extern "C" BindingCursor reference_start(address source,u64 count) on cpu;
extern "C" BindingCursor declaration_start(address result,u64 count) on cpu;
extern "C" BindingNeed binding_step(address state,address context,BindingHost host) on cpu
    requires nonzero(host.argument) requires nonzero(host.reserve)
    requires nonzero(host.parameter) requires nonzero(host.fail);
extern "C" SignatureCursor signature_start(u64 count) on cpu;
extern "C" SignatureNeed signature_step(address state,ResultShape result,address context,SignatureHost host) on cpu
    requires nonzero(host.append_value) requires nonzero(host.append_reference)
    requires nonzero(host.use_result) requires nonzero(host.fail);
