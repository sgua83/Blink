module bootstrap.record_layout;

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
