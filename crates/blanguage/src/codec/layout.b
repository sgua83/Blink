module codec.layout;

u32 up(u32 n, u32 a) on cpu {
    return (n + a - 1) & ~(a - 1);
}

u32 words(u32 size) on cpu {
    return size / 4 + u32(size % 4 != 0);
}

void put<const N:u32>(LayoutSink sink, address out, array<u8,N> value) on cpu
    requires nonzero(sink.text) {
    sink.text(out, LayoutText(address_of(value[0]), u64(N)));
}

void emit_header(address out, address value, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) {
    LayoutNode node = source.node(value);
    put(sink, out, "{\"name\":");
    sink.string(out, node.name);
    put(sink, out, ",\"kind\":");
    if ((node.flags & LAYOUT_DATASET) != 0) { put(sink, out, "\"dataset_reference\""); }
    else if ((node.flags & LAYOUT_VIEW) != 0) { put(sink, out, "\"view_reference\""); }
    else if ((node.flags & LAYOUT_ARRAY) != 0) { put(sink, out, "\"array\""); }
    else if ((node.flags & LAYOUT_RESULT) != 0) { put(sink, out, "\"result\""); }
    else if ((node.flags & LAYOUT_VARIANT) != 0) { put(sink, out, "\"variant\""); }
    else if ((node.flags & LAYOUT_ENUMERATION) != 0) { put(sink, out, "\"enumeration\""); }
    else if ((node.flags & LAYOUT_CLEANUP) != 0) { put(sink, out, "\"owned\""); }
    else if ((node.flags & LAYOUT_FUNCTION) != 0) { put(sink, out, "\"c_function\""); }
    else if ((node.flags & LAYOUT_VOID) != 0) { put(sink, out, "\"unit\""); }
    else if ((node.flags & LAYOUT_RECORD) != 0) { put(sink, out, "\"record\""); }
    else { put(sink, out, "\"value\""); }

    put(sink, out, ",\"size_bytes\":");
    sink.number(out, u64(node.size));
    put(sink, out, ",\"align_bytes\":");
    sink.number(out, u64(node.align));
    put(sink, out, ",\"stride_bytes\":");
    sink.number(out, u64(up(node.size, node.align)));
    put(sink, out, ",\"resource_word_bytes\":4,\"resource_word_count\":");
    sink.number(out, u64(words(node.size)));
    put(sink, out, ",\"type_arguments\":[");
    u64 i = 0u64;
    while (i < node.arguments) {
        if (i != 0u64) { put(sink, out, ","); }
        sink.string(out, source.argument(value, i));
        i += 1u64;
    }
    put(sink, out, "],\"const_arguments\":[");
    i = 0u64;
    while (i < node.values) {
        if (i != 0u64) { put(sink, out, ","); }
        sink.number(out, u64(source.value(value, i)));
        i += 1u64;
    }
    put(sink, out, "],\"components\":[");
    i = 0u64;
    while (i < node.components) {
        if (i != 0u64) { put(sink, out, ","); }
        LayoutComponent c = source.component(value, i);
        put(sink, out, "{\"scalar\":");
        sink.string(out, c.scalar);
        put(sink, out, ",\"offset_bytes\":");
        sink.number(out, u64(c.offset));
        put(sink, out, ",\"size_bytes\":");
        sink.number(out, u64(c.size));
        put(sink, out, ",\"bit_width\":");
        sink.number(out, u64(c.bit_width));
        put(sink, out, ",\"first_resource_word\":");
        sink.number(out, u64(c.offset / 4));
        put(sink, out, ",\"byte_in_word\":");
        sink.number(out, u64(c.offset % 4));
        put(sink, out, ",\"resource_word_count\":");
        sink.number(out, u64((c.offset % 4 + c.size + 3) / 4));
        put(sink, out, "}");
        i += 1u64;
    }
    put(sink, out, "]");
    if ((node.components == 0u64 && ((node.flags & LAYOUT_ARRAY) != 0 || (node.flags & LAYOUT_RECORD) != 0))
        || ((node.flags & LAYOUT_VARIANT) != 0 && node.components == 1u64 && node.size > node.align)) {
        put(sink, out, ",\"component_layout\":\"structural\"");
    }
    put(sink, out, ",\"fields\":[");
}

