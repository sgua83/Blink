module bootstrap.pe;

u64 align(u64 n,u64 by) on cpu requires nonzero(by) {return ((n+by-1u64)/by)*by;}
PeSection section_at(PeInput input,u64 i) on cpu {return unsafe_position<PeSection>(unsafe_address(address_bits(input.sections)+i*SECTION_BYTES));}
void section_put(PeInput input,u64 i,PeSection value) on cpu {unsafe_position<PeSection>(unsafe_address(address_bits(input.sections)+i*SECTION_BYTES))=value;}
PeRelocation relocation_at(PeInput input,u64 i) on cpu {return unsafe_position<PeRelocation>(unsafe_address(address_bits(input.relocations)+i*RELOCATION_BYTES));}
void relocation_put(PeInput input,u64 i,PeRelocation value) on cpu {unsafe_position<PeRelocation>(unsafe_address(address_bits(input.relocations)+i*RELOCATION_BYTES))=value;}
PeImport import_at(PeInput input,u64 i) on cpu {return unsafe_position<PeImport>(unsafe_address(address_bits(input.imports)+i*IMPORT_BYTES));}
void import_put(PeInput input,u64 i,PeImport value) on cpu {unsafe_position<PeImport>(unsafe_address(address_bits(input.imports)+i*IMPORT_BYTES))=value;}
PeAbsolute absolute_at(PeInput input,u64 i) on cpu {return unsafe_position<PeAbsolute>(unsafe_address(address_bits(input.absolutes)+i*ABSOLUTE_BYTES));}
void absolute_put(PeInput input,u64 i,PeAbsolute value) on cpu {unsafe_position<PeAbsolute>(unsafe_address(address_bits(input.absolutes)+i*ABSOLUTE_BYTES))=value;}
u8 byte_at(address data,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(data)+i));}
void put(address output,u64 at,u64 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(output)+at))=u8(value);}
void le16(address output,u64 at,u64 value) on cpu {put(output,at,value);put(output,at+1u64,value>>8u64);}
void le32(address output,u64 at,u64 value) on cpu {u64 i=0u64;while(i<4u64){put(output,at+i,value>>(i*8u64));i+=1u64;}}
void le64(address output,u64 at,u64 value) on cpu {u64 i=0u64;while(i<8u64){put(output,at+i,value>>(i*8u64));i+=1u64;}}
void copy(address output,u64 at,address data,u64 length) on cpu {u64 i=0u64;while(i<length){put(output,at+i,u64(byte_at(data,i)));i+=1u64;}}

