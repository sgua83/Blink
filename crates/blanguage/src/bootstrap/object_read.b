module bootstrap.object_read;
import blink.windows.coff;
import bootstrap.metadata_read;
import bootstrap.bindings;
import bootstrap.coff;

const u8 SYMBOL_IMPORT = u8(2);
const u8 SYMBOL_INVALID_IMPORT = u8(3);

ReadBytes empty_bytes() on cpu {return ReadBytes(unsafe_address(0u64),0u64);}
u32 byte_at(ReadBytes bytes,u64 index) on cpu {
    return u32(unsafe_position<u8>(unsafe_address(address_bits(bytes.bytes)+index)));
}
ReadBytes part(ReadBytes bytes,u64 start,u64 length) on cpu {
    return ReadBytes(unsafe_address(address_bits(bytes.bytes)+start),length);
}
bool matches<const N: u32>(ReadBytes value,read array<u8,N> pattern,bool whole) on cpu {
    if(whole && value.length!=u64(N)){return false;}
    if(value.length<u64(N)){return false;}
    for(index in 0..N){if(byte_at(value,u64(index))!=u32(pattern[index])){return false;}}
    return true;
}
address row_address(ObjectRows rows,u64 index) on cpu {
    return unsafe_address(address_bits(rows.values)+index*size_of<ObjectSection>());
}
ObjectSection row_at(ObjectRows rows,u64 index) on cpu {
    return unsafe_position<ObjectSection>(row_address(rows,index));
}
enum SectionKind : u32 { Code = 0; Metadata = 1; Runtime = 2; Build = 3; };
SectionKind section_kind(ReadBytes name) on cpu {
    array<u8,7> meta = array<u8,7>(u8(46),u8(98),u8(108),u8(109),u8(101),u8(116),u8(97));
    array<u8,5> runtime = array<u8,5>(u8(46),u8(98),u8(108),u8(114),u8(116));
    array<u8,8> build = array<u8,8>(u8(46),u8(98),u8(108),u8(98),u8(117),u8(105),u8(108),u8(100));
    if (matches<7>(name, meta, true)) { return SectionKind::Metadata; }
    if (matches<5>(name, runtime, true)) { return SectionKind::Runtime; }
    if (matches<8>(name, build, true)) { return SectionKind::Build; }
    return SectionKind::Code;
}
ObjectSectionResult logical_section(ReadBytes input, ReadBytes headers, u64 index,
    inout u64 mapped, inout ObjectParts parts, inout ReadSymbols table) on cpu {
    ReadSection row = resolved_section_with_symbols(input, headers, index, table);
    ObjectSection value = ObjectSection(row.name,row.bytes,row.flags,row.relocations,row.relocation_count,0u64,row.zero_fill,unsafe_address(0u64),0u64);
    if (row.error != 0) { return ObjectSectionResult(value, row.error); }
    let kind = section_kind(row.name);
    if (kind == SectionKind::Metadata) {
        if (address_bits(parts.metadata.bytes) == 0u64) { parts.metadata = row.bytes; }
    } else if (kind == SectionKind::Runtime) {
        if (address_bits(parts.runtime.bytes) == 0u64) { parts.runtime = row.bytes; }
    } else if (kind == SectionKind::Build) {
        if (parts.build_section == 0u64) { parts.build_section = index + 1u64; }
    } else {
        mapped += 1u64;
        value.mapped = mapped;
    }
    return ObjectSectionResult(value, 0);
}
address definition_address(address definitions,u64 index) on cpu {
    return unsafe_address(address_bits(definitions)+index*4u64);
}
ObjectSymbolResult object_symbol(
    ReadBytes input,ReadSymbols table,u64 index,address definitions,u64 definition_count
) on cpu {
    ReadSymbol row=symbol(input,table,index);
    u8 external=u8(0);
    if(row.storage==2u64){external=u8(1);}
    ObjectSymbol primary=ObjectSymbol(row.name,u32(row.value),i16(row.section),external,u8(row.function));
    if(row.storage==3u64 && row.section>i64(0) && u64(row.section)<=definition_count && row.auxiliary!=0u64){
        address slot=definition_address(definitions,u64(row.section)-1u64);
        unsafe_position<SectionDefinition>(slot)=SectionDefinition(u16(row.owner),u8(row.selection),u8(1));
    }
    return ObjectSymbolResult(primary,row.next,row.error);
}

