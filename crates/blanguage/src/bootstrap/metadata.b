module bootstrap.metadata;

MetadataBytes bytes_at(address values,u64 index) on cpu {
    return unsafe_position<MetadataBytes>(unsafe_address(address_bits(values)+index*BYTES_BYTES));
}
MetadataRequirement requirement_at(MetadataInput input,u64 index) on cpu {
    return unsafe_position<MetadataRequirement>(unsafe_address(address_bits(input.requirements)+index*REQUIREMENT_BYTES));
}
MetadataBinding binding_at(address bindings, u64 index) on cpu {
    return unsafe_position<MetadataBinding>(unsafe_address(address_bits(bindings) + index * BINDING_BYTES));
}
u64 binding_index(address order, u64 index) on cpu {
    return unsafe_position<u64>(unsafe_address(address_bits(order) + index * 8u64));
}
void put_binding_index(address order, u64 index, u64 value) on cpu {
    unsafe_position<u64>(unsafe_address(address_bits(order) + index * 8u64)) = value;
}
i32 compare_binding_bytes(MetadataBytes left, MetadataBytes right) on cpu {
    return compare_name(NameBytes(left.bytes, left.length), NameBytes(right.bytes, right.length));
}
extern "C" bool binding_before(address left, address right, address bindings) on cpu {
    u64 a = unsafe_position<u64>(left);
    u64 b = unsafe_position<u64>(right);
    i32 names = compare_binding_bytes(binding_at(bindings, a).name, binding_at(bindings, b).name);
    return names < 0 || (names == 0 && a < b);
}
extern "C" BindingOrder order_bindings(address bindings, u64 definitions, u64 count, address order) on cpu {
    // Place the definition permutation after enough space for all imports.
    // Each output consumes at least one input, so the write cursor stays
    // behind the unread definition indices throughout the merge.
    u64 definition_start = count - definitions;
    address definition_order = unsafe_address(address_bits(order) + definition_start * 8u64);
    u64 at = 0u64;
    while (at < definitions) {
        put_binding_index(definition_order, at, at);
        at += 1u64;
    }
    sort_rows<u64>(definition_order, definitions, bindings, c_function(binding_before));

    u64 unique = 0u64;
    u64 imported = definitions;
    at = 0u64;
    while (at < definitions) {
        u64 selected = binding_index(definition_order, at);
        MetadataBinding binding = binding_at(bindings, selected);
        i32 relation = i32(1);
        if (imported < count) {
            relation = compare_binding_bytes(binding_at(bindings, imported).name, binding.name);
        }
        if (relation < 0) {
            put_binding_index(order, unique, imported);
            unique += 1u64;
            imported += 1u64;
            continue;
        }

        at += 1u64;
        while (at < definitions) {
            u64 index = binding_index(definition_order, at);
            MetadataBinding next = binding_at(bindings, index);
            if (compare_binding_bytes(binding.name, next.name) != 0) { break; }
            selected = index;
            binding = next;
            at += 1u64;
        }
        if (relation == 0) {
            if (compare_binding_bytes(binding.value, binding_at(bindings, imported).value) != 0) {
                return BindingOrder(unique, imported);
            }
            selected = imported;
            imported += 1u64;
        }
        put_binding_index(order, unique, selected);
        unique += 1u64;
    }
    while (imported < count) {
        put_binding_index(order, unique, imported);
        unique += 1u64;
        imported += 1u64;
    }
    return BindingOrder(unique, count);
}
MetadataBinding ordered_binding_at(MetadataInput input, u64 index) on cpu {
    return binding_at(input.bindings, binding_index(input.binding_order, index));
}
MetadataFrame frame_at(MetadataInput input,u64 index) on cpu {
    return unsafe_position<MetadataFrame>(unsafe_address(address_bits(input.frames)+index*FRAME_BYTES));
}
MetadataCall call_at(MetadataInput input,u64 index) on cpu {
    return unsafe_position<MetadataCall>(unsafe_address(address_bits(input.calls)+index*CALL_BYTES));
}
MetadataLayout invalid(u32 error) on cpu {return MetadataLayout(0u64,error);}

