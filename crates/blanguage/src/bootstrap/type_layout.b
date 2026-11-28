module bootstrap.type_layout;

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

extern "C" void visit_fields(address value, FieldHost host, FieldSelection selection, address context,
    cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit) {
    walk_fields(value,0,host,selection,context,emit);
}

void walk_fields(address value, u32 base, FieldHost host, FieldSelection selection,
    address context, cfn<void,address,u32,address> emit) on cpu
    requires nonzero(host.types.inspect) requires nonzero(host.types.child)
    requires nonzero(host.part) requires nonzero(emit) {
    TypeNode node=host.types.inspect(value);
    if (node.task!=0 || node.view!=0 || node.address!=0) {
        if (selection == FieldSelection::Borrow
            || (selection == FieldSelection::Address && node.address != 0)
            || (selection == FieldSelection::Task && node.task != 0)) {
            emit(context,base,value);
        }
        return;
    }
    if (node.variants!=0u64) {
        usize i=0u64;
        while (i<node.variants) {
            walk_fields(host.types.child(value,Child::Variant,i),base+node.alignment,host,selection,context,emit);
            i+=1u64;
        }
        return;
    }
    if (node.array!=0) {
        if (node.count==0) { return; }
        address element=host.types.child(value,Child::Array,0u64);
        Property needed=selection==FieldSelection::Task ? Property::Task : Property::Borrows;
        if (contains(element,needed,host.types)==0) { return; }
    }
    usize count=node.array!=0 ? usize(node.count) : node.fields;
    usize i=0u64;
    while (i<count) {
        TypePart part=host.part(value,i);
        walk_fields(part.value,base+part.offset,host,selection,context,emit);
        i+=1u64;
    }
}

extern "C" void select_snapshot_columns(address columns, usize count, usize ranges, u32 write,
    address context, SnapshotColumnHost host) on cpu
    requires nonzero(host.word) requires nonzero(host.metadata) {
    usize at=0u64;
    while (at<count) {
        address slot=unsafe_address(address_bits(columns)+at);
        u8 kind=unsafe_position<u8>(slot);
        if (kind!=u8(0) || write!=0) {
            u8 selected=kind!=u8(0) ? u8(2) : u8(1);
            if (ranges!=0u64) {
                u32 word=host.word(context,at);
                usize i=0u64;
                while (i<ranges) {
                    MetadataRange metadata=host.metadata(context,i);
                    if (metadata.start<=word && word<metadata.end) {
                        selected=u8(0);
                        break;
                    }
                    i+=1u64;
                }
            }
            unsafe_position<u8>(slot)=selected;
        }
        at+=1u64;
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