struct ObjectSectionResult {ObjectSection value;u32 error;};
struct ObjectSymbolResult {ObjectSymbol value;u64 next;u32 error;};

extern "C" ObjectScanStatus object_sections(ReadBytes input, ReadBytes headers, address rows, address loadable,
    inout ObjectParts parts, inout ReadSymbols table, address context, cfn<u32,address,ReadBytes> validate) on cpu
    requires nonzero(validate) {
    u64 mapped = 0u64;
    u64 count = headers.length / 40u64;
    bool keep_rows = address_bits(rows) != 0u64;
    bool keep_loadable = address_bits(loadable) != 0u64;
    u64 index = 0u64;
    while (index < count) {
        let section = logical_section(input, headers, index, mapped, parts, table);
        if (section.error != 0) { return ObjectScanStatus(section.error, 0); }
        if (validate(context, section.value.name) == 0) { return ObjectScanStatus(0, 0); }
        if (keep_rows) {
            address slot = unsafe_address(address_bits(rows) + index * size_of<ObjectSection>());
            unsafe_position<ObjectSection>(slot) = section.value;
        } else if (keep_loadable) {
            address slot = unsafe_address(address_bits(loadable) + index);
            unsafe_position<u8>(slot) = section.value.mapped != 0u64 ? u8(1) : u8(0);
        }
        index += 1u64;
    }
    return ObjectScanStatus(0, 1);
}

extern "C" ObjectScanStatus object_metadata_sections(ReadBytes input, ReadBytes headers, u32 include_runtime,
    inout ObjectParts parts, inout ReadSymbols table, address context, cfn<u32,address,ReadBytes> validate) on cpu
    requires nonzero(validate) {
    u64 index = 0u64;
    u64 count = headers.length / 40u64;
    while (index < count) {
        let name = section_name_with_symbols(input, headers, index, table);
        if (name.error != 0) { return ObjectScanStatus(name.error, 0); }
        if (validate(context, name.bytes) == 0) { return ObjectScanStatus(0, 0); }
        let kind = section_kind(name.bytes);
        bool metadata = kind == SectionKind::Metadata;
        if ((metadata && address_bits(parts.metadata.bytes) == 0u64)
            || (include_runtime != 0 && kind == SectionKind::Runtime && address_bits(parts.runtime.bytes) == 0u64)) {
            let row = section(input, headers, index);
            if (row.error != 0) { return ObjectScanStatus(row.error, 0); }
            if (metadata) { parts.metadata = row.bytes; }
            else { parts.runtime = row.bytes; }
        }
        index += 1u64;
    }
    return ObjectScanStatus(0, 1);
}

extern "C" ObjectScanStatus object_definitions(ReadBytes input, ReadSymbols table, address context,
    cfn<u32,address,ReadBytes> consume) on cpu requires nonzero(consume) {
    u64 index = 0u64;
    while (true) {
        let row = next_export(input, table, index);
        if (row.error != 0) { return ObjectScanStatus(row.error, 0); }
        if (row.bytes.bytes == address()) { return ObjectScanStatus(0, 1); }
        if (consume(context, row.bytes) == 0) { return ObjectScanStatus(0, 0); }
    }
}
extern "C" ObjectScanStatus object_symbols(ReadBytes input, ReadSymbols table, address rows,
    address definitions, u64 definition_count, address context,
    cfn<u32,address,ReadBytes,u64> consume) on cpu requires nonzero(consume) {
    u64 index = 0u64;
    bool keep_rows = address_bits(rows) != 0u64;
    let auxiliary = ObjectSymbol(empty_bytes(), 0, i16(0), u8(0), u8(0));
    while (index < table.count) {
        let primary = object_symbol(input, table, index, definitions, definition_count);
        if (primary.error != 0) { return ObjectScanStatus(primary.error, 0); }
        let value = primary.value;
        u64 export_section = value.external != u8(0) && value.section > i16(0) ? u64(value.section) : 0u64;
        if (consume(context, value.name, export_section) == 0) { return ObjectScanStatus(0, 0); }
        if (keep_rows) {
            address slot = unsafe_address(address_bits(rows) + index * size_of<ObjectSymbol>());
            unsafe_position<ObjectSymbol>(slot) = value.section <= i16(0) ? undefined_symbol(value) : value;
            index += 1u64;
            while (index < primary.next) {
                address next = unsafe_address(address_bits(rows) + index * size_of<ObjectSymbol>());
                unsafe_position<ObjectSymbol>(next) = auxiliary;
                index += 1u64;
            }
        }
        index = primary.next;
    }
    return ObjectScanStatus(0, 1);
}