extern "C" MetadataLayout layout(MetadataInput input) on cpu {
    if(input.name.length>4294967295u64){return invalid(2);}
    u64 at=16u64+input.name.length;
    if(input.kind==0u64){at+=4u64;}
    else if(input.kind==1u64){
        if(input.prefix.length>4294967295u64 || input.header.length>4294967295u64 || input.binding.length>4294967295u64){return invalid(2);}
        at+=12u64+input.prefix.length+input.header.length+input.binding.length;
    }else{
        if(input.interface.length>4294967295u64){return invalid(2);}
        if(input.requirement_count>4294967295u64){return invalid(1);}
        at+=12u64+input.interface.length;
        if(at>4294967295u64){return invalid(3);}
        u64 i=0u64;
        while(i<input.requirement_count){
            MetadataRequirement requirement=requirement_at(input,i);
            if(requirement.name.length>4294967295u64){return invalid(2);}
            at+=8u64+requirement.name.length;
            if(at>4294967295u64){return invalid(3);}
            i+=1u64;
        }
    }
    if(at>4294967295u64){return invalid(3);}
    if(input.library_count>4294967295u64 || input.export_count>4294967295u64 || input.binding_count>4294967295u64 || input.frame_count>4294967295u64){return invalid(1);}
    at+=4u64;u64 i=0u64;
    while(i<input.library_count){
        MetadataBytes value=bytes_at(input.libraries,i);
        if(value.length>4294967295u64){return invalid(2);}
        at+=4u64+value.length;if(at>4294967295u64){return invalid(3);}i+=1u64;
    }
    at+=4u64;i=0u64;
    while(i<input.export_count){
        MetadataBytes value=bytes_at(input.exports,i);
        if(value.length>4294967295u64){return invalid(2);}
        at+=4u64+value.length;if(at>4294967295u64){return invalid(3);}i+=1u64;
    }
    u64 resource_length=1u64;
    if(input.resource.present!=0u64){
        resource_length+=32u64+input.resource.prefix.length+input.resource.abi.length;
        if(resource_length>4294967295u64){return invalid(2);}
        i=0u64;
        while(i<input.resource.code_count){
            resource_length+=8u64+bytes_at(input.resource.code,i).length;
            if(resource_length>4294967295u64){return invalid(2);}
            i+=1u64;
        }
    }
    at+=4u64+resource_length;
    if(at>4294967295u64){return invalid(3);}
    at+=4u64;i=0u64;
    while(i<input.binding_count){
        MetadataBinding value=ordered_binding_at(input,i);
        if(value.name.length>4294967295u64 || value.value.length>4294967295u64){return invalid(2);}
        at+=8u64+value.name.length+value.value.length;
        if(at>4294967295u64){return invalid(3);}i+=1u64;
    }
    at+=4u64;i=0u64;
    u64 call_index=0u64;
    while(i<input.frame_count){
        MetadataFrame frame=frame_at(input,i);
        if(frame.name.length>4294967295u64){return invalid(2);}
        if(frame.call_count>4294967295u64){return invalid(1);}
        at+=16u64+frame.name.length;
        if(frame.has_coff!=0u64){
            if(frame.section>4294967295u64){return invalid(2);}
            at+=8u64;
        }
        if(at>4294967295u64){return invalid(3);}
        u64 call_end=call_index+frame.call_count;
        while(call_index<call_end){
            MetadataCall call=call_at(input,call_index);
            at+=4u64;
            if(call.kind<2u64){
                if(call.offset>4294967295u64){return invalid(2);}
                at+=4u64;
            }else if(call.kind==3u64){
                if(call.reason.length>4294967295u64){return invalid(2);}
                at+=4u64+call.reason.length;
            }
            if(at>4294967295u64){return invalid(3);}call_index+=1u64;
        }
        i+=1u64;
    }
    if(at>4294967295u64){return invalid(3);}
    return MetadataLayout(at,0);
}
u64 word(address output,u64 at,u64 value) on cpu {
    u64 i=0u64;
    while(i<4u64){
        unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=u8(u32(value>>(i*8u64))&255);
        i+=1u64;
    }
    return at+4u64;
}
void copy(address output,u64 at,MetadataBytes value) on cpu {
    u64 i=0u64;
    while(i<value.length){
        unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+i));
        i+=1u64;
    }
}
u64 bytes(address output,u64 at,MetadataBytes value) on cpu {
    u64 begin=word(output,at,value.length);copy(output,begin,value);return begin+value.length;
}
u64 wide_word(address output,u64 at,u64 value) on cpu {
    u64 next=word(output,at,value&4294967295u64);
    return word(output,next,value>>32u64);
}
u64 resource_field(address output,u64 at,MetadataBytes value) on cpu {
    u64 begin=wide_word(output,at,value.length);
    copy(output,begin,value);
    return begin+value.length;
}
u64 encode_resource(address output,u64 at,MetadataResource resource) on cpu {
    u64 begin=at+4u64;
    unsafe_position<u8>(unsafe_address(address_bits(output)+begin))=u8(resource.present);
    u64 next=begin+1u64;
    if(resource.present!=0u64){
        next=resource_field(output,next,resource.prefix);
        next=word(output,next,resource.types);
        next=word(output,next,resource.programs);
        next=resource_field(output,next,resource.abi);
        next=wide_word(output,next,resource.code_count);
        u64 i=0u64;
        while(i<resource.code_count){
            next=resource_field(output,next,bytes_at(resource.code,i));
            i+=1u64;
        }
    }
    word(output,at,next-begin);
    return next;
}
extern "C" void encode(MetadataInput input,address output) on cpu {
    u64 at=word(output,0u64,1329810498u64);
    at=word(output,at,input.format_version);at=bytes(output,at,input.name);at=word(output,at,input.kind);
    if(input.kind==0u64){at=word(output,at,input.stack_bytes);}
    else if(input.kind==1u64){
        at=bytes(output,at,input.prefix);at=bytes(output,at,input.header);at=bytes(output,at,input.binding);
    }else{
        at=bytes(output,at,input.interface);at=word(output,at,input.requirement_count);
        u64 i=0u64;
        while(i<input.requirement_count){
            MetadataRequirement requirement=requirement_at(input,i);
            at=bytes(output,at,requirement.name);at=word(output,at,requirement.kind);i+=1u64;
        }
        at=word(output,at,input.stack_bytes);
    }
    at=word(output,at,input.library_count);u64 i=0u64;
    while(i<input.library_count){at=bytes(output,at,bytes_at(input.libraries,i));i+=1u64;}
    at=word(output,at,input.export_count);i=0u64;
    while(i<input.export_count){at=bytes(output,at,bytes_at(input.exports,i));i+=1u64;}
    at=encode_resource(output,at,input.resource);at=word(output,at,input.binding_count);i=0u64;
    while(i<input.binding_count){
        MetadataBinding binding=ordered_binding_at(input,i);
        at=bytes(output,at,binding.name);at=bytes(output,at,binding.value);i+=1u64;
    }
    at=word(output,at,input.frame_count);i=0u64;
    u64 call_index=0u64;
    while(i<input.frame_count){
        MetadataFrame frame=frame_at(input,i);
        at=bytes(output,at,frame.name);at=word(output,at,frame.has_coff);
        if(frame.has_coff!=0u64){at=word(output,at,frame.section);at=word(output,at,frame.start);}
        at=word(output,at,frame.local);at=word(output,at,frame.call_count);
        u64 call_end=call_index+frame.call_count;
        while(call_index<call_end){
            MetadataCall call=call_at(input,call_index);at=word(output,at,call.kind);
            if(call.kind<2u64){at=word(output,at,call.offset);}
            else if(call.kind==3u64){at=bytes(output,at,call.reason);}
            call_index+=1u64;
        }
        i+=1u64;
    }
}

