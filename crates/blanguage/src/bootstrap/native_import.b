module bootstrap.native_import;
import bootstrap.bindings;

u8 byte_at(address bytes,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(bytes)+i));}
void put(address bytes,u64 i,u8 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(bytes)+i))=value;}
bool equal(BindingBytes a,BindingBytes b) on cpu {
    if(a.length!=b.length){return false;}u64 i=0u64;while(i<a.length){if(byte_at(a.bytes,i)!=byte_at(b.bytes,i)){return false;}i+=1u64;}return true;
}
void copy(address output,u64 at,BindingBytes bytes) on cpu {u64 i=0u64;while(i<bytes.length){put(output,at+i,byte_at(bytes.bytes,i));i+=1u64;}}
void literal(address output,u64 at,u64 value,u64 length) on cpu {u64 i=0u64;while(i<length){put(output,at+i,u8(value>>(i*8u64)));i+=1u64;}}
void section_put(NativeImportOutput output,u64 index,u64 name,u64 length,u64 flags) on cpu {
    u64 owner = index == 0u64 ? 0u64 : 1u64;
    address row = unsafe_address(address_bits(output.sections)+index*IMPORT_SECTION_BYTES);
    unsafe_position<NativeImportSection>(row) =
        NativeImportSection(name,length,flags,owner,unsafe_address(0u64));
}
NativeImportSection section_at(NativeImportOutput output,u64 i) on cpu {return unsafe_position<NativeImportSection>(unsafe_address(address_bits(output.sections)+i*IMPORT_SECTION_BYTES));}
void relocation_put(NativeImportOutput output,u64 i,NativeImportRelocation value) on cpu {unsafe_position<NativeImportRelocation>(unsafe_address(address_bits(output.relocations)+i*IMPORT_RELOCATION_BYTES))=value;}
// Physical DLL section order. The IAT slot owns the other sections.
const u64 DLL_IAT = 0u64;
const u64 DLL_TERMINATOR = 1u64;
const u64 DLL_LOOKUP = 2u64;
const u64 DLL_DESCRIPTOR = 3u64;
const u64 DLL_NAMES = 4u64;
const u64 DLL_SECTIONS = 5u64;

extern "C" NativeImportPlan plan_import(NativeImportInput input,NativeImportOutput output) on cpu {
    bool frame = equal(input.dll,input.runtime);
    u64 slot = import_name_size(input.dll,input.name);
    u64 hint = frame ? 0u64 : (input.dll.length+2u64)&(0u64-2u64);
    u64 names = frame ? 0u64 : hint+3u64+input.name.length;
    NativeImportPlan plan = NativeImportPlan(slot,15u64+slot,19u64+slot,hint,
        frame ? 1u64 : 5u64,frame ? 1u64 : DLL_SECTIONS,0u64);
    if(names>4294967295u64 || plan.member>4294967295u64){
        plan.error=1u64;
        return plan;
    }
    if(frame){
        section_put(output,0u64,5u64,8u64,0x40401040u64);
        relocation_put(output,0u64,NativeImportRelocation(0u64,0u64,1u64,1u64,0u64,0u64));
        return plan;
    }
    section_put(output,DLL_IAT,3u64,16u64,0xc0401040u64);
    section_put(output,DLL_TERMINATOR,1u64,20u64,0xc0301040u64);
    section_put(output,DLL_LOOKUP,2u64,16u64,0xc0401040u64);
    section_put(output,DLL_DESCRIPTOR,0u64,20u64,0xc0301040u64);
    section_put(output,DLL_NAMES,4u64,names,0xc0201040u64);
    relocation_put(output,0u64,NativeImportRelocation(DLL_DESCRIPTOR,0u64,3u64,0u64,DLL_LOOKUP,0u64));
    relocation_put(output,1u64,NativeImportRelocation(DLL_DESCRIPTOR,12u64,3u64,0u64,DLL_NAMES,0u64));
    relocation_put(output,2u64,NativeImportRelocation(DLL_DESCRIPTOR,16u64,3u64,0u64,DLL_IAT,0u64));
    relocation_put(output,3u64,NativeImportRelocation(DLL_LOOKUP,0u64,3u64,0u64,DLL_NAMES,0u64));
    relocation_put(output,4u64,NativeImportRelocation(DLL_IAT,0u64,3u64,0u64,DLL_NAMES,0u64));
    return plan;
}
extern "C" void encode_import(NativeImportInput input,NativeImportOutput output,NativeImportPlan plan) on cpu {
    import_name(input.dll,input.name,output.slot);
    // "@blink.imports@", followed by the one existing slot name.
    literal(output.stem,0u64,7579113344870408768u64,8u64);literal(output.stem,8u64,18141342482788461u64,7u64);
    copy(output.stem,15u64,BindingBytes(output.slot,plan.slot));copy(output.member,0u64,BindingBytes(output.stem,plan.stem));
    literal(output.member,plan.stem,1784835886u64,4u64); // ".obj"
    u64 index=0u64;
    while(index<plan.sections){
        NativeImportSection section=section_at(output,index);
        u64 offset=0u64;
        while(offset<section.length){
            put(section.bytes,offset,u8(0));
            offset+=1u64;
        }
        index+=1u64;
    }
    if(plan.sections==DLL_SECTIONS){
        address names=section_at(output,DLL_NAMES).bytes;
        copy(names,0u64,input.dll);
        copy(names,plan.hint+2u64,input.name);
        literal(section_at(output,DLL_LOOKUP).bytes,0u64,plan.hint,8u64);
        literal(section_at(output,DLL_IAT).bytes,0u64,plan.hint,8u64);
    }
}