struct ObjectAssociation {u64 present;u64 owner;u32 error;};
ObjectAssociation association(ObjectRows rows,address definitions,u64 index) on cpu {
    SectionDefinition definition=unsafe_position<SectionDefinition>(definition_address(definitions,index));
    if(definition.present==u8(0)){return ObjectAssociation(0u64,0u64,16);}
    u64 selection=u64(definition.selection);
    u64 owner=u64(definition.owner);
    if(selection==1u64){return ObjectAssociation(0u64,0u64,0);}
    if(selection!=5u64){return ObjectAssociation(0u64,0u64,4);}
    if(owner==0u64){return ObjectAssociation(0u64,0u64,1);}
    if(owner>rows.count){return ObjectAssociation(0u64,0u64,2);}
    ObjectSection value=row_at(rows,owner-1u64);
    if(value.mapped==0u64){return ObjectAssociation(0u64,0u64,2);}
    if((value.flags&4096u64)==0u64){return ObjectAssociation(0u64,0u64,3);}
    return ObjectAssociation(1u64,value.mapped-1u64,0);
}
address host_section(address output,u64 index) on cpu {
    return unsafe_address(address_bits(output)+index*size_of<HostSection>());
}

extern "C" HostArchiveMode host_archive_mode(ObjectRows rows,ObjectSymbols symbols,BindingBytes runtime) on cpu {
    u64 index=0u64;
    while(index<rows.count){
        ObjectSection row=row_at(rows,index);
        if(row.mapped!=0u64 && needs_body(row.flags,row.relocation_count)!=0){return HostArchiveMode::Materialize;}
        index+=1u64;
    }

    index=0u64;
    while(index<symbols.count){
        address slot=unsafe_address(address_bits(symbols.values)+index*size_of<ObjectSymbol>());
        ObjectSymbol symbol=unsafe_position<ObjectSymbol>(slot);
        if(symbol.external!=u8(0) && symbol.section<=i16(0)){
            if(symbol.role!=SYMBOL_IMPORT || bytes_compare(BindingBytes(symbol.name.bytes,u64(symbol.value)),runtime)==0){
                return HostArchiveMode::Group;
            }
        }
        index+=1u64;
    }
    return HostArchiveMode::Borrow;
}

extern "C" HostGroupResult host_groups(ObjectRows rows,address definitions,address output,u64 count) on cpu {
    u64 unassigned=18446744073709551615u64;
    u64 index=0u64;
    while(index<rows.count){
        ObjectSection row=row_at(rows,index);
        if(row.mapped!=0u64){
            u64 owner=0u64;
            if((row.flags&4096u64)!=0u64){
                ObjectAssociation edge=association(rows,definitions,index);
                if(edge.error!=0){return HostGroupResult(0u64,edge.error);}
                if(edge.present!=0u64){owner=edge.owner+1u64;}
            }
            unsafe_position<HostSection>(host_section(output,row.mapped-1u64))=HostSection(owner,unassigned,0u64);
        }
        index+=1u64;
    }

    // Close the original owner chain in the final row. The count is outside
    // the logical index range and marks entries on the current walk.
    index=0u64;
    while(index<count){
        u64 current=index;
        address at=host_section(output,current);
        while(unsafe_position<HostSection>(at).member==unassigned){
            u64 owner=unsafe_position<HostSection>(at).owner;
            if(owner==0u64){
                unsafe_position<HostSection>(at).member=current;
                break;
            }
            unsafe_position<HostSection>(at).member=count;
            current=owner-1u64;
            at=host_section(output,current);
        }
        u64 root=unsafe_position<HostSection>(at).member;
        if(root==count){return HostGroupResult(0u64,22);}
        at=host_section(output,index);
        while(unsafe_position<HostSection>(at).member==count){
            unsafe_position<HostSection>(at).member=root;
            at=host_section(output,unsafe_position<HostSection>(at).owner-1u64);
        }
        index+=1u64;
    }

    // Until projection into members, local holds the root's group number plus one.
    u64 groups=0u64;
    index=0u64;
    while(index<count){
        address at=host_section(output,index);
        address root=host_section(output,unsafe_position<HostSection>(at).member);
        u64 member=unsafe_position<HostSection>(root).local;
        if(member==0u64){
            groups+=1u64;
            member=groups;
            unsafe_position<HostSection>(root).local=member;
        }
        unsafe_position<HostSection>(at).member=member-1u64;
        index+=1u64;
    }
    return HostGroupResult(groups,0);
}

