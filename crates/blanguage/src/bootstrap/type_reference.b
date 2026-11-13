module bootstrap.type_reference;

extern "C" void bind(Reference source, address context,
    cfn<void,address> function,
    cfn<void,address> resource,
    cfn<Parameters,address> record,
    cfn<void,address,u64> reserve,
    cfn<Argument,address,u64> argument,
    cfn<bool,address,address> append_type,
    cfn<bool,address,address> append_release,
    cfn<bool,address,u64> append_value,
    cfn<void,address,Code,Problem,u64> fail,
    cfn<void,address,u32,Name> finish) on cpu
    requires nonzero(function) requires nonzero(resource) requires nonzero(record)
    requires nonzero(reserve) requires nonzero(argument) requires nonzero(append_type)
    requires nonzero(append_release) requires nonzero(append_value) requires nonzero(fail)
    requires nonzero(finish) {
    if (source.kind == BuiltinKind::CFunction && source.wrappers == 0) { function(context); return; }
    if (source.resource != 0) { resource(context); return; }
    u64 types = 0u64;
    u64 count = 0u64;
    Code code = Code::Ordinary;
    u32 canonical = 0;
    if (source.kind == BuiltinKind::Array) { types=1u64; count=2u64; code=Code::Array; }
    else if (source.kind == BuiltinKind::Result) { types=2u64; count=2u64; code=Code::Result; }
    else if (source.kind == BuiltinKind::Owned) { types=1u64; count=2u64; code=Code::Owned; }
    else if (source.kind == BuiltinKind::Task || source.kind == BuiltinKind::TaskStartError) { types=1u64; count=1u64; code=Code::Owned; }
    else if (source.kind == BuiltinKind::Void || source.kind == BuiltinKind::Never) { code=Code::Void; }
    else if (source.kind == BuiltinKind::Missing || source.kind == BuiltinKind::CFunction) {
        // A resource-wrapped cfn follows the original nominal lookup path.
        Parameters parameters=record(context);
        if (parameters.present == 0) { return; }
        types=parameters.types; count=parameters.count; canonical=1;
    }
    if (source.arguments != count) { fail(context,code,Problem::Arity,count); return; }
    reserve(context,count);
    u64 index=0u64;
    while (index < count) {
        if (source.kind == BuiltinKind::Owned && index == 1u64) {
            Argument release=argument(context,index);
            if (address_bits(release.reference) == 0u64) { fail(context,Code::Owned,Problem::ReleaseType,0u64); return; }
            if (release.wrappers != 0 || release.arguments != 0u64) { fail(context,Code::Owned,Problem::ReleaseForm,0u64); return; }
            if (!append_release(context,release.reference)) { return; }
        } else if (index < types) {
            Argument value=argument(context,index);
            if (address_bits(value.reference) == 0u64) { fail(context,code,Problem::Type,0u64); return; }
            if (!append_type(context,value.reference)) { return; }
        } else {
            if (!append_value(context,index)) { return; }
        }
        index+=1u64;
    }
    // The count expression is evaluated only after all actual arguments.
    finish(context,canonical,source.name);
}