LayoutRequest walk(address out, u32 expand, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup) {
    while (true) {
        LayoutPart part = sink.pop(out);
        address value = part.value;
        usize i = part.index;
        if (part.kind == LayoutPartKind::Done) {
            return LayoutRequest(LayoutNeed::Done, value);
        }
        if (part.kind == LayoutPartKind::Value) {
            emit_header(out, value, sink, source);
            sink.push(out, LayoutPart(LayoutPartKind::Fields, value, 0u64));
        } else if (part.kind == LayoutPartKind::Fields) {
            LayoutField f = source.field(value, i);
            if (address_bits(f.value) != 0u64) {
                if (i != 0u64) { put(sink, out, ","); }
                put(sink, out, "{\"name\":");
                sink.string(out, f.name);
                put(sink, out, ",\"offset_bytes\":");
                sink.number(out, u64(f.offset));
                put(sink, out, ",\"layout\":");
                sink.push(out, LayoutPart(LayoutPartKind::Fields, value, i+1u64));
                sink.push(out, LayoutPart(LayoutPartKind::Close, value, 0u64));
                sink.push(out, LayoutPart(LayoutPartKind::Value, f.value, 0u64));
            } else {
                put(sink, out, "],\"variants\":[");
                sink.push(out, LayoutPart(LayoutPartKind::Variants, value, 0u64));
            }
        } else if (part.kind == LayoutPartKind::Variants) {
            LayoutVariant v = source.variant(value, i);
            if (address_bits(v.value) != 0u64) {
                if (i != 0u64) { put(sink, out, ","); }
                put(sink, out, "{\"name\":");
                sink.string(out, v.name);
                put(sink, out, ",\"discriminant\":");
                sink.number(out, u64(v.discriminant));
                put(sink, out, ",\"payload_offset_bytes\":");
                sink.number(out, u64(v.offset));
                put(sink, out, ",\"layout\":");
                sink.push(out, LayoutPart(LayoutPartKind::Variants, value, i+1u64));
                sink.push(out, LayoutPart(LayoutPartKind::Close, value, 0u64));
                sink.push(out, LayoutPart(LayoutPartKind::Value, v.value, 0u64));
            } else {
                put(sink, out, "],\"array\":");
                sink.push(out, LayoutPart(LayoutPartKind::Array, value, 0u64));
            }
        } else if (part.kind == LayoutPartKind::Array) {
            sink.push(out, LayoutPart(LayoutPartKind::View, value, 0u64));
            address element = source.array(value);
            if (address_bits(element) != 0u64) {
                LayoutNode child = source.node(element);
                put(sink, out, "{\"count\":");
                sink.number(out, u64(source.value(value, 0u64)));
                put(sink, out, ",\"element_stride_bytes\":");
                sink.number(out, u64(up(child.size, child.align)));
                put(sink, out, ",\"element\":");
                sink.push(out, LayoutPart(LayoutPartKind::Close, value, 0u64));
                sink.push(out, LayoutPart(LayoutPartKind::Value, element, 0u64));
            } else { put(sink, out, "null"); }
        } else if (part.kind == LayoutPartKind::View) {
            sink.push(out, LayoutPart(LayoutPartKind::Dataset, value, 0u64));
            put(sink, out, ",\"view\":");
            address element = source.view(value);
            if (address_bits(element) != 0u64) {
                LayoutNode child = source.node(element);
                put(sink, out, "{\"element_stride_bytes\":");
                sink.number(out, u64(up(child.size, child.align)));
                put(sink, out, ",\"element\":");
                sink.push(out, LayoutPart(LayoutPartKind::Close, value, 0u64));
                sink.push(out, LayoutPart(LayoutPartKind::Value, element, 0u64));
            } else { put(sink, out, "null"); }
        } else if (part.kind == LayoutPartKind::Dataset) {
            sink.push(out, LayoutPart(LayoutPartKind::End, value, 0u64));
            put(sink, out, ",\"dataset\":");
            LayoutNode node = source.node(value);
            if ((node.flags & LAYOUT_DATASET) != 0) {
                put(sink, out, "{\"fixed_count\":");
                if (node.values != 0u64) { sink.number(out, u64(source.value(value, 0u64))); }
                else { put(sink, out, "null"); }
                put(sink, out, ",\"element_reference\":");
                source.dataset_name(out, value);
                sink.push(out, LayoutPart(LayoutPartKind::Close, value, 0u64));
                if (expand != 0) { return LayoutRequest(LayoutNeed::Resolve, value); }
            } else { put(sink, out, "null"); }
        } else if (part.kind == LayoutPartKind::End) {
            put(sink, out, ",\"cleanup\":");
            LayoutNode node = source.node(value);
            if ((node.flags & LAYOUT_CLEANUP) != 0) { sink.string(out, source.cleanup(value)); }
            else { put(sink, out, "null"); }
            put(sink, out, "}");
        } else if (part.kind == LayoutPartKind::Close) {
            put(sink, out, "}");
        } else {
            return LayoutRequest(LayoutNeed::Release, value);
        }
    }
}

