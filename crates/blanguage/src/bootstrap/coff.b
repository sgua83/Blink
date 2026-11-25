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
    BindingPlan bindings=prepare(source,work,storage);
    if(bindings.error!=0){return bindings.error;}
    input.symbols=storage.symbols;input.symbol_count=bindings.symbols;
    CoffLayout shape=layout(input);
    if(shape.error!=0){return shape.error;}
    CoffBindingSource binding=CoffBindingSource(source.name.bytes,source.name.length,source.sections,
        storage.nodes,bindings.names_root,bindings.imports_root,source.read_relocation);
    work.consume(work.context,CoffPlan(input,shape,binding));
    return 0;
}

extern "C" void write(read CoffPlan plan,address output) on cpu {
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
                bootstrap.metadata::encode(unsafe_position<MetadataInput>(section.body),destination);
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

extern "C" address resolve_native_object_targets(address context, NativeObjectTargetHost host) on cpu
    requires nonzero(host.next) requires nonzero(host.imported)
    requires nonzero(host.external) requires nonzero(host.print) {
    NativeObjectTarget target = host.next(context);
    while (target.kind != NativeObjectTargetKind::Done) {
        NativeObjectPlace place = target.defined;
        if (target.kind == NativeObjectTargetKind::Function) {
            if (place.present == 0 && host.imported(context, target.payload) == 0) {
                return target.payload;
            }
        } else if (target.kind == NativeObjectTargetKind::External) {
            place = NativeObjectPlace(host.external(context, target.symbol, target.payload), 1);
        } else {
            place = NativeObjectPlace(host.print(context, target.symbol, target.payload), 1);
        }
        unsafe_position<NativeObjectPlace>(target.output) = place;
        target = host.next(context);
    }
    return unsafe_address(0u64);
}

extern "C" void bind_native_object_fixups(address context, usize images, NativeObjectFixupHost host) on cpu
    requires nonzero(host.image) requires nonzero(host.field)
    requires nonzero(host.section) requires nonzero(host.symbol) {
    usize next = 0u64;
    while (next < images) {
        NativeObjectImage image = host.image(context, next);
        usize index = 0u64;
        while (index < image.fields) {
            NativeObjectFixup field = host.field(context, next, index);
            if (field.place.present != 0) {
                host.section(context, image.section, field.offset, field.place.section);
            } else {
                host.symbol(context, image.section, field.offset, field.name);
            }
            index += 1u64;
        }
        next += 1u64;
    }
}
