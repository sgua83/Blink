module bootstrap.coff;
import bootstrap.bindings;
import bootstrap.metadata;
import bootstrap.native_import;

CoffSection section_at(address sections,u64 index) on cpu {
    return unsafe_position<CoffSection>(unsafe_address(address_bits(sections)+index*size_of<CoffSection>()));
}
void section_put(address sections,u64 index,CoffSection value) on cpu {
    unsafe_position<CoffSection>(unsafe_address(address_bits(sections)+index*size_of<CoffSection>()))=value;
}
CoffSymbol symbol_at(CoffInput input,u64 index) on cpu {
    return unsafe_position<CoffSymbol>(unsafe_address(address_bits(input.symbols)+index*SYMBOL_BYTES));
}
CoffLayout invalid(u32 code) on cpu {return CoffLayout(0u64,0u64,0u64,0u64,code);}

CoffLayout layout(CoffInput input) on cpu {
    if(input.section_count>65535u64){return invalid(1);}
    u64 at=20u64+40u64*input.section_count;
    u64 strings=4u64;
    u64 i=0u64;
    while(i<input.section_count){
        CoffSection section=section_at(input.sections,i);
        if(section.byte_length>4294967295u64){return invalid(2);}
        if(at>4294967295u64){return invalid(3);}
        if(section.name_length>8u64){
            // A section's slash-prefixed offset occupies at most seven decimal digits.
            if(strings>=10000000u64||section.name_length>=4294967295u64){return invalid(6);}
            strings+=section.name_length+1u64;
            if(strings>4294967295u64){return invalid(6);}
        }
        if(section.uninitialized!=0u64){
            section.offset=0u64;
            section.relocation_offset=0u64;
        }else{
            section.offset=at;at+=section.byte_length;
            if(at>4294967295u64){return invalid(3);}
            section.relocation_offset=at;
        }
        if(section.relocation_count>=4294967295u64){return invalid(4);}
        if(section.relocation_count>=65535u64){
            section.flags=section.flags|16777216u64;
            at+=COFF_RELOCATION_RECORD_BYTES;
        }
        at+=COFF_RELOCATION_RECORD_BYTES*section.relocation_count;
        section_put(input.sections,i,section);i+=1u64;
    }
    if(at>4294967295u64){return invalid(3);}
    u64 count=0u64;i=0u64;
    while(i<input.symbol_count){
        CoffSymbol symbol=symbol_at(input,i);
        count+=1u64+symbol.auxiliary;
        if(count>4294967295u64){return invalid(5);}
        if(symbol.name_length>8u64){
            if(symbol.name_length>4294967295u64){return invalid(6);}
            strings+=symbol.name_length+1u64;
            if(strings>4294967295u64){return invalid(6);}
        }
        i+=1u64;
    }
    u64 names=at+18u64*count;
    return CoffLayout(names+strings,at,names,count,0);
}
void put(address output,u64 at,u32 value) on cpu {
    unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(value);
}
void le16(address output,u64 at,u64 value) on cpu {
    put(output,at,u32(value)&255);put(output,at+1u64,u32(value>>8u64)&255);
}
void le32(address output,u64 at,u64 value) on cpu {
    u64 i=0u64;
    while(i<4u64){put(output,at+i,u32(value>>(i*8u64))&255);i+=1u64;}
}
void zero(address output,u64 at,u64 length) on cpu {
    u64 i=0u64;
    while(i<length){unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=u8(0);i+=1u64;}
}
void copy(address output,u64 at,address bytes,u64 length) on cpu {
    u64 i=0u64;
    while(i<length){
        unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=unsafe_position<u8>(unsafe_address(address_bits(bytes)+i));
        i+=1u64;
    }
}
u8 name_byte(address name,u64 index) on cpu {
    return unsafe_position<u8>(unsafe_address(address_bits(name)+index));
}
extern "C" u32 needs_body(u64 flags,u64 relocation_count) on cpu {
    return u32((flags&128u64)!=0u64 && relocation_count!=0u64);
}
CoffSection physical_section(CoffSection original,CoffWork work) on cpu requires nonzero(work.name) {
    CoffSection section=original;
    if(needs_body(section.flags,section.relocation_count)!=0){
        section.flags=(section.flags&~128u64)|64u64;
        // LINK groups conventional sections by the name before the dollar suffix.
        if(section.name_length>=4u64 && name_byte(section.name,0u64)==u8(46) &&
            name_byte(section.name,1u64)==u8(98) && name_byte(section.name,2u64)==u8(115) &&
            name_byte(section.name,3u64)==u8(115) &&
            (section.name_length==4u64 || name_byte(section.name,4u64)==u8(36))){
            address name=work.data_name;
            if(section.name_length!=4u64){
                name=work.name(work.context,section.name_length+1u64);
                copy(name,0u64,work.data_name,5u64);
                copy(name,5u64,unsafe_address(address_bits(section.name)+4u64),section.name_length-4u64);
            }
            section.name=name;section.name_length+=1u64;
        }
    }
    section.uninitialized=u64(section.uninitialized!=0u64 && (section.flags&128u64)!=0u64);
    return section;
}

