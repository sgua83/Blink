module bootstrap.archive;

void put(address bytes,u64 at,u32 value) on cpu {
    unsafe_position<u8>(unsafe_address(address_bits(bytes)+at))=u8(value);
}
ArchiveMember member_at(ArchiveInput input,u64 at) on cpu {
    return unsafe_position<ArchiveMember>(unsafe_address(address_bits(input.members)+at*MEMBER_BYTES));
}
void member_put(ArchiveInput input,u64 at,ArchiveMember value) on cpu {
    unsafe_position<ArchiveMember>(unsafe_address(address_bits(input.members)+at*MEMBER_BYTES))=value;
}
ArchiveSymbol symbol_at(ArchiveInput input,u64 at) on cpu {
    return unsafe_position<ArchiveSymbol>(unsafe_address(address_bits(input.symbols)+at*SYMBOL_BYTES));
}
void symbol_put(ArchiveInput input,u64 at,ArchiveSymbol value) on cpu {
    unsafe_position<ArchiveSymbol>(unsafe_address(address_bits(input.symbols)+at*SYMBOL_BYTES))=value;
}
u64 member_span(u64 bytes) on cpu {return 60u64+bytes+bytes%2u64;}
ArchiveLayout invalid(u32 code) on cpu {return ArchiveLayout(0u64,0u64,0u64,0u64,code);}