address host_group_state(address states,u64 group) on cpu {
    return unsafe_address(address_bits(states)+group*size_of<HostGroupState>());
}
address host_group_word(address words,u64 index) on cpu {
    return unsafe_address(address_bits(words)+index*size_of<u64>());
}
void reverse_host_links(HostGroupLinks input,HostGroupSources source,address edges,bool fill) on cpu
    requires nonzero(source.members) requires nonzero(source.relocations) {
    u64 member=0u64;
    while(member<input.count){
        HostGroupSpan sections=source.members(source.context,member);
        u64 section=0u64;
        while(section<sections.count){
            u64 physical=unsafe_position<u64>(host_group_word(sections.rows,section));
            u64 logical=row_at(input.sections,physical).mapped-1u64;
            HostGroupSpan relocations=source.relocations(source.context,logical);
            u64 i=0u64;
            while(i<relocations.count){
                address pos=unsafe_address(address_bits(relocations.rows)+i*size_of<BindingRelocation>());
                BindingRelocation relocation=unsafe_position<BindingRelocation>(pos);
                if(relocation.target==u16(0)){
                    u64 destination=unsafe_position<HostSection>(host_section(input.positions,relocation.section)).member;
                    address at=host_group_state(input.states,destination);
                    if(destination!=member && unsafe_position<HostGroupState>(at).seen!=member+1u64){
                        unsafe_position<HostGroupState>(at).seen=member+1u64;
                        u64 next=unsafe_position<HostGroupState>(at).end;
                        if(fill){unsafe_position<u64>(host_group_word(edges,next))=member;}
                        unsafe_position<HostGroupState>(at).end=next+1u64;
                    }
                }else if(!fill && (relocation.target==u16(1) || bytes_compare(relocation.dll,input.runtime)==0)){
                    unsafe_position<HostGroupState>(host_group_state(input.states,member)).required=1u64;
                }
                i+=1u64;
            }
            section+=1u64;
        }
        member+=1u64;
    }
}

extern "C" u64 host_group_resolution(HostGroupLinks input,HostGroupSources source) on cpu
    requires nonzero(source.members) requires nonzero(source.relocations) requires nonzero(source.reserve) {
    reverse_host_links(input,source,address(),false);
    u64 edges_count=0u64;
    u64 member=0u64;
    while(member<input.count){
        address at=host_group_state(input.states,member);
        u64 start=edges_count;
        edges_count+=unsafe_position<HostGroupState>(at).end;
        unsafe_position<HostGroupState>(at).start=start;
        unsafe_position<HostGroupState>(at).end=start;
        unsafe_position<HostGroupState>(at).seen=0u64;
        member+=1u64;
    }
    address edges=source.reserve(source.context,edges_count);
    reverse_host_links(input,source,edges,true);

    // Counting and filling use the stamp. Once filled, the same word
    // links groups that still need to propagate their external dependency.
    u64 pending=0u64;
    member=0u64;
    while(member<input.count){
        address at=host_group_state(input.states,member);
        if(unsafe_position<HostGroupState>(at).required!=0u64){
            unsafe_position<HostGroupState>(at).seen=pending;
            pending=member+1u64;
        }
        member+=1u64;
    }
    while(pending!=0u64){
        HostGroupState group=unsafe_position<HostGroupState>(host_group_state(input.states,pending-1u64));
        pending=group.seen;
        u64 i=group.start;
        while(i<group.end){
            u64 dependent=unsafe_position<u64>(host_group_word(edges,i));
            address at=host_group_state(input.states,dependent);
            if(unsafe_position<HostGroupState>(at).required==0u64){
                unsafe_position<HostGroupState>(at).required=1u64;
                unsafe_position<HostGroupState>(at).seen=pending;
                pending=dependent+1u64;
            }
            i+=1u64;
        }
    }

    u64 first=input.count;
    member=0u64;
    while(member<input.count){
        if(unsafe_position<HostGroupState>(host_group_state(input.states,member)).required==0u64){
            if(first!=input.count){return first;}
            first=member;
        }
        member+=1u64;
    }
    return input.count;
}

