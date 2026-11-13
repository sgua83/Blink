module bootstrap.c_function;

bool word<const N:u32 in {4,5}>(Text text, read array<u8,N> name) on cpu {
    if (text.length != u64(N)) { return false; }
    for (index in 0..N) {
        if (unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+u64(index))) != name[index]) { return false; }
    }
    return true;
}

extern "C" void reference(Type source, address context,
    cfn<Type,address,u64> argument,
    cfn<bool,address,address> bind_result,
    cfn<void,address,u64> reserve,
    cfn<bool,address,u64,Access,address> bind_parameter,
    cfn<void,address,Problem> fail,
    cfn<void,address> finish) on cpu
    requires nonzero(argument) requires nonzero(bind_result) requires nonzero(reserve)
    requires nonzero(bind_parameter) requires nonzero(fail) requires nonzero(finish) {
    if (source.arguments == 0u64) { fail(context,Problem::MissingResult); return; }
    Type result=argument(source.reference,0u64);
    if (address_bits(result.reference) == 0u64) { fail(context,Problem::TypeArgument); return; }
    if (!bind_result(context,result.reference)) { return; }
    reserve(context,source.arguments-1u64);
    u64 index=1u64;
    while (index < source.arguments) {
        Type value=argument(source.reference,index);
        if (address_bits(value.reference) == 0u64) { fail(context,Problem::TypeArgument); return; }
        Access access=Access::Value;
        if (value.wrappers == 0) {
            if (word(value.name,"read")) { access=Access::Read; }
            else if (word(value.name,"write")) { access=Access::Write; }
            else if (word(value.name,"inout")) { access=Access::InOut; }
        }
        if (access != Access::Value) {
            if (value.arguments != 1u64) { fail(context,Problem::Position); return; }
            value=argument(value.reference,0u64);
            if (address_bits(value.reference) == 0u64) { fail(context,Problem::Position); return; }
        }
        if (!bind_parameter(context,index-1u64,access,value.reference)) { return; }
        index+=1u64;
    }
    finish(context);
}

extern "C" void declaration(Type result, u64 count, address context,
    cfn<Parameter,address,u64> parameter,
    cfn<bool,address,address> bind_result,
    cfn<bool,address,u64,Access,address> bind_parameter,
    cfn<void,address> finish) on cpu
    requires nonzero(parameter) requires nonzero(bind_result)
    requires nonzero(bind_parameter) requires nonzero(finish) {
    if (!bind_result(context,result.reference)) { return; }
    u64 index=0u64;
    while (index < count) {
        Parameter value=parameter(context,index);
        if (!bind_parameter(context,index,value.access,value.value.reference)) { return; }
        index+=1u64;
    }
    finish(context);
}

bool ordinary(Layout value, address context, cfn<void,address,Problem> fail) on cpu requires nonzero(fail) {
    if (value.dataset != 0 || value.owned != 0) { fail(context,Problem::Resource); return false; }
    if (value.views != 0 || value.empty != 0) { fail(context,Problem::EmptyOrView); return false; }
    return true;
}
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
    requires nonzero(finish) {
    u64 index=0u64;
    while (index < count) {
        ParameterLayout value=parameter(context,index);
        if (value.value.present == 0 || !ordinary(value.value,context,fail)) { return; }
        if (value.access == Access::Value) { append_value(context); }
        else { append_reference(context,value.access); }
        index+=1u64;
    }
    bool plain=result.arguments == 0u64 && result.wrappers == 0;
    bool never=plain && (result.decorations & 3) == 0 && word(result.name,"never");
    bool empty=plain && result.decorations == 0 && word(result.name,"void");
    if (!never && !empty) {
        Layout value=result_layout(context);
        if (value.present == 0 || !ordinary(value,context,fail)) { return; }
        use_result(context);
    }
    finish(context,never ? 1 : 0);
}
