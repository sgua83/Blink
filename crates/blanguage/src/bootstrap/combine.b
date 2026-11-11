module bootstrap.combine;

CombineObject object_at(CombineInput input,u64 i) on cpu {return unsafe_position<CombineObject>(unsafe_address(address_bits(input.objects)+i*OBJECT_BYTES));}
CombineSection section_at(CombineInput input,u64 i) on cpu {return unsafe_position<CombineSection>(unsafe_address(address_bits(input.sections)+i*SECTION_BYTES));}
void section_put(CombineInput input,u64 i,CombineSection value) on cpu {unsafe_position<CombineSection>(unsafe_address(address_bits(input.sections)+i*SECTION_BYTES))=value;}
CombineSymbol symbol_at(CombineInput input,u64 i) on cpu {return unsafe_position<CombineSymbol>(unsafe_address(address_bits(input.symbols)+i*SYMBOL_BYTES));}
void symbol_put(CombineInput input,u64 i,CombineSymbol value) on cpu {unsafe_position<CombineSymbol>(unsafe_address(address_bits(input.symbols)+i*SYMBOL_BYTES))=value;}
CombineRelocation relocation_at(CombineInput input,u64 i) on cpu {return unsafe_position<CombineRelocation>(unsafe_address(address_bits(input.relocations)+i*RELOCATION_BYTES));}
void relocation_put(CombineInput input,u64 i,CombineRelocation value) on cpu {unsafe_position<CombineRelocation>(unsafe_address(address_bits(input.relocations)+i*RELOCATION_BYTES))=value;}
CombineFrame frame_at(CombineInput input,u64 i) on cpu {return unsafe_position<CombineFrame>(unsafe_address(address_bits(input.frames)+i*FRAME_BYTES));}
void frame_put(CombineInput input,u64 i,CombineFrame value) on cpu {unsafe_position<CombineFrame>(unsafe_address(address_bits(input.frames)+i*FRAME_BYTES))=value;}
u64 order_at(CombineInput input,u64 i) on cpu {return unsafe_position<u64>(unsafe_address(address_bits(input.order)+i*8u64));}
void order_put(CombineInput input,u64 i,u64 value) on cpu {unsafe_position<u64>(unsafe_address(address_bits(input.order)+i*8u64))=value;}
u8 byte_at(address data,u64 i) on cpu {return unsafe_position<u8>(unsafe_address(address_bits(data)+i));}
void byte_put(address data,u64 i,u8 value) on cpu {unsafe_position<u8>(unsafe_address(address_bits(data)+i))=value;}
i32 compare(address a,u64 na,address b,u64 nb) on cpu {
    u64 i=0u64;while(i<na&&i<nb){u8 x=byte_at(a,i);u8 y=byte_at(b,i);if(x<y){return -1;}if(x>y){return i32(1);}i+=1u64;}
    if(na<nb){return -1;}if(na>nb){return i32(1);}return i32(0);
}
bool less(CombineInput input,u64 a,u64 b,bool symbols) on cpu {
    if(symbols){
        CombineSymbol x=symbol_at(input,a);CombineSymbol y=symbol_at(input,b);i32 name=compare(x.name,x.name_length,y.name,y.name_length);
        if(name!=0){return name<0;}
    }else{
        CombineSection x=section_at(input,a);CombineSection y=section_at(input,b);i32 name=compare(x.name,x.name_length,y.name,y.name_length);
        if(name!=0){return name<0;}if(x.flags!=y.flags){return x.flags<y.flags;}
    }
    return a<b;
}
void sift(CombineInput input,u64 start,u64 end,bool symbols) on cpu {
    u64 root=start;while(root<end/2u64){
        u64 child=root*2u64+1u64;
        if(child+1u64<end&&less(input,order_at(input,child),order_at(input,child+1u64),symbols)){child+=1u64;}
        u64 a=order_at(input,root);u64 b=order_at(input,child);if(!less(input,a,b,symbols)){return;}
        order_put(input,root,b);order_put(input,child,a);root=child;
    }
}
void sort(CombineInput input,u64 count,bool symbols) on cpu {
    u64 i=0u64;while(i<count){order_put(input,i,i);i+=1u64;}
    u64 root=count/2u64;while(root>0u64){root-=1u64;sift(input,root,count,symbols);}
    u64 end=count;while(end>1u64){end-=1u64;u64 a=order_at(input,0u64);u64 b=order_at(input,end);order_put(input,0u64,b);order_put(input,end,a);sift(input,0u64,end,symbols);}
}
bool same_section(CombineSection a,CombineSection b) on cpu {return a.flags==b.flags&&compare(a.name,a.name_length,b.name,b.name_length)==0;}
bool same_symbol(CombineSymbol a,CombineSymbol b) on cpu {return compare(a.name,a.name_length,b.name,b.name_length)==0;}
CombinePlan invalid(u64 error,u64 at) on cpu {return CombinePlan(0u64,0u64,0u64,error,at);}
extern "C" CombinePlan prepare(CombineInput input) on cpu {
    CombinePlan result=invalid(0u64,0u64);u64 i=0u64;
    while(i<input.section_count){CombineSection row=section_at(input,i);row.flags=row.flags&4278185983u64;row.size=0u64;section_put(input,i,row);i+=1u64;}
    sort(input,input.section_count,false);i=0u64;u64 owner=0u64;
    while(i<input.section_count){
        u64 index=order_at(input,i);CombineSection row=section_at(input,index);
        if(i==0u64||!same_section(section_at(input,order_at(input,i-1u64)),row)){owner=index;}
        row.owner=owner;section_put(input,index,row);i+=1u64;
    }
    u64 o=0u64;
    while(o<input.object_count){
        CombineObject object=object_at(input,o);if(object.stack_bytes>result.stack_bytes){result.stack_bytes=object.stack_bytes;}
        i=object.section_start;
        while(i<object.section_start+object.section_count){
            CombineSection row=section_at(input,i);CombineSection first=section_at(input,row.owner);
            u64 group=first.group;if(row.owner==i){group=result.groups;result.groups+=1u64;}
            u64 exponent=(row.flags>>20u64)&15u64;if(exponent>0u64){exponent-=1u64;}u64 alignment=1u64<<exponent;
            u64 begin=(first.size+alignment-1u64)&(0u64-alignment);if(begin>4294967295u64){return invalid(1u64,i);}
            row.group=group;row.begin=begin;row.padding=begin-first.size;
            if(row.owner==i){row.size=begin+row.byte_length;}else{first.size=begin+row.byte_length;section_put(input,row.owner,first);}
            section_put(input,i,row);i+=1u64;
        }
        i=object.symbol_start;
        while(i<object.symbol_start+object.symbol_count){
            CombineSymbol row=symbol_at(input,i);CombineSection section=section_at(input,object.section_start+row.section);
            row.section=section.group;row.value+=section.begin;if(row.value>4294967295u64){return invalid(2u64,i);}symbol_put(input,i,row);i+=1u64;
        }
        i=object.frame_start;
        while(i<object.frame_start+object.frame_count){
            CombineFrame row=frame_at(input,i);
            if(row.present!=0u64){
                CombineSection section=section_at(input,object.section_start+row.section);row.section=section.group;row.start+=section.begin;
                if(row.start>4294967295u64){return invalid(3u64,i);}
                u64 j=row.call_start;while(j<row.call_start+row.call_count){address call=unsafe_address(address_bits(input.calls)+j*8u64);unsafe_position<u64>(call)=unsafe_position<u64>(call)+section.begin;j+=1u64;}
                frame_put(input,i,row);
            }
            i+=1u64;
        }
        o+=1u64;
    }
    sort(input,input.symbol_count,true);i=0u64;
    while(i<input.symbol_count){
        u64 end=i+1u64;CombineSymbol first=symbol_at(input,order_at(input,i));
        while(end<input.symbol_count&&same_symbol(first,symbol_at(input,order_at(input,end)))){end+=1u64;}
        order_put(input,result.symbols,order_at(input,end-1u64));result.symbols+=1u64;i=end;
    }
    i=0u64;
    while(i<input.relocation_count){
        CombineRelocation row=relocation_at(input,i);CombineSection source=section_at(input,row.source);row.offset+=source.begin;
        if(row.target_kind==0u64){
            CombineObject object=object_at(input,source.object);CombineSection target=section_at(input,object.section_start+row.section);
            row.section=target.group;row.value+=target.begin;if(row.value>4294967295u64){return invalid(4u64,i);}
        }else if(row.target_kind==2u64){
            u64 lo=0u64;u64 hi=result.symbols;
            while(lo<hi){u64 mid=lo+(hi-lo)/2u64;CombineSymbol symbol=symbol_at(input,order_at(input,mid));if(compare(symbol.name,symbol.name_length,row.name,row.name_length)<0){lo=mid+1u64;}else{hi=mid;}}
            if(lo==result.symbols){return invalid(5u64,i);}CombineSymbol symbol=symbol_at(input,order_at(input,lo));
            if(compare(symbol.name,symbol.name_length,row.name,row.name_length)!=0){return invalid(5u64,i);}
            row.target_kind=0u64;row.section=symbol.section;row.value=symbol.value;
        }
        relocation_put(input,i,row);i+=1u64;
    }
    return result;
}
bool is_text(CombineSection row) on cpu {return row.name_length==5u64&&byte_at(row.name,0u64)==u8(46)&&byte_at(row.name,1u64)==u8(116)&&byte_at(row.name,2u64)==u8(101)&&byte_at(row.name,3u64)==u8(120)&&byte_at(row.name,4u64)==u8(116);}
extern "C" void encode(CombineInput input,address outputs) on cpu {
    u64 i=0u64;while(i<input.section_count){
        CombineSection row=section_at(input,i);address output=unsafe_position<address>(unsafe_address(address_bits(outputs)+row.group*8u64));
        u8 padding=u8(0);if(is_text(row)){padding=u8(144);}u64 j=row.begin-row.padding;
        while(j<row.begin){byte_put(output,j,padding);j+=1u64;}
        j=0u64;while(j<row.byte_length){byte_put(output,row.begin+j,byte_at(row.bytes,j));j+=1u64;}i+=1u64;
    }
}