extern "C" void rebind_host_relocations(address positions,u64 member,address records,u64 count,
    address context,cfn<BindingBytes,address,u64,u32> anchor) on cpu requires nonzero(anchor) {
    u64 index=0u64;
    while(index<count){
        address at=unsafe_address(address_bits(records)+index*size_of<BindingRelocation>());
        BindingRelocation relocation=unsafe_position<BindingRelocation>(at);
        if(relocation.target==u16(0)){
            HostSection target=unsafe_position<HostSection>(host_section(positions,relocation.section));
            if(target.member==member){
                unsafe_position<BindingRelocation>(at).section=target.local;
            }else{
                BindingBytes name=anchor(context,relocation.section,relocation.value);
                unsafe_position<BindingRelocation>(at).target=u16(1);
                unsafe_position<BindingRelocation>(at).name=name;
            }
        }
        index+=1u64;
    }
}

ObjectTarget failure(u32 error) on cpu {
    return ObjectTarget(0u64,0u64,0u64,empty_bytes(),empty_bytes(),error);
}
ObjectSymbol invalid_import(ObjectSymbol symbol, u32 error) on cpu {
    return ObjectSymbol(symbol.name,error,symbol.section,symbol.external,SYMBOL_INVALID_IMPORT);
}
ObjectSymbol undefined_symbol(ObjectSymbol symbol) on cpu {
    array<u8,15> prefix=array<u8,15>(u8(95),u8(95),u8(98),u8(108),u8(105),u8(110),u8(107),u8(95),u8(105),u8(109),u8(112),u8(111),u8(114),u8(116),u8(36));
    ReadBytes name=symbol.name;
    if(!matches<15>(name,prefix,false)){return symbol;}
    u64 index=15u64;
    u64 end=index;
    while(end<name.length && byte_at(name,end)!=36){end+=1u64;}
    if(end==name.length){return invalid_import(symbol,6);}
    if(index==end){return invalid_import(symbol,7);}
    if(byte_at(name,index)==43){index+=1u64;if(index==end){return invalid_import(symbol,8);}}
    u64 length=0u64;
    while(index<end){
        u32 digit=byte_at(name,index);
        if(digit<48 || digit>57){return invalid_import(symbol,8);}
        u64 value=u64(digit-48);
        if(length>(18446744073709551615u64-value)/10u64){return invalid_import(symbol,9);}
        length=length*10u64+value;index+=1u64;
    }
    u64 start=end+1u64;u64 available=name.length-start;
    if(length>available){return invalid_import(symbol,10);}
    u64 boundary=start+length;
    if(boundary<name.length && (byte_at(name,boundary)&192)==128){return invalid_import(symbol,10);}
    // The borrowed name lies in the COFF string table with a 32-bit size.
    return ObjectSymbol(part(name,start,available),u32(length),symbol.section,symbol.external,SYMBOL_IMPORT);
}

ObjectTarget relocation_target(ObjectRows rows, ObjectSymbols symbols, u64 index) on cpu {
    if (index >= symbols.count) { return failure(15); }
    address slot = unsafe_address(address_bits(symbols.values) + index * size_of<ObjectSymbol>());
    ObjectSymbol symbol = unsafe_position<ObjectSymbol>(slot);
    if (address_bits(symbol.name.bytes) == 0u64) { return failure(15); }
    if(symbol.section>i16(0)){
        u64 section=u64(symbol.section)-1u64;
        if(section>=rows.count){return failure(5);}
        ObjectSection row=row_at(rows,section);
        if(row.mapped==0u64){return failure(5);}
        return ObjectTarget(0u64,row.mapped-1u64,u64(symbol.value),empty_bytes(),empty_bytes(),0);
    }
    if(symbol.role==SYMBOL_INVALID_IMPORT){return failure(symbol.value);}
    if(symbol.role==SYMBOL_IMPORT){
        u64 length=u64(symbol.value);
        return ObjectTarget(1u64,0u64,0u64,part(symbol.name,0u64,length),
            part(symbol.name,length,symbol.name.length-length),0);
    }
    return ObjectTarget(2u64,0u64,0u64,empty_bytes(),symbol.name,0);
}

