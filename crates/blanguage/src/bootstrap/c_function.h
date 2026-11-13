module bootstrap.c_function;

// Access uses the existing native ast::Access C ABI.
enum Access:u32 { Value=0; Read=1; Write=2; InOut=3; Atomic=4; };
struct Text { address bytes; u64 length; };
struct Type { address reference; Text name; u64 arguments; u32 wrappers; u32 decorations; };
struct Parameter { Access access; Type value; };
struct Layout { u32 present; u32 dataset; u32 owned; u32 views; u32 empty; };
struct ParameterLayout { Access access; Layout value; };
enum Problem:u32 { MissingResult=0; TypeArgument=1; Position=2; Resource=3; EmptyOrView=4; };

extern "C" void reference(Type source, address context,
    cfn<Type,address,u64> argument,
    cfn<bool,address,address> bind_result,
    cfn<void,address,u64> reserve,
    cfn<bool,address,u64,Access,address> bind_parameter,
    cfn<void,address,Problem> fail,
    cfn<void,address> finish) on cpu
    requires nonzero(argument) requires nonzero(bind_result) requires nonzero(reserve)
    requires nonzero(bind_parameter) requires nonzero(fail) requires nonzero(finish);

// A declaration hands over its actual parameter Vec. Each native result replaces
// that same slot, while a borrowed cfn source appends only the final parameters.
extern "C" void declaration(Type result, u64 count, address context,
    cfn<Parameter,address,u64> parameter,
    cfn<bool,address,address> bind_result,
    cfn<bool,address,u64,Access,address> bind_parameter,
    cfn<void,address> finish) on cpu
    requires nonzero(parameter) requires nonzero(bind_result)
    requires nonzero(bind_parameter) requires nonzero(finish);

extern "C" void signature(Type result, u64 count, address context,
    cfn<ParameterLayout,address,u64> parameter,
    cfn<Layout,address> result_layout,
    cfn<void,address> append_value,
    cfn<void,address,Access> append_reference,
    cfn<void,address> use_result,
    cfn<void,address,Problem> fail,
    cfn<void,address,u32> finish) on cpu
    requires nonzero(parameter) requires nonzero(result_layout) requires nonzero(append_value)
    requires nonzero(append_reference) requires nonzero(use_result) requires nonzero(fail)
    requires nonzero(finish);
