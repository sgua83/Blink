module bootstrap.native_import;
import bootstrap.bindings;
import bootstrap.coff;

u8 byte_at(address bytes,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(bytes)+i));}
void put(address bytes,u64 i,u8 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(bytes)+i))=value;}
bool equal(BindingBytes a,BindingBytes b) on cpu {
    if(a.length!=b.length){return false;}u64 i=0u64;while(i<a.length){if(byte_at(a.bytes,i)!=byte_at(b.bytes,i)){return false;}i+=1u64;}return true;
}
void copy(address output,u64 at,BindingBytes bytes) on cpu {u64 i=0u64;while(i<bytes.length){put(output,at+i,byte_at(bytes.bytes,i));i+=1u64;}}
void literal(address output,u64 at,u64 value,u64 length) on cpu {u64 i=0u64;while(i<length){put(output,at+i,u8(value>>(i*8u64)));i+=1u64;}}
void section_put(NativeImportOutput output,u64 index,u64 name_index,u64 length,u64 flags,
    u64 first,u64 count,u64 associated_count) on cpu {
    address name_row = unsafe_address(address_bits(output.names)+name_index*size_of<BindingBytes>());
    BindingBytes name = unsafe_position<BindingBytes>(name_row);
    address relocations = unsafe_address(address_bits(output.relocations)+first*size_of<BindingRelocation>());
    address row = unsafe_address(address_bits(output.sections)+index*size_of<CoffSection>());
    unsafe_position<CoffSection>(row) = CoffSection(name.bytes,name.length,length,flags,
        count,0u64,0u64,0u64,relocations,output.associated,associated_count,0u64,0u64,
        unsafe_address(0u64),0u64);
}

void relocation_put(NativeImportOutput output,u64 index,BindingRelocation relocation) on cpu {
    address row = unsafe_address(address_bits(output.relocations)+index*size_of<BindingRelocation>());
    unsafe_position<BindingRelocation>(row) = relocation;
}

void relative_relocation(NativeImportOutput output,u64 index,u64 offset,u64 target) on cpu {
    BindingBytes empty = BindingBytes(unsafe_address(0u64),0u64);
    relocation_put(output,index,BindingRelocation(offset,u16(3),u16(0),0,target,empty,empty));
}

// Physical DLL section order. The IAT slot owns the other sections.
const u64 DLL_IAT = 0u64;
const u64 DLL_TERMINATOR = 1u64;
const u64 DLL_LOOKUP = 2u64;
const u64 DLL_DESCRIPTOR = 3u64;
const u64 DLL_NAMES = 4u64;
const u64 DLL_SECTIONS = 5u64;

extern "C" NativeImportPlan plan_import(NativeImportInput input,NativeImportOutput output) on cpu {
    bool frame = equal(input.dll,input.runtime);
    u64 slot_start = 15u64; // "@blink.imports@"
    u64 slot_end = slot_start+import_name_size(input.dll,input.name);
    u64 hint = frame ? 0u64 : (input.dll.length+2u64)&(0u64-2u64);
    u64 names = frame ? 0u64 : hint+3u64+input.name.length;
    NativeImportPlan plan = NativeImportPlan(slot_start,slot_end,slot_end+4u64,hint,
        frame ? 1u64 : DLL_SECTIONS,0u64);
    if(names>4294967295u64 || plan.member>4294967295u64){
        plan.error=1u64;
        return plan;
    }
    if(frame){
        section_put(output,0u64,5u64,8u64,0x40401040u64,0u64,1u64,0u64);
        BindingBytes empty = BindingBytes(unsafe_address(0u64),0u64);
        relocation_put(output,0u64,BindingRelocation(0u64,u16(1),u16(1),0,0u64,input.name,empty));
        return plan;
    }

    u64 index=1u64;
    while(index<DLL_SECTIONS){
        address entry = unsafe_address(address_bits(output.associated)+(index-1u64)*8u64);
        unsafe_position<u64>(entry)=index;
        index+=1u64;
    }
    section_put(output,DLL_IAT,3u64,16u64,0xc0401040u64,0u64,1u64,DLL_SECTIONS-1u64);
    section_put(output,DLL_TERMINATOR,1u64,20u64,0xc0301040u64,1u64,0u64,0u64);
    section_put(output,DLL_LOOKUP,2u64,16u64,0xc0401040u64,1u64,1u64,0u64);
    section_put(output,DLL_DESCRIPTOR,0u64,20u64,0xc0301040u64,2u64,3u64,0u64);
    section_put(output,DLL_NAMES,4u64,names,0xc0201040u64,5u64,0u64,0u64);
    relative_relocation(output,0u64,0u64,DLL_NAMES);
    relative_relocation(output,1u64,0u64,DLL_NAMES);
    relative_relocation(output,2u64,0u64,DLL_LOOKUP);
    relative_relocation(output,3u64,12u64,DLL_NAMES);
    relative_relocation(output,4u64,16u64,DLL_IAT);
    return plan;
}

extern "C" void encode_member_name(NativeImportInput input,NativeImportPlan plan,address output) on cpu {
    literal(output,0u64,7579113344870408768u64,8u64);
    literal(output,8u64,18141342482788461u64,7u64);
    import_name(input.dll,input.name,unsafe_address(address_bits(output)+plan.slot_start));
    literal(output,plan.slot_end,1784835886u64,4u64); // ".obj"
}

extern "C" void encode_section(NativeImportInput input,NativeImportPlan plan,u64 section,u64 length,address output) on cpu {
    u64 begin=0u64;
    u64 end=length;
    if(plan.sections==DLL_SECTIONS){
        if(section==DLL_NAMES){
            u64 symbol=plan.hint+2u64;
            copy(output,0u64,input.dll);
            copy(output,symbol,input.name);
            put(output,symbol+input.name.length,u8(0));
            // DLL terminator, alignment and the two-byte hint.
            begin=input.dll.length;
            end=symbol;
        }else if(section==DLL_LOOKUP || section==DLL_IAT){
            literal(output,0u64,plan.hint,8u64);
            begin=8u64;
        }
    }

    while(begin<end){
        put(output,begin,u8(0));
        begin+=1u64;
    }
}