ObjectRelocationStatus read_object_relocations(
    ReadBytes input, ObjectRows rows, ObjectSymbols symbols, read ObjectSection row,
    address context, cfn<void,address,u64,u64,u32,ObjectTarget> consume,
    cfn<u32,address,ReadBytes,u32> binding, u32 native
) on cpu requires nonzero(consume) {
    ReadRelocations table = relocations(input, row.relocations, row.relocation_count, row.flags);
    if (table.error != 0) { return ObjectRelocationStatus(table.error, 0); }

    u64 length = row.zero_fill == 0u64 ? row.bytes.length : row.zero_fill;
    u64 index = 0u64;
    while (index < table.count) {
        ReadRelocation record = relocation(input, table.offset, index);
        if (record.error != 0) { return ObjectRelocationStatus(record.error, 0); }
        u32 kind = u32(record.kind);
        // OutsideSection is a read error. Other kinds remain for the applicator.
        if (relocation_span(length, record.offset, kind) == 2) {
            return ObjectRelocationStatus(0, 17);
        }

        ObjectTarget target = relocation_target(rows, symbols, record.symbol);
        if (target.error != 0) { return ObjectRelocationStatus(0, target.error); }
        if (target.kind == 2u64 && binding != cfn<u32,address,ReadBytes,u32>()) {
            if (binding(context, target.name, native) == 0) { return ObjectRelocationStatus(0, 21); }
        }
        consume(context, row.mapped - 1u64, record.offset, kind, target);
        index += 1u64;
    }
    return ObjectRelocationStatus(0, 0);
}

extern "C" ObjectRelocationStatus object_relocations(
    ReadBytes input, ObjectRows rows, ObjectSymbols symbols, read ObjectSection row,
    address context, cfn<void,address,u64,u64,u32,ObjectTarget> consume
) on cpu requires nonzero(consume) {
    return read_object_relocations(input, rows, symbols, row, context, consume,
        cfn<u32,address,ReadBytes,u32>(), 0);
}

address stack_frame_address(address frames,u64 index) on cpu {
    return unsafe_address(address_bits(frames)+index*64u64);
}
address stack_call_address(address calls,u64 index) on cpu {
    return unsafe_address(address_bits(calls)+index*size_of<MetaCall>());
}
u64 stack_relocation(ObjectSection section,u64 offset,u64 stride) on cpu {
    u64 start=address_bits(section.decoded_relocations);
    u64 i=0u64;
    while(i<section.decoded_count){
        if(unsafe_position<u64>(unsafe_address(start+i*stride))==offset){return i;}
        i+=1u64;
    }
    return section.decoded_count;
}
extern "C" u32 recover_stack(ObjectRows rows,address frames,u64 count,address calls,address nodes,u64 stride,u32 stack_output) on cpu {
    // Resolve all section identities before interpreting positions and calls.
    u64 i=0u64;
    while(i<count){
        MetaFrame frame=unsafe_position<MetaFrame>(stack_frame_address(frames,i));
        if(frame.has_coff!=0u64){
            if(frame.section>=rows.count || row_at(rows,frame.section).mapped==0u64){return 11;}
        }
        i+=1u64;
    }
    BindingForest positions=BindingForest(nodes,0u64,0u64,0u64);
    BindingBytes empty=BindingBytes(unsafe_address(0u64),0u64);
    u64 root=0u64;
    u64 next_call=0u64;
    i=0u64;
    while(i<count){
        address row=stack_frame_address(frames,i);
        MetaFrame frame=unsafe_position<MetaFrame>(row);
        if(frame.has_coff!=0u64){
            ObjectSection section=row_at(rows,frame.section);
            if(frame.start>=section.bytes.length+section.zero_fill){return 12;}
            // The immutable section/start pair is the key until remapping.
            BindingBytes location=BindingBytes(unsafe_address(address_bits(row)+24u64),16u64);
            BindingInsert insertion=insert_binding(positions,root,BindingKey(location,empty),0u64,false);
            if(!insertion.inserted){return 12;}
            root=insertion.root;
        }
        u64 remaining=frame.calls;
        while(remaining!=0u64){
            address call_row=stack_call_address(calls,next_call);
            MetaCall call=unsafe_position<MetaCall>(call_row);
            next_call+=1u64;
            remaining-=1u64;
            if(call.kind<2u64){
                if(frame.has_coff==0u64){return 13;}
                ObjectSection section=row_at(rows,frame.section);
                u64 relocation=stack_relocation(section,call.offset,stride);
                if(relocation==section.decoded_count){return 14;}
                if(stack_output!=0){
                    call.offset=relocation;
                    unsafe_position<MetaCall>(call_row)=call;
                }
            }
        }
        i+=1u64;
    }
    if(stack_output==0){return 0;}
    i=0u64;
    while(i<count){
        address row=stack_frame_address(frames,i);
        MetaFrame frame=unsafe_position<MetaFrame>(row);
        if(frame.has_coff!=0u64){
            frame.section=row_at(rows,frame.section).mapped-1u64;
            unsafe_position<MetaFrame>(row)=frame;
        }
        i+=1u64;
    }
    return 0;
}