extern "C" ArchiveLayout layout(ArchiveInput input) on cpu {
    if(input.symbol_count>4294967295u64){return invalid(3);}
    if(input.member_count>4294967295u64){return invalid(4);}
    u64 strings=0u64;
    u64 i=0u64;
    while(i<input.symbol_count){
        ArchiveSymbol symbol=symbol_at(input,i);
        if(symbol.member>=65535u64){return invalid(5);}
        strings+=symbol.name_length+1u64;i+=1u64;
    }
    u64 names=0u64;i=0u64;
    while(i<input.member_count){
        ArchiveMember member=member_at(input,i);
        member.name_offset=names;member_put(input,i,member);
        names+=member.name_length+1u64;i+=1u64;
    }
    u64 first=4u64+4u64*input.symbol_count+strings;
    u64 second=8u64+4u64*input.member_count+2u64*input.symbol_count+strings;
    if(first>9999999999u64 || second>9999999999u64 || names>9999999999u64){return invalid(1);}
    u64 at=8u64+member_span(first)+member_span(second)+member_span(names);
    i=0u64;
    while(i<input.member_count){
        ArchiveMember member=member_at(input,i);
        if(at>4294967295u64){return invalid(2);}
        if(member.byte_length>9999999999u64){return invalid(1);}
        member.offset=at;member_put(input,i,member);
        at+=member_span(member.byte_length);i+=1u64;
    }
    return ArchiveLayout(at,first,second,names,0);
}
void copy(address out,u64 at,address bytes,u64 length) on cpu {
    u64 i=0u64;while(i<length){unsafe_position<u8>(unsafe_address(address_bits(out)+at+i))=unsafe_position<u8>(unsafe_address(address_bits(bytes)+i));i+=1u64;}
}
void le32(address out,u64 at,u64 value) on cpu {
    u32 i=0;while(i<4){put(out,at+u64(i),u32(value>>u64(i*8))&255);i+=1;}
}
void be32(address out,u64 at,u64 value) on cpu {
    u32 i=0;while(i<4){put(out,at+u64(i),u32(value>>u64((3-i)*8))&255);i+=1;}
}
u64 digits(u64 value) on cpu {
    u64 remaining=value;u64 count=1u64;while(remaining>=10u64){remaining/=10u64;count+=1u64;}return count;
}
void decimal(address out,u64 at,u64 value) on cpu {
    u64 remaining=value;u64 count=digits(value);
    while(count>0u64){count-=1u64;put(out,at+count,48+u32(remaining%10u64));remaining/=10u64;}
}
void header(address out,u64 at,u64 size,u32 kind,u64 name) on cpu {
    u64 i=0u64;while(i<60u64){put(out,at+i,32);i+=1u64;}
    put(out,at,47);
    if(kind==1){put(out,at+1u64,47);}
    else if(kind==2){decimal(out,at+1u64,name);}
    put(out,at+16u64,48);put(out,at+40u64,48);decimal(out,at+48u64,size);
    put(out,at+58u64,96);put(out,at+59u64,10);
    if(size%2u64!=0u64){put(out,at+60u64+size,10);}
}
bool later(ArchiveSymbol left,ArchiveSymbol right) on cpu {
    u64 count=left.name_length;if(right.name_length<count){count=right.name_length;}
    u64 i=0u64;
    while(i<count){
        u32 a=u32(unsafe_position<u8>(unsafe_address(address_bits(left.name)+i)));u32 b=u32(unsafe_position<u8>(unsafe_address(address_bits(right.name)+i)));
        if(a!=b){return a>b;}i+=1u64;
    }
    if(left.name_length!=right.name_length){return left.name_length>right.name_length;}
    return left.member>right.member;
}
void exchange(ArchiveInput input,u64 left,u64 right) on cpu {
    ArchiveSymbol a=symbol_at(input,left);ArchiveSymbol b=symbol_at(input,right);
    symbol_put(input,left,b);symbol_put(input,right,a);
}
void sift(ArchiveInput input,u64 root,u64 end) on cpu {
    u64 parent=root;
    u64 child=parent*2u64+1u64;
    while(child<end){
        if(child+1u64<end && later(symbol_at(input,child+1u64),symbol_at(input,child))){child+=1u64;}
        if(!later(symbol_at(input,child),symbol_at(input,parent))){return;}
        exchange(input,parent,child);parent=child;child=parent*2u64+1u64;
    }
}
void sort_symbols(ArchiveInput input) on cpu {
    u64 at=input.symbol_count/2u64;
    while(at>0u64){at-=1u64;sift(input,at,input.symbol_count);}
    u64 end=input.symbol_count;
    while(end>1u64){end-=1u64;exchange(input,0u64,end);sift(input,0u64,end);}
}
extern "C" void encode(ArchiveInput input,ArchiveLayout layout,address out) on cpu {
    put(out,0u64,33);put(out,1u64,60);put(out,2u64,97);put(out,3u64,114);
    put(out,4u64,99);put(out,5u64,104);put(out,6u64,62);put(out,7u64,10);
    u64 first=8u64;header(out,first,layout.first,0,0u64);
    u64 at=first+60u64;be32(out,at,input.symbol_count);at+=4u64;
    u64 i=0u64;
    while(i<input.symbol_count){
        ArchiveSymbol symbol=symbol_at(input,i);ArchiveMember member=member_at(input,symbol.member);
        be32(out,at,member.offset);at+=4u64;i+=1u64;
    }
    i=0u64;
    while(i<input.symbol_count){
        ArchiveSymbol symbol=symbol_at(input,i);copy(out,at,symbol.name,symbol.name_length);
        at+=symbol.name_length;put(out,at,0);at+=1u64;i+=1u64;
    }
    sort_symbols(input);
    u64 second=first+member_span(layout.first);header(out,second,layout.second,0,0u64);
    at=second+60u64;le32(out,at,input.member_count);at+=4u64;i=0u64;
    while(i<input.member_count){ArchiveMember member=member_at(input,i);le32(out,at,member.offset);at+=4u64;i+=1u64;}
    le32(out,at,input.symbol_count);at+=4u64;i=0u64;
    while(i<input.symbol_count){
        ArchiveSymbol symbol=symbol_at(input,i);u64 index=symbol.member+1u64;
        put(out,at,u32(index)&255);put(out,at+1u64,u32(index>>8u64)&255);at+=2u64;i+=1u64;
    }
    i=0u64;
    while(i<input.symbol_count){
        ArchiveSymbol symbol=symbol_at(input,i);copy(out,at,symbol.name,symbol.name_length);
        at+=symbol.name_length;put(out,at,0);at+=1u64;i+=1u64;
    }
    u64 names=second+member_span(layout.second);header(out,names,layout.names,1,0u64);
    at=names+60u64;i=0u64;
    while(i<input.member_count){
        ArchiveMember member=member_at(input,i);copy(out,at,member.name,member.name_length);
        at+=member.name_length;put(out,at,0);at+=1u64;
        header(out,member.offset,member.byte_length,2,member.name_offset);
        copy(out,member.offset+60u64,member.bytes,member.byte_length);i+=1u64;
    }
}
