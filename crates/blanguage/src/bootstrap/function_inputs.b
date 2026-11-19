module bootstrap.function_inputs;

extern "C" ResourceStep resource_step(u32 stage,Facts facts) on cpu {
    u32 next = stage;
    if (next == 0) {
        if (facts.dataset != 0) { return ResourceStep(Resource::Buffer, 1); }
        next = 1;
    }
    if (next == 1) {
        if (facts.acceleration != 0) { return ResourceStep(Resource::Acceleration, 2); }
        next = 2;
    }
    if (next == 2) {
        if (facts.texture != 0) { return ResourceStep(Resource::Texture, 3); }
        next = 3;
    }
    if (next == 3) { return ResourceStep(Resource::Keep, 4); }
    return ResourceStep(Resource::Done, 4);
}

// Stage length and partition start precede the logical dataset lengths.
extern "C" u32 uniform_bytes(u32 buffers) on cpu {
    return 8 + 4 * buffers;
}

u32 align_uniform(u32 bytes, u32 alignment) on cpu {
    return (bytes + alignment - 1) & ~(alignment - 1);
}

extern "C" u32 layout_uniforms(u32 buffers, usize count, address context,
    cfn<UniformField,address,usize> ensures memory(read) field,
    cfn<void,address,usize,u32> place) on cpu
    requires nonzero(field) requires nonzero(place) {
    u32 bytes = uniform_bytes(buffers);
    usize index = 0u64;
    while (index < count) {
        UniformField value = field(context, index);
        u32 offset = align_uniform(bytes, value.alignment);
        place(context, index, offset);
        bytes = offset + value.size;
        index += 1u64;
    }
    return align_uniform(bytes, 4);
}

extern "C" Parameter parameter_form(Facts facts) on cpu {
    if (facts.acceleration != 0) { return Parameter::Acceleration; }
    if (facts.texture != 0) { return Parameter::Texture; }
    if (facts.dataset != 0) { return Parameter::Dataset; }
    if (facts.tile != 0) { return Parameter::Tile; }
    return Parameter::Ordinary;
}

extern "C" bool domain_result(u32 unit, usize arguments, u32 dataset, u32 tile, u32 view) on cpu {
    return unit != 0 && arguments == 0u64 && (dataset | tile | view) == 0;
}

extern "C" DomainParameter domain_parameter(u32 dataset, u32 resource, u32 by_value) on cpu {
    if (dataset != 0) {
        return by_value == 0 ? DomainParameter::Accept : DomainParameter::Reject;
    }
    if (resource != 0) { return DomainParameter::Accept; }
    return by_value != 0 ? DomainParameter::PlainLayout : DomainParameter::Reject;
}

extern "C" Dataset dataset_form(u32 ordinary_cpu,u32 by_value) on cpu {
    if (ordinary_cpu == 0) { return Dataset::Device; }
    if (by_value != 0) { return Dataset::Owned; }
    return Dataset::Borrowed;
}

extern "C" ValueForm value_form(Mode mode,u32 read,u32 atomic,u32 by_value,u32 owned) on cpu {
    if (read != 0 && owned != 0) { return ValueForm::OwnedRead; }
    if (atomic != 0 && mode.application == 0) { return ValueForm::AtomicParameter; }
    if (by_value == 0) { return ValueForm::Reference; }
    return ValueForm::Value;
}

extern "C" PlaceForm place_form(Mode mode,u32 read) on cpu {
    if (mode.application != 0) {
        if (mode.c_abi != 0) { return PlaceForm::CAbi; }
        return PlaceForm::Native;
    }
    if (read != 0) { return PlaceForm::LocalRead; }
    return PlaceForm::LocalWrite;
}

extern "C" Reference reference_body(Mode mode,u32 reads,u32 atomic,u32 inout,u32 owns) on cpu {
    if (atomic != 0) { return Reference(reads, RefBody::Atomic); }
    if (owns != 0) {
        if (inout != 0) { return Reference(reads, RefBody::OwnedInOut); }
        return Reference(reads, RefBody::OwnedOutput);
    }
    if (reads != 0 && mode.application != 0) { return Reference(reads, RefBody::ReadContent); }
    return Reference(reads, RefBody::Empty);
}

extern "C" u32 start_actions(Mode mode) on cpu {
    return mode.application | (mode.cooperative << 1);
}

extern "C" Prepare prepare(u32 stage,Mode mode) on cpu {
    if (stage == 0) { return Prepare::Elements; }
    if (stage == 1) { return Prepare::BindElements; }
    if (stage == 2) { return Prepare::Resources; }
    if (stage == 3) { return Prepare::Resolve; }
    if (stage == 4) { return Prepare::Assume; }
    if (stage == 5) { return Prepare::Arrays; }
    if (stage == 6) { return Prepare::Outputs; }
    if (stage == 7 && mode.application != 0 && mode.c_abi == 0) { return Prepare::Completions; }
    return Prepare::Done;
}

extern "C" u32 return_task_layout(u32 application_entry,u32 c_abi,u32 has_result) on cpu {
    return application_entry & (1 ^ c_abi) & has_result;
}

extern "C" ReferenceLayout reference_layout(ReferenceFacts facts,usize components,u32 c_abi) on cpu {
    if (c_abi != 0) { return ReferenceLayout(ReferenceForm::Whole,1u64); }
    if (components == 0u64) { return ReferenceLayout(ReferenceForm::Empty,0u64); }
    if (components > 1u64 && facts.array == 0 && facts.record == 0
        && facts.view == 0 && facts.dataset == 0 && facts.variant == 0 && facts.enumeration == 0) {
        return ReferenceLayout(ReferenceForm::Split,components);
    }
    return ReferenceLayout(ReferenceForm::Whole,1u64);
}