i32 compare(address a,u64 na,address b,u64 nb) on cpu {
    u64 i=0u64;while(i<na&&i<nb){u8 x=byte_at(a,i);u8 y=byte_at(b,i);if(x<y){return -1;}if(x>y){return i32(1);}i+=1u64;}
    if(na<nb){return -1;}if(na>nb){return i32(1);}return i32(0);
}
bool same_dll(PeImport a,PeImport b) on cpu {return compare(a.dll,a.dll_length,b.dll,b.dll_length)==0;}
bool import_less(PeImport a,PeImport b) on cpu {
    i32 dll=compare(a.dll,a.dll_length,b.dll,b.dll_length);if(dll!=0){return dll<0;}
    i32 name=compare(a.name,a.name_length,b.name,b.name_length);if(name!=0){return name<0;}
    return a.relocation<b.relocation;
}
void sift_import(PeInput input,u64 start,u64 end) on cpu {
    u64 root=start;
    while(root<end/2u64){
        u64 child=root*2u64+1u64;
        if(child+1u64<end&&import_less(import_at(input,child),import_at(input,child+1u64))){child+=1u64;}
        PeImport a=import_at(input,root);PeImport b=import_at(input,child);
        if(!import_less(a,b)){return;}
        import_put(input,root,b);import_put(input,child,a);root=child;
    }
}
void sort_imports(PeInput input) on cpu {
    u64 root=input.import_count/2u64;while(root>0u64){root-=1u64;sift_import(input,root,input.import_count);}
    u64 end=input.import_count;while(end>1u64){end-=1u64;PeImport a=import_at(input,0u64);PeImport b=import_at(input,end);import_put(input,0u64,b);import_put(input,end,a);sift_import(input,0u64,end);}
}
bool absolute_less(PeAbsolute a,PeAbsolute b) on cpu {return a.page<b.page||(a.page==b.page&&a.order<b.order);}
void sift_absolute(PeInput input,u64 start,u64 end) on cpu {
    u64 root=start;
    while(root<end/2u64){
        u64 child=root*2u64+1u64;
        if(child+1u64<end&&absolute_less(absolute_at(input,child),absolute_at(input,child+1u64))){child+=1u64;}
        PeAbsolute a=absolute_at(input,root);PeAbsolute b=absolute_at(input,child);
        if(!absolute_less(a,b)){return;}
        absolute_put(input,root,b);absolute_put(input,child,a);root=child;
    }
}
void sort_absolutes(PeInput input) on cpu {
    u64 root=input.absolute_count/2u64;while(root>0u64){root-=1u64;sift_absolute(input,root,input.absolute_count);}
    u64 end=input.absolute_count;while(end>1u64){end-=1u64;PeAbsolute a=absolute_at(input,0u64);PeAbsolute b=absolute_at(input,end);absolute_put(input,0u64,b);absolute_put(input,end,a);sift_absolute(input,0u64,end);}
}
// Assign a directory and a relative thunk offset in the sorted rows. Each
// completed DLL table contributes its original null terminator to both arrays.
void plan_imports(PeInput input, inout PeLayout result) on cpu {
    sort_imports(input);
    u64 groups = 0u64;
    u64 slots = 0u64;
    u64 slot = 0u64;
    u64 index = 0u64;
    while (index < input.import_count) {
        PeImport value = import_at(input, index);
        bool first_dll = index == 0u64;
        bool first_name = first_dll;
        if (index != 0u64) {
            PeImport previous = import_at(input, index - 1u64);
            first_dll = !same_dll(previous, value);
            first_name = first_dll || compare(previous.name, previous.name_length, value.name, value.name_length) != 0;
            if (first_dll) { slots += 1u64; }
        }
        if (first_dll) { groups += 1u64; }
        if (first_name) {
            slot = slots;
            slots += 1u64;
        }
        value.directory = (groups - 1u64) * 20u64;
        value.lookup = slot * 8u64;
        import_put(input, index, value);
        index += 1u64;
    }
    if (groups != 0u64) { slots += 1u64; }
    result.import_directory_size = (groups + 1u64) * 20u64;
    u64 lookup = align(result.import_directory_size, 8u64);
    u64 iat = lookup + slots * 8u64;
    result.iat_start = result.idata + iat;
    result.iat_size = slots * 8u64;
    u64 at = iat + result.iat_size;

    u64 directory = 0u64;
    u64 thunk = 0u64;
    u64 dll_at = 0u64;
    u64 hint = 0u64;
    index = 0u64;
    while (index < input.import_count) {
        PeImport value = import_at(input, index);
        if (index == 0u64 || value.directory != directory) {
            directory = value.directory;
            dll_at = at;
            at += value.dll_length + 1u64;
        }
        if (index == 0u64 || value.lookup != thunk) {
            thunk = value.lookup;
            at = align(at, 2u64);
            hint = at;
            at += value.name_length + 3u64;
        }
        value.lookup = lookup + thunk;
        value.iat = iat + thunk;
        value.dll_at = dll_at;
        value.hint = hint;
        import_put(input, index, value);
        PeRelocation relocation = relocation_at(input, value.relocation);
        relocation.target = result.idata + value.iat;
        relocation_put(input, value.relocation, relocation);
        index += 1u64;
    }
    result.idata_length = at;
}
PeLayout invalid(u64 code) on cpu {
    return PeLayout(0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,5368709120u64,code);
}
extern "C" u64 stack_reserve(u32 stack_bytes) on cpu {return align(u64(stack_bytes)+1048576u64,65536u64);}
extern "C" PeLayout layout(PeInput input) on cpu {
    PeLayout result=invalid(0u64);u64 rva=4096u64;u64 i=0u64;bool pdata=false;
    while(i<input.section_count){
        PeSection section=section_at(input,i);
        if(section.byte_length>4294967295u64){return invalid(1u64);}
        section.rva=u32(rva);section_put(input,i,section);
        if(section.name_length==6u64&&byte_at(section.name,0u64)==u8(46)&&byte_at(section.name,1u64)==u8(112)&&byte_at(section.name,2u64)==u8(100)&&byte_at(section.name,3u64)==u8(97)&&byte_at(section.name,4u64)==u8(116)&&byte_at(section.name,5u64)==u8(97)&&!pdata){
            pdata=true;result.pdata=rva;result.pdata_length=section.byte_length;
        }
        rva+=align(section.byte_length,4096u64);if(rva>4294967295u64){return invalid(1u64);}i+=1u64;
    }
    if(!pdata){return invalid(4u64);}
    result.idata = rva;
    plan_imports(input, result);
    u64 at = result.idata_length;
    if (result.idata + at > 4294967295u64) { return invalid(1u64); }
    result.reloc = result.idata + align(at, 4096u64);
    u64 absolute=0u64;i=0u64;
    while(i<input.section_count){
        PeSection section=section_at(input,i);u64 j=0u64;
        while(j<section.relocation_count){
            u64 index=section.relocation_start+j;PeRelocation relocation=relocation_at(input,index);relocation.source=u64(section.rva)+relocation.offset;
            if(relocation.target_kind==0){
                if(relocation.section>=input.section_count){return invalid(2u64);}
                relocation.target=u64(section_at(input,relocation.section).rva)+relocation.value;
            }
            relocation_put(input,index,relocation);
            if(relocation.kind==1){
                u64 location=u64(u32(relocation.source));u64 page=(location/4096u64)*4096u64;
                absolute_put(input,absolute,PeAbsolute(page,40960u64+(location-page),index));absolute+=1u64;
            }
            j+=1u64;
        }
        i+=1u64;
    }
    sort_absolutes(input);at=0u64;i=0u64;
    while(i<input.absolute_count){
        PeAbsolute first=absolute_at(input,i);u64 end=i+1u64;
        while(end<input.absolute_count&&absolute_at(input,end).page==first.page){end+=1u64;}
        at+=8u64+align((end-i)*2u64,4u64);i=end;
    }
    result.reloc_length=at;result.section_count=input.section_count+1u64;
    if(at!=0u64){result.section_count+=1u64;}
    if(result.section_count>65535u64){return invalid(1u64);}
    result.header=align(392u64+result.section_count*40u64,512u64);at=result.header;i=0u64;
    while(i<input.section_count){
        PeSection section=section_at(input,i);section.raw=at;section_put(input,i,section);
        at+=align(section.byte_length,512u64);i+=1u64;
    }
    at+=align(result.idata_length,512u64);at+=align(result.reloc_length,512u64);result.length=at;
    if(result.reloc_length!=0u64){result.image_size=result.reloc+align(result.reloc_length,4096u64);}else{result.image_size=result.idata+align(result.idata_length,4096u64);}
    if(result.length>4294967295u64||result.image_size>4294967295u64){return invalid(1u64);}return result;
}
void section_header(address output,u64 index,address name,u64 name_length,u64 rva,u64 length,u64 raw,u64 flags) on cpu {
    u64 header=392u64+index*40u64;u64 count=name_length;if(count>8u64){count=8u64;}
    copy(output,header,name,count);le32(output,header+8u64,length);le32(output,header+12u64,rva);
    le32(output,header+16u64,align(length,512u64));le32(output,header+20u64,raw);le32(output,header+36u64,flags);
}
extern "C" void encode(PeInput input,PeLayout layout,address output) on cpu {
    put(output,0u64,77u64);put(output,1u64,90u64);le32(output,60u64,128u64);
    put(output,128u64,80u64);put(output,129u64,69u64);le16(output,132u64,34404u64);
    le16(output,134u64,layout.section_count);le16(output,148u64,240u64);le16(output,150u64,34u64);
    u64 optional=152u64;le16(output,optional,523u64);
    le32(output,optional+4u64,align(section_at(input,0u64).byte_length,512u64));
    u64 initialized=0u64;u64 i=1u64;
    while(i<input.section_count){initialized+=align(section_at(input,i).byte_length,512u64);i+=1u64;}
    initialized+=align(layout.idata_length,512u64);initialized+=align(layout.reloc_length,512u64);
    le32(output,optional+8u64,initialized);le32(output,optional+16u64,4096u64);le32(output,optional+20u64,4096u64);
    le64(output,optional+24u64,layout.image_base);le32(output,optional+32u64,4096u64);le32(output,optional+36u64,512u64);
    le16(output,optional+40u64,6u64);le16(output,optional+48u64,6u64);le32(output,optional+56u64,layout.image_size);le32(output,optional+60u64,layout.header);le16(output,optional+68u64,3u64);
    u64 flags=256u64;if(layout.reloc_length!=0u64){flags+=96u64;}le16(output,optional+70u64,flags);
    le64(output,optional+72u64,stack_reserve(u32(input.stack_bytes)));le64(output,optional+80u64,4096u64);le64(output,optional+88u64,1048576u64);le64(output,optional+96u64,4096u64);le32(output,optional+108u64,16u64);
    le32(output,optional+120u64,layout.idata);le32(output,optional+124u64,layout.import_directory_size);
    le32(output,optional+136u64,layout.pdata);le32(output,optional+140u64,layout.pdata_length);
    if(layout.reloc_length!=0u64){le32(output,optional+152u64,layout.reloc);}le32(output,optional+156u64,layout.reloc_length);
    le32(output,optional+208u64,layout.iat_start);le32(output,optional+212u64,layout.iat_size);
    u64 raw=layout.header;i=0u64;
    while(i<input.section_count){
        PeSection section=section_at(input,i);section_header(output,i,section.name,section.name_length,u64(section.rva),section.byte_length,section.raw,u64(section.flags)&4262461439u64);
        copy(output,section.raw,section.bytes,section.byte_length);raw=section.raw+align(section.byte_length,512u64);i+=1u64;
    }
    // Generated section names occupy their actual fixed PE name fields.
    u64 ih=392u64+input.section_count*40u64;
    put(output,ih,46u64);put(output,ih+1u64,105u64);put(output,ih+2u64,100u64);put(output,ih+3u64,97u64);put(output,ih+4u64,116u64);put(output,ih+5u64,97u64);
    le32(output,ih+8u64,layout.idata_length);le32(output,ih+12u64,layout.idata);le32(output,ih+16u64,align(layout.idata_length,512u64));le32(output,ih+20u64,raw);le32(output,ih+36u64,3221225536u64);
    u64 idata=raw;i=0u64;
    while(i<input.import_count){
        PeImport value=import_at(input,i);bool first=i==0u64;
        if(i!=0u64){first=import_at(input,i-1u64).directory!=value.directory;}
        if(first){
            le32(output,idata+value.directory,layout.idata+value.lookup);
            le32(output,idata+value.directory+12u64,layout.idata+value.dll_at);le32(output,idata+value.directory+16u64,layout.idata+value.iat);
            copy(output,idata+value.dll_at,value.dll,value.dll_length);
        }
        bool unique=i==0u64;if(i!=0u64){unique=import_at(input,i-1u64).lookup!=value.lookup;}
        if(unique){
            le64(output,idata+value.lookup,layout.idata+value.hint);le64(output,idata+value.iat,layout.idata+value.hint);
            copy(output,idata+value.hint+2u64,value.name,value.name_length);
        }
        i+=1u64;
    }
    if(layout.reloc_length!=0u64){
        raw+=align(layout.idata_length,512u64);u64 rh=ih+40u64;
        put(output,rh,46u64);put(output,rh+1u64,114u64);put(output,rh+2u64,101u64);put(output,rh+3u64,108u64);put(output,rh+4u64,111u64);put(output,rh+5u64,99u64);
        le32(output,rh+8u64,layout.reloc_length);le32(output,rh+12u64,layout.reloc);le32(output,rh+16u64,align(layout.reloc_length,512u64));le32(output,rh+20u64,raw);le32(output,rh+36u64,1107296320u64);
        u64 cursor=raw;i=0u64;
        while(i<input.absolute_count){
            PeAbsolute first=absolute_at(input,i);u64 end=i+1u64;
            while(end<input.absolute_count&&absolute_at(input,end).page==first.page){end+=1u64;}
            u64 size=8u64+align((end-i)*2u64,4u64);le32(output,cursor,first.page);le32(output,cursor+4u64,size);
            u64 j=i;while(j<end){le16(output,cursor+8u64+(j-i)*2u64,absolute_at(input,j).value);j+=1u64;}
            cursor+=size;i=end;
        }
    }
}