struct BindingPlan {u64 symbols;u64 names_root;u64 imports_root;u32 error;};

BindingRelocation relocation_at(cfn<BindingRelocation,address,u64> read_relocation,address relocations,u64 index) on cpu {
    if(read_relocation!=cfn<BindingRelocation,address,u64>()){
        return read_relocation(relocations,index);
    }
    address record=unsafe_address(address_bits(relocations)+index*size_of<BindingRelocation>());
    BindingRelocation value=unsafe_position<BindingRelocation>(record);
    if(value.target==u16(0)){
        value.name=BindingBytes(address_of(unsafe_position<BindingRelocation>(record).section),size_of<u64>());
        value.dll=BindingBytes(address_of(unsafe_position<BindingRelocation>(record).value),size_of<u32>());
    }
    return value;
}
BindingKey reference_key(BindingRelocation value) on cpu {
    if(value.target==u16(0)){return BindingKey(value.name,value.dll);}
    return BindingKey(value.name,BindingBytes(unsafe_address(0u64),0u64));
}
void symbol_put(BindingStorage storage,u64 index,CoffSymbol value) on cpu {
    unsafe_position<CoffSymbol>(unsafe_address(address_bits(storage.symbols)+index*SYMBOL_BYTES))=value;
}
CoffSymbol symbol(BindingBytes name,u64 section,u64 value,u64 function,u64 external,u64 auxiliary,u64 owner,u64 generated,u64 source) on cpu {
    return CoffSymbol(name.bytes,name.length,section,value,function,external,auxiliary,owner,generated,source);
}
BindingPlan invalid_bindings(u32 error) on cpu {return BindingPlan(0u64,0u64,0u64,error);}
BindingPlan prepare_bindings(CoffSource source,CoffWork work,BindingStorage storage) on cpu
    requires nonzero(source.next_definition) {
    if(source.bound_count>32767u64){return invalid_bindings(7);}
    BindingBytes empty=BindingBytes(unsafe_address(0u64),0u64);
    BindingForest forest=BindingForest(storage.nodes,0u64,0u64,0u64);
    u64 i=0u64;
    while(i<source.bound_count){
        CoffSection section=section_at(source.sections,i);u64 j=0u64;
        while(j<section.associated_count){
            u64 child=unsafe_position<u64>(unsafe_address(address_bits(section.associated)+j*8u64));
            CoffSection value=section_at(source.sections,child);value.owner=i+1u64;section_put(source.sections,child,value);j+=1u64;
        }
        i+=1u64;
    }
    // The private COMDAT symbols follow all ordinary section symbols.
    u64 count=source.bound_count;u64 row=0u64;i=0u64;
    while(i<source.bound_count){
        CoffSection section=section_at(source.sections,i);section.symbol=row;section_put(source.sections,i,section);
        bool comdat=(section.flags&4096u64)!=0u64;
        u64 auxiliary=u64(comdat);
        symbol_put(storage,i,symbol(BindingBytes(section.name,section.name_length),i+1u64,0u64,0u64,0u64,auxiliary,section.owner,0u64,0u64));
        row+=1u64+auxiliary;
        if(comdat){
            u64 length=section_symbol_name_size(source.name,i);
            symbol_put(storage,count,symbol(BindingBytes(unsafe_address(0u64),length),i+1u64,0u64,0u64,0u64,0u64,0u64,1u64,i));
            count+=1u64;
        }
        i+=1u64;
    }
    row+=count-source.bound_count;
    i=0u64;
    while(i<source.definition_count){
        BindingDefinition value=source.next_definition(work.context);
        if(value.section>=32767u64){return invalid_bindings(7);}
        forest.names=insert_binding(forest,forest.names,BindingKey(value.name,empty),row,true).root;
        symbol_put(storage,count,symbol(value.name,value.section+1u64,value.value,value.function,1u64,0u64,0u64,0u64,0u64));
        count+=1u64;row+=1u64;i+=1u64;
    }
    i=0u64;
    while(i<source.bound_count){
        CoffSection section=section_at(source.sections,i);u64 j=0u64;
        while(j<section.relocation_count){
            BindingRelocation value=relocation_at(source.read_relocation,section.relocations,j);
            if(value.offset>4294967295u64){return invalid_bindings(8);}
            if(value.target==u16(2)){
                BindingKey key=BindingKey(value.dll,value.name);
                BindingInsert inserted=insert_binding(forest,forest.imports,key,0u64,false);forest.imports=inserted.root;
            }else if(value.target==u16(1) || value.value!=0){
                BindingKey key=reference_key(value);
                BindingInsert insertion=insert_binding(forest,forest.names,key,row,false);
                forest.names=insertion.root;
                if(insertion.inserted){
                    if(value.target==u16(0)){
                        CoffSection target=section_at(source.sections,value.section);
                        symbol_put(storage,count,symbol(BindingBytes(target.name,target.name_length),value.section+1u64,u64(value.value),0u64,0u64,0u64,0u64,0u64,0u64));
                    }else{
                        symbol_put(storage,count,symbol(value.name,0u64,0u64,1u64,1u64,0u64,0u64,0u64,0u64));
                    }
                    count+=1u64;row+=1u64;
                }
            }
            j+=1u64;
        }
        i+=1u64;
    }
    u64 at=first(forest,forest.imports);
    while(at!=0u64){
        BindingNode node=node_at(forest,at);
        u64 length=import_name_size(node.key.first,node.key.second);
        symbol_put(storage,count,symbol(BindingBytes(unsafe_address(0u64),length),0u64,0u64,1u64,1u64,0u64,0u64,3u64,at));
        if (work.emit_import != cfn<void,address,BindingKey>()) {
            work.emit_import(work.import_context, node.key);
        }
        node.value=row;node_put(forest,at,node);count+=1u64;row+=1u64;
        
        at=next(forest,at);
    }
    return BindingPlan(count,forest.names,forest.imports,0);
}