extern "C" void write_value(address out, address value, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup) {
    sink.push(out, LayoutPart(LayoutPartKind::Value, value, 0u64));
    walk(out, 0, sink, source);
}

extern "C" LayoutRequest next_reference(address out, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup) {
    return walk(out, 1, sink, source);
}

extern "C" void resolved_element(address out, address value, u32 repeated, LayoutSink sink, LayoutSource source) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(source.node) {
    LayoutNode node = source.node(value);
    put(sink, out, ",\"element_stride_bytes\":");
    sink.number(out, u64(up(node.size, node.align)));
    put(sink, out, ",\"element\":");
    if (repeated != 0) {
        put(sink, out, "{\"kind\":\"type_reference\",\"name\":");
        sink.string(out, node.name);
        put(sink, out, "}");
    } else {
        sink.push(out, LayoutPart(LayoutPartKind::Release, value, 0u64));
        sink.push(out, LayoutPart(LayoutPartKind::Value, value, 0u64));
    }
}

extern "C" void write_interface(address out, address owner, LayoutSink sink, LayoutSource source,
    LayoutInterface declaration) on cpu
    requires nonzero(sink.text) requires nonzero(sink.string) requires nonzero(sink.number)
    requires nonzero(sink.push) requires nonzero(sink.pop)
    requires nonzero(source.node) requires nonzero(source.argument) requires nonzero(source.value)
    requires nonzero(source.component) requires nonzero(source.field) requires nonzero(source.variant)
    requires nonzero(source.array) requires nonzero(source.view) requires nonzero(source.dataset_name)
    requires nonzero(source.cleanup) requires nonzero(declaration.owner)
    requires nonzero(declaration.buffer) requires nonzero(declaration.uniform) {
    LayoutProgram header = declaration.owner(owner);
    put(sink, out, "{\"kind\":\"interface_layout\",\"program\":");
    sink.string(out, header.name);
    put(sink, out, ",\"target\":");
    if (header.target == 1) { put(sink, out, "\"cpu\""); }
    else if (header.target == 2) { put(sink, out, "\"gpu\""); }
    else { put(sink, out, "\"compute\""); }
    put(sink, out, ",\"uniform_bytes\":");
    sink.number(out, u64(header.uniform_bytes));
    put(sink, out, ",\"buffers\":[");
    u64 i = 0u64;
    while (i < header.buffers) {
        if (i != 0u64) { put(sink, out, ","); }
        LayoutBuffer buffer = declaration.buffer(owner, i);
        put(sink, out, "{\"binding\":");
        sink.number(out, i);
        put(sink, out, ",\"name\":");
        sink.string(out, buffer.name);
        put(sink, out, ",\"access\":");
        if (buffer.access == 1) { put(sink, out, "\"read\""); }
        else if (buffer.access == 2) { put(sink, out, "\"write\""); }
        else if (buffer.access == 3) { put(sink, out, "\"inout\""); }
        else if (buffer.access == 4) { put(sink, out, "\"atomic\""); }
        else { put(sink, out, "\"value\""); }
        put(sink, out, ",\"stride_bytes\":");
        sink.number(out, u64(buffer.stride));
        put(sink, out, ",\"fixed_count\":");
        if (buffer.count_present != 0) { sink.number(out, u64(buffer.count)); }
        else { put(sink, out, "null"); }
        put(sink, out, ",\"element\":");
        write_value(out, buffer.element, sink, source);
        put(sink, out, "}");
        i += 1u64;
    }
    put(sink, out, "],\"uniforms\":[");
    i = 0u64;
    while (i < header.uniforms) {
        if (i != 0u64) { put(sink, out, ","); }
        LayoutUniform uniform = declaration.uniform(owner, i);
        put(sink, out, "{\"name\":");
        sink.string(out, uniform.name);
        put(sink, out, ",\"offset_bytes\":");
        sink.number(out, u64(uniform.offset));
        put(sink, out, ",\"layout\":");
        write_value(out, uniform.value, sink, source);
        put(sink, out, "}");
        i += 1u64;
    }
    put(sink, out, "]}");
}