bool text_at(MetadataBytes text,u64 at,MetadataBytes expected) on cpu {
    u64 index=0u64;
    while (index<expected.length) {
        u8 actual=unsafe_position<u8>(unsafe_address(address_bits(text.bytes)+at+index));
        u8 wanted=unsafe_position<u8>(unsafe_address(address_bits(expected.bytes)+index));
        if (actual!=wanted) { return false; }
        index+=1u64;
    }
    return true;
}

extern "C" u64 rust_link_binding(MetadataBytes binding,MetadataBytes prefix,
    MetadataBytes library,address output) on cpu {
    if (prefix.length==library.length && text_at(prefix,0u64,library)) { return 0u64; }

    // Match the exact generated Rust directive: #[link(name="...")]
    let opening=array<u8,13>(u8(35),u8(91),u8(108),u8(105),u8(110),u8(107),u8(40),
        u8(110),u8(97),u8(109),u8(101),u8(61),u8(34));
    let closing=array<u8,3>(u8(34),u8(41),u8(93));
    let open=MetadataBytes(address_of(opening[0]),13u64);
    let close=MetadataBytes(address_of(closing[0]),3u64);
    u64 directive_bytes=open.length+prefix.length+close.length;
    bool render=address_bits(output)!=0u64;

    u64 at=0u64;
    u64 copied=0u64;
    u64 written=0u64;
    while (at+directive_bytes<=binding.length) {
        u64 name=at+open.length;
        if (text_at(binding,at,open) && text_at(binding,name,prefix) &&
                text_at(binding,name+prefix.length,close)) {
            u64 unchanged=name-copied;
            if (render) {
                address start=unsafe_address(address_bits(binding.bytes)+copied);
                copy(output,written,MetadataBytes(start,unchanged));
                copy(output,written+unchanged,library);
            }
            written+=unchanged+library.length;
            copied=name+prefix.length;
            at+=directive_bytes;
        } else {
            at+=1u64;
        }
    }
    if (copied==0u64) { return 0u64; }
    u64 remaining=binding.length-copied;
    if (render) {
        address start=unsafe_address(address_bits(binding.bytes)+copied);
        copy(output,written,MetadataBytes(start,remaining));
    }
    return written+remaining;
}