u64 bound_symbol(BindingForest forest,address sections,BindingRelocation value) on cpu {
    if(value.target==u16(0) && value.value==0){
        return section_at(sections,value.section).symbol;
    }
    if(value.target==u16(2)){
        return node_at(forest,find(forest,forest.imports,BindingKey(value.dll,value.name))).value;
    }
    return node_at(forest,find(forest,forest.names,reference_key(value))).value;
}

extern "C" u64 relocation_symbol(read CoffBindingSource binding,u64 index,u64 relocation) on cpu {
    BindingForest forest=BindingForest(binding.nodes,0u64,binding.names_root,binding.imports_root);
    CoffSection section=section_at(binding.sections,index);
    return bound_symbol(forest,binding.sections,relocation_at(binding.read_relocation,section.relocations,relocation));
}

void write_section_relocations(read CoffBindingSource binding,u64 index,address output) on cpu {
    BindingForest forest=BindingForest(binding.nodes,0u64,binding.names_root,binding.imports_root);
    CoffSection section=section_at(binding.sections,index);
    u64 i=0u64;
    while(i<section.relocation_count){
        BindingRelocation value=relocation_at(binding.read_relocation,section.relocations,i);
        u64 target=bound_symbol(forest,binding.sections,value);
        u64 at=i*COFF_RELOCATION_RECORD_BYTES;
        le32(output,at,value.offset);le32(output,at+4u64,target);le16(output,at+8u64,u64(value.kind));
        i+=1u64;
    }
}

