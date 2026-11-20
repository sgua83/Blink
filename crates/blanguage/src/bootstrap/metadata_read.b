module bootstrap.metadata_read;
import blink.windows.coff;

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
ReadBytes since(MetaCursor cursor,u64 start) on cpu {
    return ReadBytes(at(cursor.input.bytes,start),cursor.position-start);
}
MetaField name_group(inout MetaCursor cursor) on cpu {
    u64 start=cursor.position;
    MetaWord count=word(cursor);if(count.error!=0){return MetaField(empty(),count.error);}
    u64 i=0u64;
    while(i<count.value){
        MetaField name=bytes(cursor);if(name.error!=0){return name;}
        i+=1u64;
    }
    return MetaField(since(cursor,start),0);
}
extern "C" MetaPrefix prefix(inout MetaCursor cursor,u64 format) on cpu {
    MetaPrefix value=MetaPrefix(header(cursor,format),empty(),empty(),empty(),empty(),0u64);
    if(value.header.error!=0){return value;}
    u64 start=cursor.position;
    if(value.header.kind==2u64){
        u64 i=0u64;
        while(i<value.header.requirements){
            MetaRequirement item=requirement(cursor);
            if(item.error!=0){value.header.error=item.error;return value;}
            i+=1u64;
        }
        value.requirements=since(cursor,start);
        MetaWord stack=word(cursor);
        if(stack.error!=0){value.header.error=stack.error;return value;}
        value.header.stack=stack.value;
    }
    MetaField libraries=name_group(cursor);
    if(libraries.error!=0){value.header.error=libraries.error;return value;}
    value.libraries=libraries.value;
    MetaField exports=name_group(cursor);
    if(exports.error!=0){value.header.error=exports.error;return value;}
    value.exports=exports.value;
    MetaField resource=bytes(cursor);
    if(resource.error!=0){value.header.error=resource.error;return value;}
    value.resource=resource.value;
    MetaWord count=word(cursor);
    if(count.error!=0){value.header.error=count.error;return value;}
    value.binding_count=count.value;
    return value;
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

const u32 MANIFEST_ABSENT = 1;
const u32 MANIFEST_UTF8 = 11;
const u32 MANIFEST_SELECTION = 12;
const u32 MANIFEST_DECLARATION = 13;
const u32 MANIFEST_TRAILING = 14;

// The first format error ends the cursor and prevents another record from reaching the receiver.
struct ManifestScan { MetaCursor cursor; u32 status; };

u64 manifest_number(inout ManifestScan scan, u64 width) on cpu {
    if (scan.status != 0) { return 0u64; }
    MetaWord field = number(scan.cursor,width,8);
    scan.status = field.error;
    return field.value;
}

ReadBytes manifest_bytes(inout ManifestScan scan, bool text) on cpu {
    if (scan.status != 0) { return empty(); }
    MetaField field = resource_range(scan.cursor,text);
    scan.status = field.error;
    return field.value;
}

ReadBytes manifest_text(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) {
    ReadBytes field = manifest_bytes(scan,true);
    if (scan.status == 0 && !receiver.valid_text(receiver.context,field)) {
        scan.status = MANIFEST_UTF8;
    }
    return field;
}

address manifest_digest(inout ManifestScan scan) on cpu {
    if (scan.status != 0) { return unsafe_address(0u64); }
    MetaField field = take_range(scan.cursor,32u64,false);
    scan.status = field.error;
    return field.value.bytes;
}

address manifest_native_digest(inout ManifestScan scan) on cpu {
    u64 present = manifest_number(scan,1u64);
    if (present == 0u64) { return unsafe_address(0u64); }
    if (present == 1u64) { return manifest_digest(scan); }
    scan.status = 10;
    return unsafe_address(0u64);
}

void manifest_names(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name) {
    u64 count = manifest_number(scan,8u64);
    while (count != 0u64 && scan.status == 0) {
        ReadBytes name = manifest_text(scan,receiver);
        if (scan.status == 0) { receiver.name(receiver.context,name); }
        count -= 1u64;
    }
}

void manifest_selection(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name)
    requires nonzero(receiver.selection) {
    u32 tag = u32(manifest_number(scan,4u64));
    if (tag > 3) { scan.status = MANIFEST_SELECTION; return; }
    ReadBytes entry = empty();
    ReadBytes target = empty();
    if (tag == 1) {
        manifest_names(scan,receiver);
    } else {
        entry = manifest_text(scan,receiver);
        if (tag != 0) { target = manifest_text(scan,receiver); }
    }
    if (scan.status == 0) { receiver.selection(receiver.context,ManifestVariant(entry,target,tag)); }
}

void manifest_declaration(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name)
    requires nonzero(receiver.declaration) {
    u32 tag = u32(manifest_number(scan,4u64));
    if (tag > 7) { scan.status = MANIFEST_DECLARATION; return; }
    ReadBytes name = manifest_text(scan,receiver);
    ReadBytes module_name = empty();
    if (tag >= 4 && tag <= 6) {
        module_name = manifest_text(scan,receiver);
        manifest_names(scan,receiver);
    }
    if (scan.status == 0) { receiver.declaration(receiver.context,ManifestVariant(name,module_name,tag)); }
}

void manifest_source(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.source) {
    ReadBytes module_name = manifest_text(scan,receiver);
    ReadBytes path = manifest_bytes(scan,false);
    ReadBytes scope = manifest_text(scan,receiver);
    address fingerprint = manifest_digest(scan);
    if (scan.status == 0) {
        receiver.source(receiver.context,ManifestReadSource(module_name,path,scope,fingerprint));
    }
}

void manifest_requirement(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.requirement) {
    ReadBytes name = manifest_text(scan,receiver);
    ReadBytes kind = manifest_text(scan,receiver);
    if (scan.status == 0) { receiver.requirement(receiver.context,ManifestReadRequirement(name,kind)); }
}

ManifestIdentity manifest_identity(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) {
    ReadBytes path = manifest_bytes(scan,false);
    ReadBytes root = manifest_bytes(scan,false);
    ReadBytes working_directory = manifest_bytes(scan,false);
    ReadBytes module_name = manifest_text(scan,receiver);
    ReadBytes compiler = manifest_text(scan,receiver);
    u32 max_parameter_bytes = u32(manifest_number(scan,4u64));
    return ManifestIdentity(path,root,working_directory,module_name,compiler,max_parameter_bytes);
}

ManifestReadResult manifest_records(inout ManifestScan scan, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name)
    requires nonzero(receiver.selection) requires nonzero(receiver.source)
    requires nonzero(receiver.requirement) requires nonzero(receiver.declaration) {
    ManifestIdentity identity = manifest_identity(scan,receiver);
    manifest_selection(scan,receiver);
    u64 sources = manifest_number(scan,8u64);
    while (sources != 0u64 && scan.status == 0) {
        manifest_source(scan,receiver);
        sources -= 1u64;
    }
    u64 requirements = manifest_number(scan,8u64);
    while (requirements != 0u64 && scan.status == 0) {
        manifest_requirement(scan,receiver);
        requirements -= 1u64;
    }
    address imports_digest = manifest_digest(scan);
    address native_digest = manifest_native_digest(scan);
    u64 declarations = manifest_number(scan,8u64);
    while (declarations != 0u64 && scan.status == 0) {
        manifest_declaration(scan,receiver);
        declarations -= 1u64;
    }
    if (scan.status == 0 && scan.cursor.position != scan.cursor.input.length) {
        scan.status = MANIFEST_TRAILING;
    }
    return ManifestReadResult(identity,imports_digest,native_digest,scan.status);
}

extern "C" ManifestReadResult read_manifest(ReadBytes input, u64 magic, ManifestReceiver receiver) on cpu
    requires nonzero(receiver.valid_text) requires nonzero(receiver.name)
    requires nonzero(receiver.selection) requires nonzero(receiver.source)
    requires nonzero(receiver.requirement) requires nonzero(receiver.declaration) {
    MetaCursor cursor = MetaCursor(input,0u64);
    MetaWord signature = number(cursor,8u64,8);
    if (signature.error != 0 || signature.value != magic) {
        ManifestIdentity identity = ManifestIdentity(empty(),empty(),empty(),empty(),empty(),0);
        address absent = unsafe_address(0u64);
        return ManifestReadResult(identity,absent,absent,MANIFEST_ABSENT);
    }
    ReadBytes payload = ReadBytes(at(input.bytes,cursor.position),input.length-cursor.position);
    ManifestScan scan = ManifestScan(MetaCursor(payload,0u64),0);
    return manifest_records(scan,receiver);
}
