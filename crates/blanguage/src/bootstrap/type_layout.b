module bootstrap.type_layout;
import bootstrap.type_families;

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

extern "C" ReferenceStep reference_step(inout ReferenceCursor cursor, read ReferenceShape shape) on cpu {
    if (cursor.phase == 0) {
        if (shape.arguments == 0u64 && shape.fields == 0u64 && shape.variants == 0u64
            && shape.signature == 0 && shape.array == 0 && shape.view == 0 && shape.dataset == 0) {
            return ReferenceStep(0u64, ReferenceNeed::Done);
        }
        cursor.phase = 1;
        return ReferenceStep(0u64, ReferenceNeed::Enter);
    }
    if (cursor.phase == 1) {
        cursor.phase = 2;
        if (shape.signature != 0) {
            return ReferenceStep(0u64, ReferenceNeed::Signature);
        }
    }

    usize index = cursor.next;
    cursor.next += 1u64;
    if (index < shape.parameters) { return ReferenceStep(index, ReferenceNeed::Parameter); }
    index -= shape.parameters;
    if (shape.result != 0) {
        if (index == 0u64) { return ReferenceStep(0u64, ReferenceNeed::Result); }
        index -= 1u64;
    }
    if (index < shape.arguments) { return ReferenceStep(index, ReferenceNeed::Argument); }
    index -= shape.arguments;
    if (index < shape.fields) { return ReferenceStep(index, ReferenceNeed::Field); }
    index -= shape.fields;
    if (index < shape.variants) { return ReferenceStep(index, ReferenceNeed::Variant); }
    index -= shape.variants;
    if (shape.array != 0) {
        if (index == 0u64) { return ReferenceStep(0u64, ReferenceNeed::Array); }
        index -= 1u64;
    }
    if (shape.view != 0) {
        if (index == 0u64) { return ReferenceStep(0u64, ReferenceNeed::View); }
        index -= 1u64;
    }
    if (shape.dataset != 0 && index == 0u64) {
        return ReferenceStep(0u64, ReferenceNeed::Dataset);
    }
    return ReferenceStep(0u64, ReferenceNeed::Done);
}

extern "C" StorageStep storage_step(inout StorageCursor cursor, read TypeNode node,
        u32 empty_dataset, u32 found) on cpu {
    if (found != 0) { return StorageStep(0u64, StorageNeed::Present); }

    bool array = node.array != 0 && node.count != 0;
    bool dataset = node.dataset != 0 && empty_dataset == 0;
    if (cursor.next == 0u64) {
        if (cursor.mode == StorageMode::CleanupBorrow && node.cleanup != 0) {
            cursor.mode = StorageMode::Borrow;
        }
        if (cursor.mode == StorageMode::CleanupRequirement) {
            if (node.cleanup != 0) { return StorageStep(0u64, StorageNeed::Contract); }
        } else if (node.view != 0 || node.address != 0) {
            StorageNeed leaf = cursor.mode == StorageMode::Borrow ? StorageNeed::Present : StorageNeed::Absent;
            return StorageStep(0u64, leaf);
        }
        if (node.fields == 0u64 && node.variants == 0u64 && !array && !dataset) {
            return StorageStep(0u64, StorageNeed::Absent);
        }
        cursor.next = 1u64;
        return StorageStep(0u64, StorageNeed::Enter);
    }

    usize index = cursor.next - 1u64;
    cursor.next += 1u64;
    if (index < node.fields) { return StorageStep(index, StorageNeed::Field); }
    index -= node.fields;
    if (index < node.variants) { return StorageStep(index, StorageNeed::Variant); }
    index -= node.variants;
    if (array) {
        if (index == 0u64) { return StorageStep(0u64, StorageNeed::Array); }
        index -= 1u64;
    }
    if (dataset && index == 0u64) { return StorageStep(0u64, StorageNeed::Dataset); }
    return StorageStep(0u64, StorageNeed::Absent);
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

extern "C" u32 numeric(address value,NumericHost host) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.inspect) requires nonzero(host.scalar) {
    NumericNode node=host.inspect(value);
    if(node.function!=0){return 0;}
    BuiltinType name=recognize(node.name,u64(node.length));
    if(name.kind==BuiltinKind::Handle||name.kind==BuiltinKind::Address){return 0;}
    if(node.enumeration!=0||node.variants!=0u64||node.view!=0||node.array!=0
        ||node.fields!=0u64||node.components==0u64){return 0;}
    usize index=0u64;
    while(index<node.components){
        if(host.scalar(value,index)==u32(BuiltinScalar::Bool)){return 0;}
        index+=1u64;
    }
    return contains(value,Property::Resources,host.types)==0 ? 1 : 0;
}

struct DatasetBranch { Child kind; usize count; };

DatasetBranch dataset_branch(TypeNode node) on cpu {
    if (node.variants != 0u64) { return DatasetBranch(Child::Variant, node.variants); }
    if (node.array != 0) { return DatasetBranch(Child::Array, usize(node.count)); }
    return DatasetBranch(Child::Field, node.fields);
}

extern "C" usize dataset_count(address value, TypeHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) {
    TypeNode node = host.inspect(value);
    if (node.dataset != 0) { return 1u64; }
    DatasetBranch branch = dataset_branch(node);
    if (branch.kind == Child::Array && branch.count != 0u64) {
        return branch.count * dataset_count(host.child(value, Child::Array, 0u64), host);
    }
    usize count = 0u64;
    usize index = 0u64;
    while (index < branch.count) {
        count += dataset_count(host.child(value, branch.kind, index), host);
        index += 1u64;
    }
    return count;
}

struct DatasetPath { usize remaining; u32 found; };

