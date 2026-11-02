module bootstrap.coff;

CoffSection section_at(CoffInput input,u64 index) on cpu {
    return unsafe_position<CoffSection>(unsafe_address(address_bits(input.sections)+index*SECTION_BYTES));
}
void section_put(CoffInput input,u64 index,CoffSection value) on cpu {
    unsafe_position<CoffSection>(unsafe_address(address_bits(input.sections)+index*SECTION_BYTES))=value;
}
CoffSymbol symbol_at(CoffInput input,u64 index) on cpu {
    return unsafe_position<CoffSymbol>(unsafe_address(address_bits(input.symbols)+index*SYMBOL_BYTES));
}
CoffRelocation relocation_at(CoffSection section,u64 index) on cpu {
    return unsafe_position<CoffRelocation>(unsafe_address(address_bits(section.relocations)+index*RELOCATION_BYTES));
}
CoffLayout invalid(u32 code) on cpu {return CoffLayout(0u64,0u64,0u64,0u64,code);}

extern "C" CoffLayout layout(CoffInput input) on cpu {
    if(input.section_count>65535u64){return invalid(1);}
    u64 at=20u64+40u64*input.section_count;
    u64 i=0u64;
    while(i<input.section_count){
        CoffSection section=section_at(input,i);
        if(section.byte_length>4294967295u64){return invalid(2);}
        if(at>4294967295u64){return invalid(3);}
        section.offset=at;at+=section.byte_length;
        if(at>4294967295u64){return invalid(3);}
        section.relocation_offset=at;
        if(section.relocation_count>=4294967295u64){return invalid(4);}
        if(section.relocation_count>=65535u64){at+=10u64;}
        at+=10u64*section.relocation_count;
        section_put(input,i,section);i+=1u64;
    }
    if(at>4294967295u64){return invalid(3);}
    u64 count=0u64;u64 strings=4u64;i=0u64;
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
extern "C" void encode(CoffInput input,CoffLayout layout,address output) on cpu {
    zero(output,0u64,20u64+40u64*input.section_count);
    le16(output,0u64,34404u64);le16(output,2u64,input.section_count);
    le32(output,8u64,layout.symbols);le32(output,12u64,layout.symbol_count);
    u64 i=0u64;
    while(i<input.section_count){
        CoffSection section=section_at(input,i);
        u64 h=20u64+40u64*i;
        copy(output,h,section.name,section.name_length);
        le32(output,h+16u64,section.byte_length);le32(output,h+20u64,section.offset);
        copy(output,section.offset,section.bytes,section.byte_length);
        le32(output,h+24u64,section.relocation_offset);
        u64 count=section.relocation_count;u64 flags=section.flags;
        u64 at=section.relocation_offset;
        if(count>=65535u64){
            count=65535u64;flags=flags|16777216u64;
            le32(output,at,section.relocation_count+1u64);zero(output,at+4u64,6u64);at+=10u64;
        }
        le16(output,h+32u64,count);le32(output,h+36u64,flags);
        u64 j=0u64;
        while(j<section.relocation_count){
            CoffRelocation relocation=relocation_at(section,j);
            le32(output,at,relocation.offset);le32(output,at+4u64,relocation.symbol);
            le16(output,at+8u64,relocation.kind);at+=10u64;j+=1u64;
        }
        i+=1u64;
    }
    u64 at=layout.symbols;u64 strings=4u64;i=0u64;
    le32(output,layout.strings,layout.length-layout.strings);
    while(i<input.symbol_count){
        CoffSymbol symbol=symbol_at(input,i);
        zero(output,at,18u64);
        if(symbol.name_length<=8u64){copy(output,at,symbol.name,symbol.name_length);}
        else{
            le32(output,at+4u64,strings);
            copy(output,layout.strings+strings,symbol.name,symbol.name_length);
            strings+=symbol.name_length;put(output,layout.strings+strings,0);strings+=1u64;
        }
        le32(output,at+8u64,symbol.value);le16(output,at+12u64,symbol.section);
        if(symbol.function!=0u64){le16(output,at+14u64,32u64);}
        u32 storage=3;if(symbol.external!=0u64){storage=2;}put(output,at+16u64,storage);
        put(output,at+17u64,u32(symbol.auxiliary));at+=18u64;
        if(symbol.auxiliary!=0u64){
            CoffSection section=section_at(input,symbol.section-1u64);
            zero(output,at,18u64);le32(output,at,section.byte_length);
            u64 count=section.relocation_count;if(count>65535u64){count=65535u64;}
            le16(output,at+4u64,count);le16(output,at+12u64,symbol.owner);
            u32 selection=1;if(symbol.owner!=0u64){selection=5;}put(output,at+14u64,selection);
            at+=18u64;
        }
        i+=1u64;
    }
}
