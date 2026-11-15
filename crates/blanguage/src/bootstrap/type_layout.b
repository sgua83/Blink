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

extern "C" u32 contains(address value, Property property, TypeHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) {
    TypeNode node=host.inspect(value);
    bool owner=node.task!=0 || node.cleanup!=0;
    if ((property==Property::Dataset && node.dataset!=0)
        || (property==Property::Resources && (owner || node.dataset!=0))
        || (property==Property::Cleanup && owner)
        || (property==Property::Arrays && node.array!=0)
        || (property==Property::Views && node.view!=0)
        || (property==Property::Borrows && (node.task!=0 || node.address!=0 || node.view!=0))) {
        return 1;
    }
    usize i=0u64;
    while (i<node.variants) {
        if (contains(host.child(value,Child::Variant,i),property,host)!=0) { return 1; }
        i+=1u64;
    }
    if (node.array!=0 && node.count!=0
        && contains(host.child(value,Child::Array,0u64),property,host)!=0) { return 1; }
    i=0u64;
    while (i<node.fields) {
        if (contains(host.child(value,Child::Field,i),property,host)!=0) { return 1; }
        i+=1u64;
    }
    return 0;
}

extern "C" void borrow_fields(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit) {
    TypeNode node=host.types.inspect(value);
    if (node.task!=0 || node.view!=0 || node.address!=0) {
        emit(context,base,value);
        return;
    }
    if (node.variants!=0u64) {
        usize i=0u64;
        while (i<node.variants) {
            borrow_fields(host.types.child(value,Child::Variant,i),base+node.alignment,host,context,emit);
            i+=1u64;
        }
        return;
    }
    if (node.array!=0) {
        if (node.count==0) { return; }
        address element=host.types.child(value,Child::Array,0u64);
        if (contains(element,Property::Borrows,host.types)==0) { return; }
    }
    usize count=node.array!=0 ? usize(node.count) : node.fields;
    usize i=0u64;
    while (i<count) {
        TypePart part=host.part(value,i);
        borrow_fields(part.value,base+part.offset,host,context,emit);
        i+=1u64;
    }
}

extern "C" void task_output_owners(address value, u32 base, FieldHost host, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit) {
    TypeNode node=host.types.inspect(value);
    if (node.task!=0 || node.cleanup!=0 || node.dataset!=0) {
        emit(context,base,value);
        return;
    }
    if (node.variants!=0u64) {
        if (contains(value,Property::Resources,host.types)!=0) { emit(context,base,value); }
        return;
    }
    if (node.array!=0) {
        if (node.count==0) { return; }
        address element=host.types.child(value,Child::Array,0u64);
        if (contains(element,Property::Resources,host.types)==0) { return; }
    }
    usize count=node.array!=0 ? usize(node.count) : node.fields;
    usize i=0u64;
    while (i<count) {
        TypePart part=host.part(value,i);
        task_output_owners(part.value,base+part.offset,host,context,emit);
        i+=1u64;
    }
}

extern "C" u32 zero_valid(address value, TypeHost host,
    cfn<u32,address,usize> ensures memory(read) discriminant) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(discriminant) {
    TypeNode node=host.inspect(value);
    if (node.task!=0 || node.cleanup!=0 || node.dataset!=0 || node.view!=0) { return 0; }
    usize i=0u64;
    if (node.variants!=0u64) {
        while (i<node.variants) {
            if (discriminant(value,i)==0 && zero_valid(host.child(value,Child::Variant,i),host,discriminant)!=0) {
                return 1;
            }
            i+=1u64;
        }
        return 0;
    }
    if (node.array!=0 && node.count!=0
        && zero_valid(host.child(value,Child::Array,0u64),host,discriminant)==0) { return 0; }
    while (i<node.fields) {
        if (zero_valid(host.child(value,Child::Field,i),host,discriminant)==0) { return 0; }
        i+=1u64;
    }
    return 1;
}

u8 path_byte(Text text, usize index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+u64(index)));
}
bool same_name(Text a, Text b) on cpu {
    if (a.length!=b.length) { return false; }
    usize i=0u64;
    while (i<a.length) {
        if (path_byte(a,i)!=path_byte(b,i)) { return false; }
        i+=1u64;
    }
    return true;
}
extern "C" TypePart field(address value, Text path, address words, usize word_count,
    u32 active, PathHost host) on cpu
    requires nonzero(host.fields.types.inspect) requires nonzero(host.fields.part)
    requires nonzero(host.entry) requires nonzero(host.tag) {
    TypePart missing=TypePart(unsafe_address(0u64),0);
    address current=value;
    u32 offset=0;
    usize begin=0u64;
    while (true) {
        usize end=begin;
        while (end<path.length && path_byte(path,end)!=46) { end+=1u64; }
        usize split=begin;
        while (split<end && path_byte(path,split)!=91) { split+=1u64; }
        if (split!=begin) {
            Text name=Text(unsafe_address(address_bits(path.bytes)+u64(begin)),split-begin);
            TypeNode node=host.fields.types.inspect(current);
            bool variant=node.variants!=0u64;
            usize count=variant ? node.variants : node.fields;
            TypePart selected=missing;
            usize i=0u64;
            while (i<count) {
                PathEntry entry=host.entry(current,variant ? Child::Variant : Child::Field,i);
                if (same_name(name,entry.name)) {
                    if (variant && active!=0) {
                        usize at=usize(offset/4);
                        if (at>=word_count) { return missing; }
                        u32 bits=unsafe_position<u32>(unsafe_address(address_bits(words)+u64(at)*4u64));
                        if (host.tag(current,bits)!=entry.discriminant) { return missing; }
                    }
                    selected=entry.part;
                    break;
                }
                i+=1u64;
            }
            if (address_bits(selected.value)==0u64) { return missing; }
            offset+=selected.offset;
            current=selected.value;
        } else if (split==end) { return missing; }
        usize cursor=split;
        while (cursor<end) {
            if (path_byte(path,cursor)!=91) { return missing; }
            usize close=cursor+1u64;
            while (close<end && path_byte(path,close)!=93) { close+=1u64; }
            if (close==end) { return missing; }
            ParsedNumber number=decimal_number(unsafe_address(address_bits(path.bytes)+u64(cursor)+1u64),u64(close-cursor-1u64),4294967295u64);
            if (number.error!=NumberError::None) { return missing; }
            TypeNode node=host.fields.types.inspect(current);
            if (node.array==0 || number.value>=u64(node.count)) { return missing; }
            TypePart part=host.fields.part(current,usize(number.value));
            offset+=part.offset;
            current=part.value;
            cursor=close+1u64;
        }
        if (end==path.length) { break; }
        begin=end+1u64;
    }
    return TypePart(current,offset);
}
