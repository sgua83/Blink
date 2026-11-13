module bootstrap.type_value;

bool word<const N:u32 in {3,4,9}>(Text text, read array<u8,N> expected) on cpu {
    if (text.length != u64(N)) { return false; }
    for (index in 0..N) {
        if (unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+u64(index))) != expected[index]) { return false; }
    }
    return true;
}
bool plain_void(Type value, bool canonical) on cpu {
    return value.arguments == 0u64 && value.flags == 0 && (!canonical || value.canonical == 0) && word(value.name,"void");
}
bool is_shape(Type value) on cpu {
    return address_bits(value.value) == 0u64 || (value.arguments == 0u64 && (value.flags & 7) == 0);
}

extern "C" bool resolve(Type value, BuiltinKind kind, Host host, address context) on cpu
    requires nonzero(host.failure) requires nonzero(host.function) requires nonzero(host.builtin)
    requires nonzero(host.unit) requires nonzero(host.view) requires nonzero(host.argument)
    requires nonzero(host.child) requires nonzero(host.dataset) requires nonzero(host.cleanup)
    requires nonzero(host.owner) requires nonzero(host.enter_cleanup) requires nonzero(host.specialize_cleanup)
    requires nonzero(host.bind) requires nonzero(host.leave_cleanup) requires nonzero(host.owner_matches)
    requires nonzero(host.use_owner) requires nonzero(host.save_payload) requires nonzero(host.result)
    requires nonzero(host.shape_argument) requires nonzero(host.array_layout) requires nonzero(host.array)
    requires nonzero(host.component_offset) requires nonzero(host.array_component) requires nonzero(host.record)
    requires nonzero(host.family) requires nonzero(host.type_argument) requires nonzero(host.const_domain)
    requires nonzero(host.const_argument) requires nonzero(host.arguments_done) requires nonzero(host.enter_record)
    requires nonzero(host.leave_record) requires nonzero(host.simple_enum) requires nonzero(host.begin_variants)
    requires nonzero(host.unique_variant_name) requires nonzero(host.variant_tag) requires nonzero(host.unique_variant_tag)
    requires nonzero(host.variant) requires nonzero(host.variants_done) requires nonzero(host.begin_record)
    requires nonzero(host.unique_field) requires nonzero(host.dataset_field) requires nonzero(host.reference)
    requires nonzero(host.field_count) requires nonzero(host.reference_dataset) requires nonzero(host.field)
    requires nonzero(host.record_done) requires nonzero(host.record_stride) requires nonzero(host.stride_accepts)
    requires nonzero(host.set_stride) requires nonzero(host.use_record) requires nonzero(host.facts)
    requires nonzero(host.task) requires nonzero(host.views) {
    if ((value.flags & 16) != 0 && kind != BuiltinKind::CFunction) {
        host.failure(context,Site::Root,0u64,Problem::Contracts,0u64); return false;
    }
    if (kind == BuiltinKind::CFunction && (value.flags & 7) == 0) { return host.function(context); }
    if (word(value.name,"texture2d") || word(value.name,"texture3d")) {
        host.failure(context,Site::Root,0u64,Problem::Texture,0u64); return false;
    }
    if (kind == BuiltinKind::Void) { host.failure(context,Site::Root,0u64,Problem::Void,0u64); return false; }
    if (kind == BuiltinKind::Never) { host.failure(context,Site::Root,0u64,Problem::Never,0u64); return false; }
    if ((value.flags & 4) != 0) { host.failure(context,Site::Root,0u64,Problem::Tile,0u64); return false; }
    if ((value.flags & 2) != 0) {
        // The original input remains borrowed; only this actual wrapper view changes.
        Type element=value; element.flags &= ~2;
        if (!resolve(element,kind,host,context)) { return false; }
        host.view(context); return true;
    }
    if (kind == BuiltinKind::Task || kind == BuiltinKind::TaskStartError) {
        if (value.arguments != 1u64) { host.failure(context,Site::Root,0u64,Problem::TaskArguments,0u64); return false; }
        Type payload=host.argument(value.value,0u64);
        if (address_bits(payload.value) == 0u64) { host.failure(context,Site::Root,0u64,Problem::TaskArguments,0u64); return false; }
        if (kind == BuiltinKind::Task && plain_void(payload,false)) { host.unit(context); }
        else { if (!host.child(context,payload,Site::Root,0u64)) { return false; } }
        if ((payload.flags & 1) != 0) {
            if (!host.dataset(context,payload,Site::Root,0u64)) { return false; }
        }
        if (!host.task(context,kind == BuiltinKind::TaskStartError ? 1 : 0)) {
            host.failure(context,Site::Root,0u64,Problem::TaskOverflow,0u64); return false;
        }
        return true;
    }
    if (kind == BuiltinKind::Owned) {
        if (value.arguments != 2u64) { host.failure(context,Site::Root,0u64,Problem::OwnedArguments,0u64); return false; }
        Type payload=host.argument(value.value,0u64);
        Type release=host.argument(value.value,1u64);
        if (address_bits(payload.value) == 0u64 || address_bits(release.value) == 0u64) {
            host.failure(context,Site::Root,0u64,Problem::OwnedArguments,0u64); return false;
        }
        if ((release.flags & 7) != 0 || release.arguments != 0u64) {
            host.failure(context,Site::Root,0u64,Problem::ReleaseForm,0u64); return false;
        }
        Cleanup cleanup=host.cleanup(context,release);
        if (cleanup.ready == 0) { return false; }
        if (cleanup.ordinary == 0 || cleanup.cpu == 0 || cleanup.parameters != 1u64 || cleanup.value_parameter == 0) {
            host.failure(context,Site::Root,0u64,Problem::ReleaseShape,0u64); return false;
        }
        if (!host.child(context,payload,Site::Root,0u64)) { return false; }
        if ((payload.flags & 1) != 0) {
            if (host.views(context)) { host.failure(context,Site::Root,0u64,Problem::DatasetView,0u64); return false; }
            if (!host.dataset(context,payload,Site::Root,0u64)) { return false; }
        }
        host.owner(context);
        if (!host.enter_cleanup(context)) { host.failure(context,Site::Root,0u64,Problem::OwnedCycle,0u64); return false; }
        if (!host.specialize_cleanup(context)) { return false; }
        Type result=host.bind(context,Binding::CleanupResult,0u64);
        if (address_bits(result.value) == 0u64) { return false; }
        if (!plain_void(result,true)) { host.failure(context,Site::Root,0u64,Problem::ReleaseResult,0u64); return false; }
        Type parameter=host.bind(context,Binding::CleanupParameter,0u64);
        if (address_bits(parameter.value) == 0u64) { return false; }
        if (!host.child(context,parameter,Site::CleanupParameter,0u64)) { return false; }
        if ((parameter.flags & 1) != 0) {
            if (!host.dataset(context,parameter,Site::CleanupParameter,0u64)) { return false; }
        }
        host.leave_cleanup(context);
        if (!host.owner_matches(context)) { host.failure(context,Site::Root,0u64,Problem::ReleasePayload,0u64); return false; }
        host.use_owner(context); return true;
    }
    if (kind == BuiltinKind::Result) {
        if (value.arguments != 2u64) { host.failure(context,Site::Root,0u64,Problem::ResultArguments,0u64); return false; }
        u64 index=0u64;
        while (index < 2u64) {
            Type payload=host.argument(value.value,index);
            if (address_bits(payload.value) == 0u64) { host.failure(context,Site::Root,0u64,Problem::ResultType,0u64); return false; }
            if ((payload.flags & 4) != 0) { host.failure(context,Site::Root,0u64,Problem::ResultTile,0u64); return false; }
            if (plain_void(payload,true)) { host.unit(context); }
            else { if (!host.child(context,payload,Site::Root,0u64)) { return false; } }
            if ((payload.flags & 1) != 0) {
                if (host.views(context)) { host.failure(context,Site::Root,0u64,Problem::DatasetViewSpaced,0u64); return false; }
                if (!host.dataset(context,payload,Site::Root,0u64)) { return false; }
            }
            host.save_payload(context); index+=1u64;
        }
        if (!host.result(context)) { host.failure(context,Site::Root,0u64,Problem::ResultOverflow,0u64); return false; }
        return true;
    }
    if (kind == BuiltinKind::Array) {
        if (value.arguments != 2u64) { host.failure(context,Site::Root,0u64,Problem::ArrayArguments,0u64); return false; }
        Type element=host.argument(value.value,0u64);
        if (address_bits(element.value) == 0u64) { host.failure(context,Site::Root,0u64,Problem::ArrayType,0u64); return false; }
        if ((element.flags & 4) != 0) { host.failure(context,Site::Root,0u64,Problem::ArrayTile,0u64); return false; }
        if (!host.child(context,element,Site::Root,0u64)) { return false; }
        if ((element.flags & 1) != 0) {
            if (host.views(context)) { host.failure(context,Site::Root,0u64,Problem::DatasetViewSpaced,0u64); return false; }
            if (!host.dataset(context,element,Site::Root,0u64)) { return false; }
        }
        Facts actual=host.facts(context);
        Type length=host.argument(value.value,1u64);
        if (!is_shape(length)) { host.failure(context,Site::Root,0u64,Problem::ArrayLength,0u64); return false; }
        Value count=host.shape_argument(context,1u64);
        if (count.ready == 0) { return false; }
        Extent extent=host.array_layout(context,count.value);
        if (extent.size > 4294967295u64) { host.failure(context,Site::Root,0u64,Problem::ArrayOverflow,0u64); return false; }
        host.array(context,count.value,u32(extent.size));
        if (actual.components != 0u64) {
            u32 item=0;
            while (item < count.value) {
                u64 component=0u64;
                while (component < actual.components) {
                    u32 offset=item*u32(extent.stride)+host.component_offset(context,component);
                    host.array_component(context,component,offset); component+=1u64;
                }
                item+=1;
            }
        }
        return true;
    }
    if (kind == BuiltinKind::Numeric || kind == BuiltinKind::Address || kind == BuiltinKind::Handle || kind == BuiltinKind::RayHit) {
        if (value.arguments != 0u64) { host.failure(context,Site::Root,0u64,Problem::BuiltinArguments,0u64); return false; }
        host.builtin(context); return true;
    }
    Record record=host.record(context);
    if (record.ready == 0) { return false; }
    if (record.types+record.values != value.arguments) { host.failure(context,Site::Root,0u64,Problem::RecordArguments,0u64); return false; }
    u64 index=0u64;
    while (index < record.types) {
        Type argument=host.argument(value.value,index);
        if (address_bits(argument.value) == 0u64) { host.failure(context,Site::Root,0u64,Problem::RecordType,0u64); return false; }
        if ((argument.flags & 4) != 0) { host.failure(context,Site::Root,0u64,Problem::RecordTile,0u64); return false; }
        if (plain_void(argument,true)) { host.unit(context); }
        else { if (!host.child(context,argument,Site::Root,0u64)) { return false; } }
        if ((argument.flags & 1) != 0) {
            if (host.views(context)) { host.failure(context,Site::Root,0u64,Problem::DatasetView,0u64); return false; }
            if (!host.dataset(context,argument,Site::Root,0u64)) { return false; }
        }
        Value accepted=host.family(context,index);
        if (accepted.ready == 0) { return false; }
        if (accepted.value == 0) { host.failure(context,Site::Root,index,Problem::Family,0u64); return false; }
        host.type_argument(context,index); index+=1u64;
    }
    index=0u64;
    while (index < record.values) {
        Type argument=host.argument(value.value,record.types+index);
        if (!is_shape(argument)) { host.failure(context,Site::Root,0u64,Problem::RecordValue,0u64); return false; }
        Value constant=host.shape_argument(context,record.types+index);
        if (constant.ready == 0) { return false; }
        if (!host.const_domain(context,index,constant.value)) { host.failure(context,Site::Root,index,Problem::RecordDomain,u64(constant.value)); return false; }
        host.const_argument(context,index,constant.value); index+=1u64;
    }
    host.arguments_done(context);
    if (!host.enter_record(context)) { host.failure(context,Site::Root,0u64,Problem::RecordCycle,0u64); return false; }
    if (record.enumeration != 0) {
        Type representation=host.bind(context,Binding::Representation,0u64);
        if (address_bits(representation.value) == 0u64) { return false; }
        if (!host.child(context,representation,Site::Record,0u64)) { return false; }
        Facts actual=host.facts(context);
        if (actual.numeric == 0 || actual.components != 1u64 || actual.integer == 0) { host.failure(context,Site::Record,0u64,Problem::EnumScalar,0u64); return false; }
        if (record.variants == 0u64) { host.simple_enum(context); host.leave_record(context); return true; }
        if (!word(actual.name,"u32") && !word(actual.name,"i32")) { host.failure(context,Site::Record,0u64,Problem::VariantScalar,0u64); return false; }
        host.begin_variants(context);
        index=0u64;
        while (index < record.variants) {
            if (!host.unique_variant_name(context,index)) { host.failure(context,Site::Variant,index,Problem::DuplicateVariantName,0u64); return false; }
            Value tag=host.variant_tag(context,index);
            if (tag.ready == 0) { return false; }
            if (!host.unique_variant_tag(context,tag.value)) { host.failure(context,Site::Variant,index,Problem::DuplicateTag,0u64); return false; }
            Type payload=host.bind(context,Binding::Variant,index);
            if (address_bits(payload.value) == 0u64) { return false; }
            if ((payload.flags & 4) != 0) { host.failure(context,Site::Variant,index,Problem::VariantTile,0u64); return false; }
            if (plain_void(payload,true)) { host.unit(context); }
            else { if (!host.child(context,payload,Site::Variant,index)) { return false; } }
            if ((payload.flags & 1) != 0) {
                if (host.views(context)) { host.failure(context,Site::Variant,index,Problem::DatasetView,0u64); return false; }
                if (!host.dataset(context,payload,Site::Variant,index)) { return false; }
            }
            host.variant(context,index,tag.value); index+=1u64;
        }
        if (!host.variants_done(context)) { host.failure(context,Site::Record,0u64,Problem::VariantOverflow,0u64); return false; }
        host.leave_record(context); host.use_record(context); return true;
    }
    host.begin_record(context);
    index=0u64;
    while (index < record.fields) {
        if (!host.unique_field(context,index)) { host.failure(context,Site::Field,index,Problem::DuplicateField,0u64); return false; }
        Type field=host.bind(context,Binding::Field,index);
        if (address_bits(field.value) == 0u64) { return false; }
        if ((field.flags & 1) != 0) {
            Type element=host.dataset_field(context);
            if (!host.reference(context,element,Site::Field,index)) { return false; }
            Count count=host.field_count(context,index);
            if (count.ready == 0) { return false; }
            host.reference_dataset(context,count);
        } else { if (!host.child(context,field,Site::Field,index)) { return false; } }
        if (!host.field(context,index)) { host.failure(context,Site::Field,index,Problem::FieldOverflow,0u64); return false; }
        index+=1u64;
    }
    if (!host.record_done(context)) { host.failure(context,Site::Root,0u64,Problem::AlignedOverflow,0u64); return false; }
    if (record.stride != 0) {
        Value stride=host.record_stride(context);
        if (stride.ready == 0) { return false; }
        if (!host.stride_accepts(context,stride.value)) { host.failure(context,Site::Record,0u64,Problem::Stride,u64(stride.value)); return false; }
        host.set_stride(context,stride.value);
    }
    host.leave_record(context); host.use_record(context); return true;
}
