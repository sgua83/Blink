module bootstrap.metadata_read;
import bootstrap.coff_read;

ReadBytes empty() on cpu {return ReadBytes(unsafe_address(0u64),0u64);}
address at(address bytes,u64 offset) on cpu {return unsafe_address(address_bits(bytes)+offset);}
extern "C" MetaWord word(inout MetaCursor cursor) on cpu {
    if(cursor.input.length-cursor.position<4u64){return MetaWord(0u64,1);}
    u64 value=0u64;u64 i=0u64;
    while(i<4u64){
        value=value|(u64(unsafe_position<u8>(at(cursor.input.bytes,cursor.position+i)))<<(i*8u64));i+=1u64;
    }
    cursor.position+=4u64;return MetaWord(value,0);
}
extern "C" MetaField bytes(inout MetaCursor cursor) on cpu {
    MetaWord length=word(cursor);if(length.error!=0){return MetaField(empty(),length.error);}
    if(length.value>cursor.input.length-cursor.position){return MetaField(empty(),2);}
    ReadBytes value=ReadBytes(at(cursor.input.bytes,cursor.position),length.value);
    cursor.position+=length.value;return MetaField(value,0);
}
MetaHeader bad_header(u32 error) on cpu {
    return MetaHeader(empty(),0u64,empty(),empty(),empty(),empty(),0u64,0u64,error);
}
extern "C" MetaHeader header(inout MetaCursor cursor,u64 format) on cpu {
    MetaWord magic=word(cursor);if(magic.error!=0){return bad_header(magic.error);}
    if(magic.value!=1329810498u64){return bad_header(3);}
    MetaWord version=word(cursor);if(version.error!=0){return bad_header(version.error);}
    if(version.value!=format){return bad_header(3);}
    MetaField name=bytes(cursor);if(name.error!=0){return bad_header(name.error);}
    MetaWord kind=word(cursor);if(kind.error!=0){return bad_header(kind.error);}
    if(kind.value>2u64){return bad_header(4);}
    MetaHeader value=MetaHeader(name.value,kind.value,empty(),empty(),empty(),empty(),0u64,0u64,0);
    if(kind.value==2u64){
        MetaField interface=bytes(cursor);if(interface.error!=0){return bad_header(interface.error);}
        MetaWord count=word(cursor);if(count.error!=0){return bad_header(count.error);}
        value.interface=interface.value;value.requirements=count.value;
    }else if(kind.value==1u64){
        MetaField prefix=bytes(cursor);if(prefix.error!=0){return bad_header(prefix.error);}
        MetaField header=bytes(cursor);if(header.error!=0){return bad_header(header.error);}
        MetaField binding=bytes(cursor);if(binding.error!=0){return bad_header(binding.error);}
        value.prefix=prefix.value;value.header=header.value;value.binding=binding.value;
    }else{
        MetaWord stack=word(cursor);if(stack.error!=0){return bad_header(stack.error);}
        value.stack=stack.value;
    }
    return value;
}
extern "C" MetaRequirement requirement(inout MetaCursor cursor) on cpu {
    MetaField name=bytes(cursor);if(name.error!=0){return MetaRequirement(empty(),0u64,name.error);}
    MetaWord kind=word(cursor);if(kind.error!=0){return MetaRequirement(empty(),0u64,kind.error);}
    if(kind.value>2u64){return MetaRequirement(empty(),0u64,5);}
    return MetaRequirement(name.value,kind.value,0);
}
extern "C" MetaBinding binding(inout MetaCursor cursor) on cpu {
    MetaField name=bytes(cursor);if(name.error!=0){return MetaBinding(empty(),empty(),name.error);}
    MetaField value=bytes(cursor);return MetaBinding(name.value,value.value,value.error);
}
MetaFrame bad_frame(u32 error) on cpu {return MetaFrame(empty(),0u64,0u64,0u64,0u64,0u64,error);}
extern "C" MetaFrame frame(inout MetaCursor cursor) on cpu {
    MetaField name=bytes(cursor);if(name.error!=0){return bad_frame(name.error);}
    MetaWord has_coff=word(cursor);if(has_coff.error!=0){return bad_frame(has_coff.error);}
    if(has_coff.value>1u64){return bad_frame(6);}
    u64 section=0u64;u64 start=0u64;
    if(has_coff.value==1u64){
        MetaWord first=word(cursor);if(first.error!=0){return bad_frame(first.error);}
        MetaWord second=word(cursor);if(second.error!=0){return bad_frame(second.error);}
        section=first.value;start=second.value;
    }
    MetaWord local=word(cursor);if(local.error!=0){return bad_frame(local.error);}
    MetaWord calls=word(cursor);if(calls.error!=0){return bad_frame(calls.error);}
    return MetaFrame(name.value,has_coff.value,section,start,local.value,calls.value,0);
}
extern "C" MetaCall call(inout MetaCursor cursor) on cpu {
    MetaWord kind=word(cursor);if(kind.error!=0){return MetaCall(0u64,0u64,empty(),kind.error);}
    if(kind.value>3u64){return MetaCall(0u64,0u64,empty(),7);}
    u64 offset=0u64;ReadBytes reason=empty();
    if(kind.value<2u64){
        MetaWord value=word(cursor);if(value.error!=0){return MetaCall(0u64,0u64,empty(),value.error);}
        offset=value.value;
    }else if(kind.value==3u64){
        MetaField value=bytes(cursor);if(value.error!=0){return MetaCall(0u64,0u64,empty(),value.error);}
        reason=value.value;
    }
    return MetaCall(kind.value,offset,reason,0);
}