extern "C" void encode_build_image(u64 magic,MetadataBytes identity,MetadataBytes image,address output) on cpu {
    u64 at=wide_word(output,0u64,magic);
    at=resource_field(output,at,identity);
    resource_field(output,at,image);
}

struct ManifestWriter { address output; u64 cursor; };

void manifest_word(inout ManifestWriter writer, u32 value) on cpu {
    word(writer.output, writer.cursor, u64(value));
    writer.cursor += 4u64;
}

void manifest_count(inout ManifestWriter writer, u64 value) on cpu {
    wide_word(writer.output, writer.cursor, value);
    writer.cursor += 8u64;
}

void manifest_payload(inout ManifestWriter writer, MetadataBytes value) on cpu {
    copy(writer.output, writer.cursor, value);
    writer.cursor += value.length;
}

void manifest_field(inout ManifestWriter writer, MetadataBytes value) on cpu {
    manifest_count(writer, value.length);
    manifest_payload(writer, value);
}

void manifest_fields(inout ManifestWriter writer, address values, u64 start, u64 count) on cpu {
    manifest_count(writer, count);
    u64 index = 0u64;
    while (index < count) {
        manifest_field(writer, bytes_at(values, start + index));
        index += 1u64;
    }
}

void manifest_request(inout ManifestWriter writer, ManifestSelection selection) on cpu {
    manifest_word(writer, selection.tag);
    if (selection.tag == 1) {
        manifest_fields(writer, selection.instances, 0u64, selection.instance_count);
    } else {
        manifest_field(writer, selection.first);
        if (selection.tag != 0) { manifest_field(writer, selection.second); }
    }
}

void manifest_sources(inout ManifestWriter writer, BuildManifest input) on cpu {
    manifest_count(writer, input.source_count);
    u64 index = 0u64;
    while (index < input.source_count) {
        address row = unsafe_address(address_bits(input.sources) + index * size_of<ManifestSource>());
        ManifestSource source = unsafe_position<ManifestSource>(row);
        manifest_field(writer, source.module_name);
        manifest_field(writer, source.path);
        manifest_field(writer, source.scope);
        manifest_payload(writer, MetadataBytes(source.fingerprint, 32u64));
        index += 1u64;
    }

    manifest_count(writer, input.requirement_count);
    index = 0u64;
    while (index < input.requirement_count) {
        address row = unsafe_address(address_bits(input.requirements) + index * size_of<ManifestRequirement>());
        ManifestRequirement requirement = unsafe_position<ManifestRequirement>(row);
        manifest_field(writer, requirement.name);
        manifest_field(writer, requirement.kind);
        index += 1u64;
    }
}

