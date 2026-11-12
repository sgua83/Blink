module bootstrap.type_layout;

struct VectorLayout {
    u64 size;
    u32 alignment;
};

extern "C" VectorLayout vector(u32 scalar_size, u32 scalar_alignment, u64 count) on cpu;

struct FieldPlacement {
    u64 offset;
    u64 end;
    u32 alignment;
};

extern "C" FieldPlacement place(u32 size, u32 alignment, u32 field_size, u32 field_alignment)
    on cpu requires nonzero(field_alignment);
extern "C" u64 natural_size(u32 size, u32 alignment) on cpu requires nonzero(alignment);
extern "C" bool accepts_stride(u32 size, u32 alignment, u32 stride)
    on cpu requires nonzero(alignment);

struct ArrayLayout {
    u64 size;
    u64 stride;
};

extern "C" ArrayLayout fixed_array(u32 element_size, u32 alignment, u32 count)
    on cpu requires nonzero(alignment);

struct VariantLayout {
    u64 size;
    u32 alignment;
};

extern "C" VariantLayout tagged_variant(u32 payload_size, u32 payload_alignment) on cpu;
