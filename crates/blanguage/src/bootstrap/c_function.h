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

struct NamedCursor {u64 index;u32 phase;};
struct NamedFacts {u32 present;u32 c_abi;u64 parameters;};
enum NamedNeed:u32 {Resolve=0;Unknown=1;CAbi=2;InvalidAbi=3;Arguments=4;Instance=5;Contracts=6;Result=7;Parameter=8;Bind=9;Complete=10;};
struct NamedStep {NamedCursor cursor;NamedNeed need;};
extern "C" NamedStep named_reference_step(NamedCursor cursor,NamedFacts facts) on cpu;
struct ReferenceContract {u32 present;u32 kind;u32 predicate;address name;address condition;};
struct ReferenceCondition {u32 kind;address left;address right;};
struct ReferenceHost {
 cfn<ReferenceContract,address> next;
 cfn<void,address> append;
 cfn<Text,address,u64> parameter;
 cfn<Text,address> name;
 cfn<void,address,u64,u64> rename;
 cfn<ReferenceCondition,address> condition;
};
extern "C" void reference_contracts(address context,u64 parameters,ReferenceHost host) on cpu
 requires nonzero(host.next) requires nonzero(host.append) requires nonzero(host.parameter)
 requires nonzero(host.name) requires nonzero(host.rename) requires nonzero(host.condition);
struct AddressCursor {u32 phase;};
struct AddressFacts {u32 cpu;u32 application;u32 external;u32 declaration;};
enum AddressNeed:u32 {BadTarget=0;Application=1;Signature=2;ExternalKind=3;ExternalTarget=4;NativeTarget=5;Register=6;Emit=7;Declaration=8;Reference=9;Complete=10;};
struct AddressStep {AddressCursor cursor;AddressNeed need;};
extern "C" AddressStep address_value_step(AddressCursor cursor,AddressFacts facts) on cpu;
