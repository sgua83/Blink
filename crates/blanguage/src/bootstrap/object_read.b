module bootstrap.object_read;
import blink.windows.coff;
import bootstrap.metadata_read;
import bootstrap.bindings;

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
ObjectSection row_at(ObjectRows rows,u64 index) on cpu {
    return unsafe_position<ObjectSection>(unsafe_address(address_bits(rows.values)+index*size_of<ObjectSection>()));
}
extern "C" ObjectSectionResult logical_section(ReadBytes input, ReadBytes headers, u64 index,
    inout u64 mapped, inout ObjectParts parts) on cpu {
    ReadSection row=section(input,headers,index);
    ObjectSection value=ObjectSection(row.name,row.bytes,row.flags,row.relocations,row.relocation_count,0u64,row.zero_fill,unsafe_address(0u64),0u64);
    if(row.error!=0){return ObjectSectionResult(value,row.error);}
    array<u8,7> meta=array<u8,7>(u8(46),u8(98),u8(108),u8(109),u8(101),u8(116),u8(97));array<u8,5> runtime=array<u8,5>(u8(46),u8(98),u8(108),u8(114),u8(116));array<u8,8> build=array<u8,8>(u8(46),u8(98),u8(108),u8(98),u8(117),u8(105),u8(108),u8(100));
    if (matches<7>(row.name,meta,true)) {
        if (address_bits(parts.metadata.bytes) == 0u64) { parts.metadata = row.bytes; }
    } else if (matches<5>(row.name,runtime,true)) {
        if (address_bits(parts.runtime.bytes) == 0u64) { parts.runtime = row.bytes; }
    } else if (!matches<8>(row.name,build,true)) {
        mapped += 1u64;
        value.mapped = mapped;
    }
    return ObjectSectionResult(value,0);
}
address definition_address(address definitions,u64 index) on cpu {
    return unsafe_address(address_bits(definitions)+index*4u64);
}
extern "C" ObjectSymbolResult object_symbol(
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
extern "C" ObjectAssociation association(ObjectRows rows,address definitions,u64 index) on cpu {
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
ObjectTarget failure(u32 error) on cpu {
    return ObjectTarget(0u64,0u64,0u64,empty_bytes(),empty_bytes(),error);
}
ObjectTarget import_target(ReadBytes name,u64 prefix_length) on cpu {
    u64 end=prefix_length;
    while(end<name.length && byte_at(name,end)!=36){end+=1u64;}
    if(end==name.length){return failure(6);}
    u64 index=prefix_length;
    if(index==end){return failure(7);}
    if(byte_at(name,index)==43){index+=1u64;if(index==end){return failure(8);}}
    u64 length=0u64;
    while(index<end){
        u32 digit=byte_at(name,index);
        if(digit<48 || digit>57){return failure(8);}
        u64 value=u64(digit-48);
        if(length>(18446744073709551615u64-value)/10u64){return failure(9);}
        length=length*10u64+value;index+=1u64;
    }
    u64 start=end+1u64;u64 available=name.length-start;
    if(length>available){return failure(10);}
    u64 boundary=start+length;
    if(boundary<name.length && (byte_at(name,boundary)&192)==128){return failure(10);}
    return ObjectTarget(1u64,0u64,0u64,part(name,start,length),part(name,boundary,available-length),0);
}
ObjectTarget resolve_target(ObjectRows rows,ReadBytes name,i64 section,u64 value) on cpu {
    if(section>i64(0)){
        u64 index=u64(section)-1u64;
        if(index>=rows.count){return failure(5);}
        ObjectSection row=row_at(rows,index);
        if(row.mapped==0u64){return failure(5);}
        return ObjectTarget(0u64,row.mapped-1u64,value,empty_bytes(),empty_bytes(),0);
    }
    array<u8,15> prefix=array<u8,15>(u8(95),u8(95),u8(98),u8(108),u8(105),u8(110),u8(107),u8(95),u8(105),u8(109),u8(112),u8(111),u8(114),u8(116),u8(36));
    if(matches<15>(name,prefix,false)){return import_target(name,15u64);}
    return ObjectTarget(2u64,0u64,0u64,empty_bytes(),name,0);
}

extern "C" ObjectRelocation object_relocation(
    ReadBytes input,ObjectRows rows,ObjectSymbols symbols,u64 table,u64 index
) on cpu {
    ReadRelocation record=relocation(input,table,index);
    if(record.error!=0){return ObjectRelocation(record,failure(0));}
    if(record.symbol>=symbols.count){return ObjectRelocation(record,failure(15));}
    address row=unsafe_address(address_bits(symbols.values)+record.symbol*24u64);
    ObjectSymbol symbol=unsafe_position<ObjectSymbol>(row);
    if(address_bits(symbol.name.bytes)==0u64){return ObjectRelocation(record,failure(15));}
    ObjectTarget target=resolve_target(rows,symbol.name,i64(symbol.section),u64(symbol.value));
    return ObjectRelocation(record,target);
}

address stack_frame_address(address frames,u64 index) on cpu {
    return unsafe_address(address_bits(frames)+index*64u64);
}
MetaCall stack_call_at(address calls,u64 index) on cpu {
    return unsafe_position<MetaCall>(unsafe_address(address_bits(calls)+index*40u64));
}
bool stack_relocation(ObjectSection section,u64 offset,u64 stride) on cpu {
    u64 start=address_bits(section.decoded_relocations);
    u64 i=0u64;
    while(i<section.decoded_count){
        if(unsafe_position<u64>(unsafe_address(start+i*stride))==offset){return true;}
        i+=1u64;
    }
    return false;
}
extern "C" u32 recover_stack(ObjectRows rows,address frames,u64 count,address calls,address nodes,u64 stride) on cpu {
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
            u64 before=positions.count;
            BindingInsert inserted=put(positions,root,BindingKey(location,empty),0u64);
            if(positions.count==before){return 12;}
            root=inserted.root;
        }
        u64 remaining=frame.calls;
        while(remaining!=0u64){
            MetaCall call=stack_call_at(calls,next_call);
            next_call+=1u64;
            remaining-=1u64;
            if(call.kind<2u64){
                if(frame.has_coff==0u64){return 13;}
                if(!stack_relocation(row_at(rows,frame.section),call.offset,stride)){return 14;}
            }
        }
        i+=1u64;
    }
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
