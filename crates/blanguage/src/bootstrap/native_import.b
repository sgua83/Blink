module bootstrap.native_import;
import bootstrap.bindings;

u8 byte_at(address bytes,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(bytes)+i));}
void put(address bytes,u64 i,u8 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(bytes)+i))=value;}
bool equal(BindingBytes a,BindingBytes b) on cpu {
    if(a.length!=b.length){return false;}u64 i=0u64;while(i<a.length){if(byte_at(a.bytes,i)!=byte_at(b.bytes,i)){return false;}i+=1u64;}return true;
}
void copy(address output,u64 at,BindingBytes bytes) on cpu {u64 i=0u64;while(i<bytes.length){put(output,at+i,byte_at(bytes.bytes,i));i+=1u64;}}
void literal(address output,u64 at,u64 value,u64 length) on cpu {u64 i=0u64;while(i<length){put(output,at+i,u8(value>>(i*8u64)));i+=1u64;}}
void section_put(NativeImportOutput output,u64 i,NativeImportSection value) on cpu {unsafe_position<NativeImportSection>(unsafe_address(address_bits(output.sections)+i*IMPORT_SECTION_BYTES))=value;}
NativeImportSection section_at(NativeImportOutput output,u64 i) on cpu {return unsafe_position<NativeImportSection>(unsafe_address(address_bits(output.sections)+i*IMPORT_SECTION_BYTES));}
void relocation_put(NativeImportOutput output,u64 i,NativeImportRelocation value) on cpu {unsafe_position<NativeImportRelocation>(unsafe_address(address_bits(output.relocations)+i*IMPORT_RELOCATION_BYTES))=value;}
u64 mapped(u64 root,u64 section) on cpu {if(section==0u64){return root;}if(section==root){return 0u64;}return section;}
extern "C" NativeImportPlan plan_import(NativeImportInput input,NativeImportOutput output) on cpu {
    u64 slot=import_name_size(input.dll,input.name);u64 root=3u64;u64 hint=0u64;u64 names=0u64;u64 relocations=5u64;
    if(equal(input.dll,input.runtime)){root=5u64;relocations=1u64;}else{hint=(input.dll.length+2u64)&(0u64-2u64);names=hint+3u64+input.name.length;}
    NativeImportPlan plan=NativeImportPlan(slot,15u64+slot,19u64+slot,hint,relocations,root,0u64);
    if(names>4294967295u64||plan.member>4294967295u64){plan.error=1u64;return plan;}
    u64 i=0u64;
    while(i<6u64){
        u64 length=0u64;u64 flags=3221225536u64;if(i==5u64){flags=1073741888u64;}
        if(i==0u64||i==1u64){flags=flags|3145728u64;}else if(i==2u64||i==3u64||i==5u64){flags=flags|4194304u64;}else{flags=flags|2097152u64;}
        if(root==5u64){if(i==5u64){length=8u64;}}else{
            if(i==0u64||i==1u64){length=20u64;}else if(i==2u64||i==3u64){length=16u64;}else if(i==4u64){length=names;}
        }
        u64 index=mapped(root,i);u64 owner=1u64;if(index==0u64){owner=0u64;}
        section_put(output,index,NativeImportSection(i,length,flags|4096u64,owner,unsafe_address(0u64)));i+=1u64;
    }
    if(root==5u64){relocation_put(output,0u64,NativeImportRelocation(0u64,0u64,1u64,1u64,0u64,0u64));}else{
        relocation_put(output,0u64,NativeImportRelocation(mapped(root,0u64),0u64,3u64,0u64,mapped(root,2u64),0u64));
        relocation_put(output,1u64,NativeImportRelocation(mapped(root,0u64),12u64,3u64,0u64,mapped(root,4u64),0u64));
        relocation_put(output,2u64,NativeImportRelocation(mapped(root,0u64),16u64,3u64,0u64,mapped(root,3u64),0u64));
        relocation_put(output,3u64,NativeImportRelocation(mapped(root,2u64),0u64,3u64,0u64,mapped(root,4u64),0u64));
        relocation_put(output,4u64,NativeImportRelocation(mapped(root,3u64),0u64,3u64,0u64,mapped(root,4u64),0u64));
    }
    return plan;
}
extern "C" void encode_import(NativeImportInput input,NativeImportOutput output,NativeImportPlan plan) on cpu {
    import_name(input.dll,input.name,output.slot);
    // "@blink.imports@", followed by the one existing slot name.
    literal(output.stem,0u64,7579113344870408768u64,8u64);literal(output.stem,8u64,18141342482788461u64,7u64);
    copy(output.stem,15u64,BindingBytes(output.slot,plan.slot));copy(output.member,0u64,BindingBytes(output.stem,plan.stem));
    literal(output.member,plan.stem,1784835886u64,4u64); // ".obj"
    u64 i=0u64;while(i<6u64){NativeImportSection section=section_at(output,i);u64 j=0u64;while(j<section.length){put(section.bytes,j,u8(0));j+=1u64;}i+=1u64;}
    if(plan.root==3u64){
        address names=section_at(output,mapped(plan.root,4u64)).bytes;
        copy(names,0u64,input.dll);copy(names,plan.hint+2u64,input.name);
        address lookup=section_at(output,mapped(plan.root,2u64)).bytes;address iat=section_at(output,mapped(plan.root,3u64)).bytes;
        literal(lookup,0u64,plan.hint,8u64);literal(iat,0u64,plan.hint,8u64);
    }
}