DatasetPath select_dataset_path(address value, usize ordinal, TypeHost host, address context,
        cfn<void,address,usize> push) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(push) {
    TypeNode node = host.inspect(value);
    if (node.dataset != 0) {
        if (ordinal == 0u64) { return DatasetPath(0u64, 1); }
        return DatasetPath(ordinal - 1u64, 0);
    }
    DatasetBranch branch = dataset_branch(node);
    if (branch.kind == Child::Array && branch.count != 0u64) {
        address element = host.child(value, Child::Array, 0u64);
        usize width = dataset_count(element, host);
        if (width == 0u64) { return DatasetPath(ordinal, 0); }
        usize index = ordinal / width;
        if (index >= branch.count) { return DatasetPath(ordinal - branch.count * width, 0); }
        DatasetPath selected = select_dataset_path(element, ordinal % width, host, context, push);
        push(context, index);
        return selected;
    }
    usize index = 0u64;
    usize remaining = ordinal;
    while (index < branch.count) {
        DatasetPath selected = select_dataset_path(host.child(value, branch.kind, index),
            remaining, host, context, push);
        if (selected.found != 0) {
            push(context, index);
            return selected;
        }
        remaining = selected.remaining;
        index += 1u64;
    }
    return DatasetPath(remaining, 0);
}

extern "C" u32 dataset_path(address value, usize ordinal, TypeHost host, address context,
        cfn<void,address,usize> push) on cpu
    requires nonzero(host.inspect) requires nonzero(host.child) requires nonzero(push) {
    return select_dataset_path(value, ordinal, host, context, push).found;
}

extern "C" void dataset_paths(address value, address context, DatasetPathsHost host) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.push) requires nonzero(host.pop) requires nonzero(host.emit) {
    TypeNode node = host.types.inspect(value);
    if (node.dataset != 0) {
        host.emit(context);
        return;
    }
    DatasetBranch branch = dataset_branch(node);
    if (branch.kind == Child::Array && branch.count != 0u64) {
        address element = host.types.child(value, Child::Array, 0u64);
        if (contains(element, Property::Dataset, host.types) == 0) { return; }
        usize index = 0u64;
        while (index < branch.count) {
            host.push(context, index);
            dataset_paths(element, context, host);
            host.pop(context);
            index += 1u64;
        }
        return;
    }
    usize index = 0u64;
    while (index < branch.count) {
        host.push(context, index);
        dataset_paths(host.types.child(value, branch.kind, index), context, host);
        host.pop(context);
        index += 1u64;
    }
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
PathEntry named_entry(address value, Text name, Child kind, usize count,
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry) on cpu requires nonzero(entry) {
    usize index=0u64;
    while (index<count) {
        PathEntry candidate=entry(value,kind,index);
        if (same_name(name,candidate.name)) { return candidate; }
        index+=1u64;
    }
    return PathEntry(Text(address(),0u64),TypePart(address(),0),0);
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
            PathEntry selected=named_entry(current,name,variant ? Child::Variant : Child::Field,count,host.entry);
            if (address_bits(selected.part.value)==0u64) { return missing; }
            if (variant && active!=0) {
                usize at=usize(offset/4);
                if (at>=word_count) { return missing; }
                u32 bits=unsafe_position<u32>(unsafe_address(address_bits(words)+u64(at)*4u64));
                if (host.tag(current,bits)!=selected.discriminant) { return missing; }
            }
            offset+=selected.part.offset;
            current=selected.part.value;
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

extern "C" MemberSelection source_member(address value, Text name, MemberHost host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.entry) requires nonzero(host.swizzle_width) {
    MemberSelection selected;
    selected.kind=MemberKind::Missing;
    selected.count=0;
    selected.part=TypePart(address(),0);
    selected.lanes=array<u32,4>(0,0,0,0);
    TypeNode node=host.inspect(value);
    if (node.variants!=0u64) {
        selected.kind=MemberKind::Variant;
        return selected;
    }
    if (node.view!=0) {
        array<u8,6> length_name="length";
        selected.kind=same_name(name,Text(address_of(length_name[0]),6u64)) ?
            MemberKind::Length : MemberKind::HiddenView;
        return selected;
    }
    if (node.fields!=0u64) {
        PathEntry field=named_entry(value,name,Child::Field,node.fields,host.entry);
        if (address_bits(field.part.value)!=0u64) {
            selected.kind=MemberKind::Declared;
            selected.part=field.part;
        }
        return selected;
    }
    if (node.array!=0 || name.length>4u64) { return selected; }
    usize width=host.swizzle_width(value);
    if (width<=1u64) { return selected; }
    for (index in 0..4) {
        if (usize(index)==name.length) { break; }
        u8 letter=path_byte(name,usize(index));
        u32 lane=0;
        if (letter==120 || letter==114) { lane=0; }
        else if (letter==121 || letter==103) { lane=1; }
        else if (letter==122 || letter==98) { lane=2; }
        else if (letter==119 || letter==97) { lane=3; }
        else {
            selected.kind=MemberKind::UnknownComponent;
            return selected;
        }
        if (u64(lane)>=width) {
            selected.kind=MemberKind::ComponentBounds;
            return selected;
        }
        selected.lanes[index]=lane;
    }
    selected.kind=MemberKind::Swizzle;
    selected.count=u32(name.length);
    return selected;
}

extern "C" VariantMember declared_variant(address value, Text name, usize count,
    cfn<PathEntry,address,Child,usize> ensures memory(read) entry) on cpu requires nonzero(entry) {
    PathEntry selected=named_entry(value,name,Child::Variant,count,entry);
    return VariantMember(u32(address_bits(selected.part.value)!=0u64),selected.discriminant);
}