extern "C" void write_generated_name(read CoffBindingSource binding,u64 kind,u64 source,address output) on cpu {
    if(kind==1u64){
        section_symbol_name(BindingBytes(binding.module_name,binding.module_name_length),source,output);
    }else{
        BindingForest forest=BindingForest(binding.nodes,0u64,0u64,0u64);
        BindingKey key=node_at(forest,source).key;
        import_name(key.first,key.second,output);
    }
}

extern "C" u32 with_output(CoffSource source,CoffWork work) on cpu
    requires nonzero(source.next_definition) requires nonzero(work.reserve)
    requires nonzero(work.name) requires nonzero(work.consume) {
    CoffInput input=CoffInput(source.sections,source.section_count,unsafe_address(0u64),0u64);
    u64 references=source.definition_count;
    u64 i=0u64;
    while(i<source.section_count){
        CoffSection section;
        if(source.next_section!=cfn<CoffSection,address>()){
            section=source.next_section(work.context);
        }else{
            section=section_at(input.sections,i);
        }
        section=physical_section(section,work);
        section_put(input.sections,i,section);
        references+=section.relocation_count;
        i+=1u64;
    }
    BindingStorage storage=work.reserve(work.context,2u64*source.bound_count+references,references);
    BindingPlan bindings=prepare_bindings(source,work,storage);
    if(bindings.error!=0){return bindings.error;}
    input.symbols=storage.symbols;input.symbol_count=bindings.symbols;
    CoffLayout shape=layout(input);
    if(shape.error!=0){return shape.error;}
    CoffBindingSource binding=CoffBindingSource(source.name.bytes,source.name.length,source.sections,
        storage.nodes,bindings.names_root,bindings.imports_root,source.read_relocation);
    work.consume(work.context,CoffPlan(input,shape,binding));
    return 0;
}

extern "C" void write(read CoffPlan plan,address output,MetadataStackHost stack) on cpu
    requires nonzero(stack.frame) requires nonzero(stack.call) {
    CoffInput input=plan.input;
    CoffLayout layout=plan.layout;
    CoffBindingSource binding=plan.binding;
    zero(output,0u64,20u64+40u64*input.section_count);
    le16(output,0u64,34404u64);le16(output,2u64,input.section_count);
    le32(output,8u64,layout.symbols);le32(output,12u64,layout.symbol_count);
    u64 strings=4u64;
    u64 i=0u64;
    while(i<input.section_count){
        CoffSection section=section_at(input.sections,i);
        u64 h=20u64+40u64*i;
        if(section.name_length<=8u64){
            copy(output,h,section.name,section.name_length);
        }else{
            put(output,h,47);
            u64 digit=h+1u64;
            u64 remaining=strings;
            while(remaining>=10u64){digit+=1u64;remaining/=10u64;}
            remaining=strings;
            while(digit>h){
                put(output,digit,u32(remaining%10u64)+48);
                digit-=1u64;remaining/=10u64;
            }
            copy(output,layout.strings+strings,section.name,section.name_length);
            strings+=section.name_length;
            put(output,layout.strings+strings,0);
            strings+=1u64;
        }
        le32(output,h+16u64,section.byte_length);le32(output,h+20u64,section.offset);
        le32(output,h+24u64,section.relocation_offset);
        u64 count=section.relocation_count;
        u64 at=section.relocation_offset;
        if(count>=65535u64){
            count=65535u64;
            le32(output,at,section.relocation_count+1u64);zero(output,at+4u64,6u64);at+=COFF_RELOCATION_RECORD_BYTES;
        }
        le16(output,h+32u64,count);le32(output,h+36u64,section.flags);
        if(section.relocation_count!=0u64){
            address destination=unsafe_address(address_bits(output)+at);
            write_section_relocations(binding,i,destination);
        }
        i+=1u64;
    }
    u64 at=layout.symbols;i=0u64;
    le32(output,layout.strings,layout.length-layout.strings);
    while(i<input.symbol_count){
        CoffSymbol symbol=symbol_at(input,i);
        zero(output,at,18u64);
        u64 name_offset=at;
        if(symbol.name_length>8u64){
            le32(output,at+4u64,strings);
            name_offset=layout.strings+strings;
            strings+=symbol.name_length;
            put(output,layout.strings+strings,0);
            strings+=1u64;
        }
        if(symbol.generated==0u64){
            copy(output,name_offset,symbol.name,symbol.name_length);
        }else{
            address destination=unsafe_address(address_bits(output)+name_offset);
            write_generated_name(binding,symbol.generated,symbol.source,destination);
        }
        le32(output,at+8u64,symbol.value);le16(output,at+12u64,symbol.section);
        if(symbol.function!=0u64){le16(output,at+14u64,32u64);}
        u32 storage=3;if(symbol.external!=0u64){storage=2;}put(output,at+16u64,storage);
        put(output,at+17u64,u32(symbol.auxiliary));at+=18u64;
        if(symbol.auxiliary!=0u64){
            CoffSection section=section_at(input.sections,symbol.section-1u64);
            zero(output,at,18u64);le32(output,at,section.byte_length);
            u64 count=section.relocation_count;if(count>65535u64){count=65535u64;}
            le16(output,at+4u64,count);le16(output,at+12u64,symbol.owner);
            u32 selection=1;if(symbol.owner!=0u64){selection=5;}put(output,at+14u64,selection);
            at+=18u64;
        }
        i+=1u64;
    }
    // The original owners write generated payloads into their final ranges.
    i=0u64;
    while(i<input.section_count){
        CoffSection section=section_at(input.sections,i);
        if(section.offset!=0u64){
            address destination=unsafe_address(address_bits(output)+section.offset);
            if(section.body_kind==1u64){
                bootstrap.metadata::encode(unsafe_position<MetadataInput>(section.body),destination,stack);
            }else if(section.body_kind==2u64){
                NativeImportBody body=unsafe_position<NativeImportBody>(section.body);
                encode_section(body.input,body.plan,i,section.byte_length,destination);
            }else if(address_bits(section.body)==0u64){
                zero(output,section.offset,section.byte_length);
            }else{
                copy(output,section.offset,section.body,section.byte_length);
            }
        }
        i+=1u64;
    }
}

