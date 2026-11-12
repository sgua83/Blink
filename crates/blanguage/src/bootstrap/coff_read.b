module bootstrap.coff_read;

ReadBytes empty() on cpu {return ReadBytes(unsafe_address(0u64),0u64);}
address at(address bytes,u64 offset) on cpu {return unsafe_address(address_bits(bytes)+offset);}
u64 little(address bytes,u64 offset,u64 length) on cpu {
    u64 value=0u64;u64 i=0u64;
    while(i<length){value=value|(u64(unsafe_position<u8>(at(bytes,offset+i)))<<(i*8u64));i+=1u64;}
    return value;
}
ReadRange range(ReadBytes input,u64 offset,u64 length) on cpu {
    if(offset>input.length || length>input.length-offset){return ReadRange(empty(),2);}
    return ReadRange(ReadBytes(at(input.bytes,offset),length),0);
}
ReadBytes inline_name(address bytes) on cpu {
    u64 length=0u64;while(length<8u64){if(unsafe_position<u8>(at(bytes,length))==u8(0)){break;}length+=1u64;}
    return ReadBytes(bytes,length);
}
extern "C" ReadRange headers(ReadBytes input) on cpu {
    ReadRange machine=range(input,0u64,2u64);if(machine.error!=0){return machine;}
    if(little(machine.bytes.bytes,0u64,2u64)!=34404u64){return ReadRange(empty(),1);}
    ReadRange optional=range(input,16u64,2u64);if(optional.error!=0){return optional;}
    if(little(optional.bytes.bytes,0u64,2u64)!=0u64){return ReadRange(empty(),1);}
    return range(input,20u64,little(input.bytes,2u64,2u64)*40u64);
}
extern "C" ReadBuild build_inputs(ReadBytes input,ReadBytes headers) on cpu {
    u64 i=0u64;
    while(i<headers.length){
        address header=at(headers.bytes,i);
        if(little(header,0u64,8u64)==7236274654160904750u64){
            ReadRange bytes=range(input,little(header,20u64,4u64),little(header,16u64,4u64));
            return ReadBuild(bytes.bytes,1u64,bytes.error);
        }
        i+=40u64;
    }
    return ReadBuild(empty(),0u64,0);
}
extern "C" ReadSymbols symbols(ReadBytes input) on cpu {
    ReadRange fields=range(input,8u64,8u64);if(fields.error!=0){return ReadSymbols(empty(),0u64,0u64,fields.error);}
    u64 offset=little(fields.bytes.bytes,0u64,4u64);u64 count=little(fields.bytes.bytes,4u64,4u64);
    u64 strings=offset+count*18u64;
    ReadRange size=range(input,strings,4u64);if(size.error!=0){return ReadSymbols(empty(),0u64,0u64,size.error);}
    ReadRange bytes=range(input,strings,little(size.bytes.bytes,0u64,4u64));
    return ReadSymbols(bytes.bytes,offset,count,bytes.error);
}
ReadSymbol bad_symbol(u32 error) on cpu {
    return ReadSymbol(empty(),i64(0),0u64,0u64,0u64,0u64,0u64,0u64,0u64,error);
}
extern "C" ReadSymbol symbol(ReadBytes input,ReadSymbols table,u64 index) on cpu {
    ReadRange record=range(input,table.offset+index*18u64,18u64);if(record.error!=0){return bad_symbol(record.error);}
    address bytes=record.bytes.bytes;ReadBytes name=empty();
    if(little(bytes,0u64,4u64)==0u64){
        u64 begin=little(bytes,4u64,4u64);
        if(begin>table.strings.length){return bad_symbol(4);}
        u64 length=0u64;
        while(begin+length<table.strings.length){
            if(unsafe_position<u8>(at(table.strings.bytes,begin+length))==u8(0)){break;}
            length+=1u64;
        }
        if(begin+length==table.strings.length){return bad_symbol(5);}
        name=ReadBytes(at(table.strings.bytes,begin),length);
    }else{name=inline_name(bytes);}
    u64 raw=little(bytes,12u64,2u64);i64 section=i64(raw);
    if(raw>=32768u64){section-=i64(65536);}
    u64 storage=little(bytes,16u64,1u64);u64 auxiliary=little(bytes,17u64,1u64);
    u64 next=index+1u64+auxiliary;
    if(next>table.count){return bad_symbol(7);}
    u64 selection=0u64;u64 owner=0u64;
    if(storage==3u64 && section>i64(0) && auxiliary>0u64){
        ReadRange extra=range(input,table.offset+(index+1u64)*18u64,18u64);
        if(extra.error!=0){return bad_symbol(extra.error);}
        selection=little(extra.bytes.bytes,14u64,1u64);owner=little(extra.bytes.bytes,12u64,2u64);
    }
    u64 function=0u64;if(little(bytes,14u64,2u64)==32u64){function=1u64;}
    return ReadSymbol(name,section,little(bytes,8u64,4u64),storage,function,auxiliary,selection,owner,next,0);
}
extern "C" ReadSection section(ReadBytes input,ReadBytes headers,u64 index) on cpu {
    address bytes=at(headers.bytes,index*40u64);
    ReadRange body=range(input,little(bytes,20u64,4u64),little(bytes,16u64,4u64));
    return ReadSection(inline_name(bytes),body.bytes,little(bytes,36u64,4u64),little(bytes,24u64,4u64),little(bytes,32u64,2u64),body.error);
}
extern "C" ReadRelocations relocations(ReadBytes input,u64 offset,u64 count,u64 flags) on cpu {
    if((flags&16777216u64)!=0u64){
        ReadRange row=range(input,offset,4u64);if(row.error!=0){return ReadRelocations(0u64,0u64,row.error);}
        u64 value=little(row.bytes.bytes,0u64,4u64);
        if(value==0u64){return ReadRelocations(0u64,0u64,6);}
        return ReadRelocations(offset+10u64,value-1u64,0);
    }
    return ReadRelocations(offset,count,0);
}
extern "C" ReadRelocation relocation(ReadBytes input,u64 offset,u64 index) on cpu {
    ReadRange row=range(input,offset+index*10u64,10u64);
    if(row.error!=0){return ReadRelocation(0u64,0u64,0u64,row.error);}
    return ReadRelocation(little(row.bytes.bytes,0u64,4u64),little(row.bytes.bytes,4u64,4u64),little(row.bytes.bytes,8u64,2u64),0);
}
