module bootstrap.object_read;
import bootstrap.coff_read;

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
    return unsafe_position<ObjectSection>(unsafe_address(address_bits(rows.values)+index*OBJECT_SECTION_BYTES));
}
extern "C" ObjectSectionResult logical_section(ReadBytes input,ReadBytes headers,u64 index,inout u64 mapped) on cpu {
    ReadSection row=section(input,headers,index);
    ObjectSection value=ObjectSection(row.name,row.bytes,row.flags,row.relocations,row.relocation_count,0u64);
    if(row.error!=0){return ObjectSectionResult(value,row.error);}
    array<u8,7> meta=array<u8,7>(u8(46),u8(98),u8(108),u8(109),u8(101),u8(116),u8(97));array<u8,5> runtime=array<u8,5>(u8(46),u8(98),u8(108),u8(114),u8(116));array<u8,8> build=array<u8,8>(u8(46),u8(98),u8(108),u8(98),u8(117),u8(105),u8(108),u8(100));
    if(!matches<7>(row.name,meta,true) &&
       !matches<5>(row.name,runtime,true) &&
       !matches<8>(row.name,build,true)){
        mapped+=1u64;value.mapped=mapped;
    }
    return ObjectSectionResult(value,0);
}
extern "C" ObjectAssociation association(ObjectRows rows,u64 selection,u64 owner) on cpu {
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
extern "C" ObjectTarget resolve_target(ObjectRows rows,ReadBytes name,i64 section,u64 value) on cpu {
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