extern "C" void host_headers(address output, u64 sections, u64 symbols, u64 cache_end, u64 cache_length) on cpu {
    le16(output, 2u64, sections);
    le32(output, 8u64, symbols);
    u64 section = 0u64;
    while (section < sections) {
        u64 field = 20u64 + 40u64 * section + 20u64;
        u64 end = field + 12u64;
        while (field < end) {
            u64 original = 0u64;
            u64 byte = 0u64;
            while (byte < 4u64) {
                original |= u64(unsafe_position<u8>(unsafe_address(address_bits(output) + field + byte))) << (byte * 8u64);
                byte += 1u64;
            }
            if (original != 0u64) {
                u64 removed = 40u64;
                if (original >= cache_end) { removed += cache_length; }
                le32(output, field, original - removed);
            }
            field += 4u64;
        }
        section += 1u64;
    }
}

extern "C" void resolve_native_object_targets(address context, NativeObjectTargetHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.external) requires nonzero(host.print) {
    NativeObjectTarget target = host.next(context);
    while (target.kind != NativeObjectTargetKind::Done) {
        usize section = target.kind == NativeObjectTargetKind::External
            ? host.external(context, target.symbol, target.payload)
            : host.print(context, target.symbol, target.payload);
        unsafe_position<usize>(target.output) = section;
        target = host.next(context);
    }
}

extern "C" void bind_native_object_fixups(address context, usize images, NativeObjectFixupHost host) on cpu
    requires nonzero(host.image) requires nonzero(host.field)
    requires nonzero(host.section) requires nonzero(host.symbol) requires nonzero(host.missing) {
    usize next = 0u64;
    while (next < images) {
        NativeObjectImage image = host.image(context, next);
        usize index = 0u64;
        while (index < image.fields) {
            NativeObjectFixup field = host.field(context, next, index);
            if (field.place.present != 0) {
                host.section(context, image.section, field.offset, field.place.section);
            } else if (field.imported != 0) {
                host.symbol(context, image.section, field.offset, field.name);
            } else {
                host.missing(context, field.name);
            }
            index += 1u64;
        }
        next += 1u64;
    }
}