MetaRecovery restore_blink_metadata(RestoreSource source, ObjectParts parts, MetaNameSink names,
    MetaReceiver metadata) on cpu
    requires nonzero(names.emit)
    requires nonzero(metadata.kind) requires nonzero(metadata.valid_text) requires nonzero(metadata.binding)
    requires nonzero(metadata.resource) requires nonzero(metadata.code) requires nonzero(metadata.frame)
    requires nonzero(metadata.call) {
    u32 archive = address_bits(parts.runtime.bytes) == 0u64 ? 0 : 1;
    u32 groups = u32(MetaNameGroup::Requirement) | u32(MetaNameGroup::Export);
    if (address_bits(source.reused_prefix) != 0u64) {
        MetaCursor cursor = source.reused_cursor;
        let fields = unsafe_position<MetaPrefix>(source.reused_prefix);
        u32 error = project_names(fields, MetaNameSink(names.context, names.emit, groups));
        if (error != 0) { return MetaRecovery(error, 0); }
        return restore_metadata(cursor, fields, archive, 1, metadata);
    }
    MetaCursor cursor = MetaCursor(parts.metadata, 0u64);
    let fields = prefix(cursor, source.format, MetaNameSink(names.context, names.emit, groups | u32(MetaNameGroup::Library)));
    if (fields.header.error != 0) { return MetaRecovery(fields.header.error, 0); }
    return restore_metadata(cursor, fields, archive, 0, metadata);
}

u32 emit_sections(ObjectRows rows, address definitions, RestoreObjectSink sink) on cpu
    requires nonzero(sink.section) requires nonzero(sink.associate) {
    u64 index = 0u64;
    while (index < rows.count) {
        let row = row_at(rows, index);
        if (row.mapped != 0u64) {
            sink.section(sink.context, row.name, row.bytes, row.flags, row.zero_fill);
        }
        index += 1u64;
    }
    index = 0u64;
    while (index < rows.count) {
        let row = row_at(rows, index);
        if (row.mapped != 0u64 && (row.flags & 4096u64) != 0u64) {
            let associated = association(rows, definitions, index);
            if (associated.error != 0) { return associated.error; }
            if (associated.present != 0u64) {
                sink.associate(sink.context, associated.owner, row.mapped - 1u64);
            }
        }
        index += 1u64;
    }
    return 0;
}

u32 emit_definitions(ObjectRows rows, ObjectSymbols symbols, bool native, RestoreObjectSink sink) on cpu
    requires nonzero(sink.take_contract) requires nonzero(sink.definition) {
    u64 index = 0u64;
    while (index < symbols.count) {
        let symbol = unsafe_position<ObjectSymbol>(
            unsafe_address(address_bits(symbols.values) + index * size_of<ObjectSymbol>()));
        if (symbol.external != u8(0) && symbol.section > i16(0)) {
            if (!native && sink.take_contract(sink.context, symbol.name) == 0) { return 19; }
            u64 physical = u64(symbol.section) - 1u64;
            if (physical >= rows.count) { return 20; }
            let row = row_at(rows, physical);
            if (row.mapped == 0u64) { return 20; }
            sink.definition(sink.context, symbol, row.mapped - 1u64, native ? 1 : 0);
        }
        index += 1u64;
    }
    return 0;
}

