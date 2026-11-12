module bootstrap.type_layout;

extern "C" VectorLayout vector(u32 scalar_size, u32 scalar_alignment, u64 count) on cpu {
    u32 lanes = count == 2 ? 2 : 4;
    return VectorLayout(u64(scalar_size) * count, scalar_alignment * lanes);
}

extern "C" u64 natural_size(u32 size, u32 alignment) on cpu requires nonzero(alignment) {
    u32 remainder = size % alignment;
    if (remainder == 0) {
        return u64(size);
    }
    return u64(size) + u64(alignment - remainder);
}

extern "C" FieldPlacement place(u32 size, u32 alignment, u32 field_size, u32 field_alignment)
    on cpu requires nonzero(field_alignment) {
    u64 offset = natural_size(size, field_alignment);
    u32 record_alignment = alignment;
    if (field_alignment > alignment) {
        record_alignment = field_alignment;
    }
    return FieldPlacement(offset, offset + u64(field_size), record_alignment);
}

extern "C" bool accepts_stride(u32 size, u32 alignment, u32 stride)
    on cpu requires nonzero(alignment) {
    return stride >= size && stride % alignment == 0;
}

extern "C" ArrayLayout fixed_array(u32 element_size, u32 alignment, u32 count)
    on cpu requires nonzero(alignment) {
    u64 stride = natural_size(element_size, alignment);
    return ArrayLayout(stride * u64(count), stride);
}

extern "C" VariantLayout tagged_variant(u32 payload_size, u32 payload_alignment) on cpu {
    u32 alignment = 4;
    if (payload_alignment > alignment) {
        alignment = payload_alignment;
    }
    return VariantLayout(u64(alignment) + natural_size(payload_size, alignment), alignment);
}
