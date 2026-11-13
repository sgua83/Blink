module bootstrap.metadata_read;
import bootstrap.coff_read;

ReadBytes empty() on cpu {return ReadBytes(unsafe_address(0u64),0u64);}
address at(address bytes,u64 offset) on cpu {return unsafe_address(address_bits(bytes)+offset);}
MetaWord number(inout MetaCursor cursor,u64 width,u32 error) on cpu {
    if(cursor.input.length-cursor.position<width){return MetaWord(0u64,error);}
    u64 value=0u64;u64 i=0u64;
    while(i<width){
        value=value|(u64(unsafe_position<u8>(at(cursor.input.bytes,cursor.position+i)))<<(i*8u64));
        i+=1u64;
    }
    cursor.position+=width;
    return MetaWord(value,0);
}
extern "C" MetaWord word(inout MetaCursor cursor) on cpu {
    return number(cursor,4u64,1);
}
MetaField take_range(inout MetaCursor cursor, u64 length, bool text) on cpu {
    if (length > cursor.input.length - cursor.position) {
        if (text && length > 18446744073709551615u64 - cursor.position) { return MetaField(empty(),9); }
        return MetaField(empty(),8);
    }
    ReadBytes field = ReadBytes(at(cursor.input.bytes,cursor.position),length);
    cursor.position += length;
    return MetaField(field,0);
}
MetaField resource_range(inout MetaCursor cursor,bool text) on cpu {
    MetaWord length=number(cursor,8u64,8);
    if(length.error!=0){return MetaField(empty(),length.error);}
    return take_range(cursor,length.value,text);
}
extern "C" MetaField resource_text(inout MetaCursor cursor) on cpu {
    return resource_range(cursor,true);
}
extern "C" MetaResourceHead resource_head(inout MetaCursor cursor) on cpu {
    MetaWord present=number(cursor,1u64,8);
    if(present.error!=0){return MetaResourceHead(0u64,empty(),present.error);}
    if(present.value==0u64){return MetaResourceHead(0u64,empty(),0);}
    if(present.value!=1u64){return MetaResourceHead(0u64,empty(),10);}
    MetaField prefix=resource_range(cursor,true);
    return MetaResourceHead(1u64,prefix.value,prefix.error);
}
extern "C" MetaResourceBody resource_body(inout MetaCursor cursor) on cpu {
    MetaWord types=number(cursor,4u64,8);
    if(types.error!=0){return MetaResourceBody(0u64,0u64,empty(),0u64,types.error);}
    MetaWord programs=number(cursor,4u64,8);
    if(programs.error!=0){return MetaResourceBody(0u64,0u64,empty(),0u64,programs.error);}
    MetaField abi=resource_range(cursor,false);
    if(abi.error!=0){return MetaResourceBody(0u64,0u64,empty(),0u64,abi.error);}
    MetaWord code=number(cursor,8u64,8);
    return MetaResourceBody(types.value,programs.value,abi.value,code.value,code.error);
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

extern "C" BuildImage read_build_image(ReadBytes input,u64 magic) on cpu {
    MetaCursor cursor=MetaCursor(input,0u64);
    MetaWord signature=number(cursor,8u64,1);
    if(signature.error!=0 || signature.value!=magic){return BuildImage(empty(),empty(),1);}
    MetaField identity=resource_range(cursor,false);
    if(identity.error!=0){return BuildImage(empty(),empty(),2);}
    MetaField image=resource_range(cursor,false);
    if(image.error!=0){return BuildImage(empty(),empty(),2);}
    u32 status=cursor.position==input.length ? 0 : 3;
    return BuildImage(identity.value,image.value,status);
}

extern "C" ReadBytes manifest_body(ReadBytes input, u64 magic) on cpu {
    MetaCursor cursor = MetaCursor(input,0u64);
    MetaWord signature = number(cursor,8u64,8);
    if (signature.error != 0 || signature.value != magic) { return empty(); }
    return ReadBytes(at(input.bytes,cursor.position),input.length-cursor.position);
}

extern "C" MetaWord manifest_number(inout MetaCursor cursor, bool wide) on cpu {
    return number(cursor,wide ? 8u64 : 4u64,8);
}

extern "C" MetaField manifest_field(inout MetaCursor cursor, ManifestFieldKind kind) on cpu {
    if (kind == ManifestFieldKind::Text || kind == ManifestFieldKind::Bytes) {
        return resource_range(cursor,kind == ManifestFieldKind::Text);
    }
    if (kind == ManifestFieldKind::OptionalDigest) {
        MetaWord present = number(cursor,1u64,8);
        if (present.error != 0 || present.value == 0u64) { return MetaField(empty(),present.error); }
        if (present.value != 1u64) { return MetaField(empty(),10); }
    }
    return take_range(cursor,32u64,false);
}