u32 emit_stack(ObjectRows rows, RestoreSource source, RestoreObjectSink sink) on cpu
    requires nonzero(sink.stack) requires nonzero(sink.frame) requires nonzero(sink.call) {
    let buffers = sink.stack(sink.context);
    u32 error = recover_stack(rows, buffers.frames, buffers.count, buffers.calls,
        buffers.nodes, source.relocation_stride, source.stack_output);
    if (error != 0 || source.stack_output == 0) { return error; }
    u64 frame_index = 0u64;
    u64 call_index = 0u64;
    while (frame_index < buffers.count) {
        let frame = unsafe_position<MetaFrame>(stack_frame_address(buffers.frames, frame_index));
        sink.frame(sink.context, frame);
        u64 calls = frame.calls;
        while (calls != 0u64) {
            let call = unsafe_position<MetaCall>(stack_call_address(buffers.calls, call_index));
            sink.call(sink.context, call);
            call_index += 1u64;
            calls -= 1u64;
        }
        frame_index += 1u64;
    }
    return 0;
}

extern "C" RestoreObjectStatus restore_object(RestoreSource source, RestoreObjectSink sink,
    MetaNameSink names, MetaReceiver metadata) on cpu
    requires nonzero(sink.symbol_name) requires nonzero(sink.section_name)
    requires nonzero(sink.symbols) requires nonzero(sink.sections) requires nonzero(sink.archive)
    requires nonzero(sink.section) requires nonzero(sink.associate) requires nonzero(sink.relocation)
    requires nonzero(sink.relocated) requires nonzero(sink.bind_import) requires nonzero(sink.take_contract) requires nonzero(sink.definition)
    requires nonzero(sink.stack) requires nonzero(sink.frame) requires nonzero(sink.call)
    requires nonzero(names.emit)
    requires nonzero(metadata.kind) requires nonzero(metadata.valid_text) requires nonzero(metadata.binding)
    requires nonzero(metadata.resource) requires nonzero(metadata.code) requires nonzero(metadata.frame)
    requires nonzero(metadata.call) {
    u64 section_count = source.headers.length / 40u64;
    ReadSymbols table = source.table;
    let storage = sink.symbols(sink.context, table.count, section_count);
    let scanned_symbols = object_symbols(source.input, table, storage.rows.values,
        storage.definitions, section_count, sink.name_context, sink.symbol_name);
    if (scanned_symbols.complete == 0) { return RestoreObjectStatus(scanned_symbols.error, 0, 0); }
    let rows = sink.sections(sink.context, section_count);
    ObjectParts parts = ObjectParts(empty_bytes(), empty_bytes(), 0u64);
    let scanned_sections = object_sections(source.input, source.headers, rows.values, address(), parts, table,
        sink.name_context, sink.section_name);
    if (scanned_sections.complete == 0) { return RestoreObjectStatus(scanned_sections.error, 0, 0); }
    bool native = address_bits(source.native_name.bytes) != 0u64;
    if (native) {
        let no_interface = ReadBytes(source.native_name.bytes, 0u64);
        let header = MetaHeader(source.native_name,2u64,no_interface,empty_bytes(),empty_bytes(),
            empty_bytes(),0u64,0u64,0);
        if (metadata.kind(metadata.context, header) == 0) { return RestoreObjectStatus(0, 0, 0); }
    } else {
        if (address_bits(parts.runtime.bytes) != 0u64) { sink.archive(sink.context, parts.runtime); }
        if (address_bits(source.reused_prefix) == 0u64 && address_bits(parts.metadata.bytes) == 0u64) {
            return RestoreObjectStatus(0, 18, 0);
        }
        let recovered = restore_blink_metadata(source, parts, names, metadata);
        if (recovered.complete == 0) { return RestoreObjectStatus(0, 0, recovered.error); }
    }
    u32 error = emit_sections(rows, storage.definitions, sink);
    if (error != 0) { return RestoreObjectStatus(0, error, 0); }
    u64 index = 0u64;
    while (index < rows.count) {
        let slot = row_address(rows, index);
        ObjectSection row = unsafe_position<ObjectSection>(slot);
        if (row.mapped != 0u64) {
            let status = read_object_relocations(source.input, rows, storage.rows, row, sink.context,
                sink.relocation, sink.bind_import, native ? 1 : 0);
            if (status.format != 0 || status.object != 0) {
                return RestoreObjectStatus(status.format, status.object, 0);
            }
            let owned = sink.relocated(sink.context, row.mapped - 1u64);
            row.decoded_relocations = owned.values;
            row.decoded_count = owned.count;
            unsafe_position<ObjectSection>(slot) = row;
        }
        index += 1u64;
    }
    error = emit_definitions(rows, storage.rows, native, sink);
    if (error != 0) { return RestoreObjectStatus(0, error, 0); }
    return RestoreObjectStatus(0, emit_stack(rows, source, sink), 0);
}