void manifest_declarations(inout ManifestWriter writer, BuildManifest input) on cpu {
    manifest_count(writer, input.declaration_count);
    u64 index = 0u64;
    while (index < input.declaration_count) {
        address row = unsafe_address(address_bits(input.declarations) + index * size_of<ManifestDeclaration>());
        ManifestDeclaration declaration = unsafe_position<ManifestDeclaration>(row);
        manifest_word(writer, declaration.tag);
        manifest_field(writer, declaration.name);
        if (declaration.tag >= 4 && declaration.tag <= 6) {
            manifest_field(writer, declaration.module_name);
            manifest_fields(writer, input.declaration_imports, declaration.import_start, declaration.import_count);
        }
        index += 1u64;
    }
}

u64 manifest_field_length(MetadataBytes field) on cpu {
    return 8u64 + field.length;
}

u64 manifest_list_length(address values, u64 start, u64 count) on cpu {
    u64 length = 8u64;
    u64 index = 0u64;
    while (index < count) {
        length += manifest_field_length(bytes_at(values, start + index));
        index += 1u64;
    }
    return length;
}

extern "C" u64 build_manifest_length(BuildManifest input) on cpu {
    u64 length = 8u64; // BLBO magic and version
    length += manifest_field_length(input.path) + manifest_field_length(input.root);
    length += manifest_field_length(input.working_directory) + manifest_field_length(input.module_name);
    length += manifest_field_length(input.compiler) + 4u64; // parameter-byte limit

    length += 4u64; // selection tag
    ManifestSelection selection = input.selection;
    if (selection.tag == 1) {
        length += manifest_list_length(selection.instances, 0u64, selection.instance_count);
    } else {
        length += manifest_field_length(selection.first);
        if (selection.tag != 0) { length += manifest_field_length(selection.second); }
    }

    length += 8u64; // source count
    u64 index = 0u64;
    while (index < input.source_count) {
        address row = unsafe_address(address_bits(input.sources) + index * size_of<ManifestSource>());
        ManifestSource source = unsafe_position<ManifestSource>(row);
        length += manifest_field_length(source.module_name) + manifest_field_length(source.path);
        length += manifest_field_length(source.scope) + 32u64;
        index += 1u64;
    }

    length += 8u64; // requirement count
    index = 0u64;
    while (index < input.requirement_count) {
        address row = unsafe_address(address_bits(input.requirements) + index * size_of<ManifestRequirement>());
        ManifestRequirement requirement = unsafe_position<ManifestRequirement>(row);
        length += manifest_field_length(requirement.name) + manifest_field_length(requirement.kind);
        index += 1u64;
    }

    length += 33u64; // imports digest and native digest presence
    if (address_bits(input.native_digest) != 0u64) { length += 32u64; }
    length += 8u64; // declaration count
    index = 0u64;
    while (index < input.declaration_count) {
        address row = unsafe_address(address_bits(input.declarations) + index * size_of<ManifestDeclaration>());
        ManifestDeclaration declaration = unsafe_position<ManifestDeclaration>(row);
        length += 4u64 + manifest_field_length(declaration.name);
        if (declaration.tag >= 4 && declaration.tag <= 6) {
            length += manifest_field_length(declaration.module_name);
            length += manifest_list_length(input.declaration_imports, declaration.import_start, declaration.import_count);
        }
        index += 1u64;
    }
    return length;
}

extern "C" u64 encode_build_manifest(u64 magic, BuildManifest input, address output) on cpu {
    ManifestWriter writer = ManifestWriter(output, 0u64);
    manifest_count(writer, magic);
    manifest_field(writer, input.path);
    manifest_field(writer, input.root);
    manifest_field(writer, input.working_directory);
    manifest_field(writer, input.module_name);
    manifest_field(writer, input.compiler);
    manifest_word(writer, input.max_parameter_bytes);
    manifest_request(writer, input.selection);
    manifest_sources(writer, input);
    manifest_payload(writer, MetadataBytes(input.imports_digest, 32u64));
    bool has_native = address_bits(input.native_digest) != 0u64;
    unsafe_position<u8>(unsafe_address(address_bits(output) + writer.cursor)) = u8(has_native);
    writer.cursor += 1u64;
    if (has_native) { manifest_payload(writer, MetadataBytes(input.native_digest, 32u64)); }
    manifest_declarations(writer, input);
    return writer.cursor;
}
