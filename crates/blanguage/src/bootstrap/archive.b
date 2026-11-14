module bootstrap.archive;

const u64 ARCHIVE_SIGNATURE = 0x0a3e686372613c21u64;

u32 signature_byte(u64 index) on cpu {
    return u32((ARCHIVE_SIGNATURE >> (index * 8u64)) & 255u64);
}

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
i32 compare_names(ArchiveSymbol left,ArchiveSymbol right) on cpu {
    u64 count=left.name_length;if(right.name_length<count){count=right.name_length;}
    u64 i=0u64;
    while(i<count){
        u32 a=u32(unsafe_position<u8>(unsafe_address(address_bits(left.name)+i)));u32 b=u32(unsafe_position<u8>(unsafe_address(address_bits(right.name)+i)));
        if(a<b){return i32(-1);}if(a>b){return i32(1);}i+=1u64;
    }
    if(left.name_length<right.name_length){return i32(-1);}if(left.name_length>right.name_length){return i32(1);}
    return i32(0);
}
bool later(ArchiveSymbol left,ArchiveSymbol right) on cpu {
    i32 order=compare_names(left,right);return order>i32(0)||(order==i32(0)&&left.member>right.member);
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
    for (index in 0..8) {
        put(out, u64(index), signature_byte(u64(index)));
    }
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


struct ArchiveReadHeader {ArchiveRange name;ArchiveRange body;u64 next;u32 error;u64 at;};
struct ArchiveReadNumber {u64 value;bool valid;};

u32 read_byte(address bytes,u64 at) on cpu {
    return u32(unsafe_position<u8>(unsafe_address(address_bits(bytes)+at)));
}
extern "C" bool has_signature(address bytes, u64 length) on cpu {
    if (length < 8u64) { return false; }
    for (index in 0..8) {
        if (read_byte(bytes, u64(index)) != signature_byte(u64(index))) { return false; }
    }
    return true;
}

u32 read_word(address bytes,u64 at) on cpu {
    return read_byte(bytes,at)|read_byte(bytes,at+1u64)<<8
        |read_byte(bytes,at+2u64)<<16|read_byte(bytes,at+3u64)<<24;
}
// Match the whitespace consumed by the existing UTF-8 read_header text reader.
u32 header_space_width(address bytes,u64 at,u64 end) on cpu {
    u32 a=read_byte(bytes,at);
    if(a>=9&&a<=13||a==32){return 1;}
    if(end-at>=2u64&&a==194){
        u32 b=read_byte(bytes,at+1u64);
        if(b==133||b==160){return 2;}
    }
    if(end-at>=3u64){
        u32 b=read_byte(bytes,at+1u64);u32 c=read_byte(bytes,at+2u64);
        if(a==225&&b==154&&c==128
            ||a==226&&b==128&&(c>=128&&c<=138||c==168||c==169||c==175)
            ||a==226&&b==129&&c==159||a==227&&b==128&&c==128){return 3;}
    }
    return 0;
}
ArchiveRange trim_header(address bytes,ArchiveRange range,bool leading) on cpu {
    u64 at=range.start;u64 end=at+range.length;
    if(leading){
        while(at<end){u32 width=header_space_width(bytes,at,end);if(width==0){break;}at+=u64(width);}
    }
    u64 start=at;u64 content_end=at;
    while(at<end){
        u32 width=header_space_width(bytes,at,end);
        if(width==0){at+=1u64;content_end=at;}else{at+=u64(width);}
    }
    return ArchiveRange(start,content_end-start);
}
ArchiveReadNumber read_decimal(address bytes,ArchiveRange range) on cpu {
    u64 at=range.start;u64 end=at+range.length;
    if(at<end&&read_byte(bytes,at)==43){at+=1u64;}
    if(at==end){return ArchiveReadNumber(0u64,false);}
    u64 value=0u64;
    while(at<end){
        u32 digit=read_byte(bytes,at);
        if(digit<48||digit>57){return ArchiveReadNumber(0u64,false);}
        value=value*10u64+u64(digit-48);at+=1u64;
    }
    return ArchiveReadNumber(value,true);
}
ArchiveReadHeader read_header(address bytes,u64 length,u64 at) on cpu {
    if(at>4294967295u64){return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,10,at);}
    if(length-at<60u64){return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,1,at);}
    if(read_byte(bytes,at+58u64)!=96||read_byte(bytes,at+59u64)!=10){
        return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,2,at);
    }
    ArchiveReadNumber size=read_decimal(bytes,trim_header(bytes,ArchiveRange(at+48u64,10u64),true));
    if(!size.valid){return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,3,at+48u64);}
    u64 body=at+60u64;
    if(size.value>length-body){return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,1,body);}
    u64 next=body+size.value;
    if(next%2u64!=0u64){
        if(next==length){return ArchiveReadHeader(ArchiveRange(0u64,0u64),ArchiveRange(0u64,0u64),0u64,1,next);}
        next+=1u64;
    }
    return ArchiveReadHeader(trim_header(bytes,ArchiveRange(at,16u64),false),ArchiveRange(body,size.value),next,0,at);
}
bool header_named(address bytes,ArchiveRange name,u32 slashes) on cpu {
    if(name.length!=u64(slashes)){return false;}
    for(i in 0..slashes){if(read_byte(bytes,name.start+u64(i))!=47){return false;}}
    return true;
}
ArchiveReadResult read_terminated(address bytes,ArchiveRange region,u64 offset) on cpu {
    if(offset>region.length){return ArchiveReadResult(7,region.start);}
    u64 at=offset;
    while(at<region.length){if(read_byte(bytes,region.start+at)==0){return ArchiveReadResult(0,region.start+at);}at+=1u64;}
    return ArchiveReadResult(8,region.start+offset);
}
// The directory header and both row arrays share one allocation. Symbol names
// borrow the caller's immutable archive until the directory is destroyed.
extern "C" address malloc(u64 bytes) on cpu from "ucrtbase.dll";
extern "C" void free(address bytes) on cpu from "ucrtbase.dll";

struct LibraryMember {ArchiveReadMember member;u32 taken;};
struct LibraryDirectory {u64 member_count;u64 symbol_count;u64 symbol_offset;};
u64 directory_member_offset(u64 index) on cpu {
    return size_of<LibraryDirectory>()+index*size_of<LibraryMember>();
}
LibraryMember directory_member(address library,u64 index) on cpu {
    return unsafe_position<LibraryMember>(unsafe_address(address_bits(library)+directory_member_offset(index)));
}
void directory_member_put(address library,u64 index,LibraryMember member) on cpu {
    unsafe_position<LibraryMember>(unsafe_address(address_bits(library)+directory_member_offset(index)))=member;
}
u64 directory_symbol_offset(address library,u64 index) on cpu {
    return unsafe_position<LibraryDirectory>(library).symbol_offset+index*size_of<ArchiveSymbol>();
}
ArchiveSymbol directory_symbol(address library,u64 index) on cpu {
    return unsafe_position<ArchiveSymbol>(unsafe_address(address_bits(library)+directory_symbol_offset(library,index)));
}
void directory_symbol_put(address library,u64 index,ArchiveSymbol symbol) on cpu {
    unsafe_position<ArchiveSymbol>(unsafe_address(address_bits(library)+directory_symbol_offset(library,index)))=symbol;
}
u64 directory_symbol_position(address library,u64 count,ArchiveSymbol name) on cpu {
    u64 first=0u64;u64 end=count;
    while(first<end){
        u64 middle=first+(end-first)/2u64;
        if(compare_names(directory_symbol(library,middle),name)<i32(0)){first=middle+1u64;}else{end=middle;}
    }
    return first;
}
void directory_add_symbol(address library,ArchiveSymbol name) on cpu {
    u64 count=unsafe_position<LibraryDirectory>(library).symbol_count;
    u64 slot=directory_symbol_position(library,count,name);
    if(slot<count&&compare_names(directory_symbol(library,slot),name)==i32(0)){return;}
    u64 at=count;
    while(at>slot){at-=1u64;directory_symbol_put(library,at+1u64,directory_symbol(library,at));}
    directory_symbol_put(library,slot,name);
    unsafe_position<LibraryDirectory>(library).symbol_count=count+1u64;
}
extern "C" u64 library_member_count(address library) on cpu {return unsafe_position<LibraryDirectory>(library).member_count;}
extern "C" ArchiveMemberState library_member(address library,u64 index) on cpu {
    LibraryMember member=directory_member(library,index);
    u32 available=0;if(member.taken==0){available=1;}
    return ArchiveMemberState(member.member,available);
}
extern "C" ArchiveSelection library_take(address library,address name,u64 length) on cpu {
    LibraryDirectory value=unsafe_position<LibraryDirectory>(library);
    ArchiveSymbol key=ArchiveSymbol(name,length,0u64);
    u64 symbol=directory_symbol_position(library,value.symbol_count,key);
    if(symbol==value.symbol_count||compare_names(directory_symbol(library,symbol),key)!=i32(0)){return ArchiveSelection(0,0u64);}
    u64 offset=directory_symbol(library,symbol).member;
    u64 first=0u64;u64 end=value.member_count;
    while(first<end){
        u64 middle=first+(end-first)/2u64;
        if(u64(directory_member(library,middle).member.offset)<offset){first=middle+1u64;}else{end=middle;}
    }
    if(first==value.member_count||u64(directory_member(library,first).member.offset)!=offset){return ArchiveSelection(2,0u64);}
    LibraryMember selected=directory_member(library,first);
    if(selected.taken!=0){return ArchiveSelection(0,0u64);}
    selected.taken=1;directory_member_put(library,first,selected);return ArchiveSelection(1,first);
}

extern "C" void library_destroy(address library) on cpu {free(library);}

ArchiveReadResult fill_directory(address library,address bytes,u64 length,ArchiveRange directory,
    ArchiveRange names,u32 offset_count,u32 symbol_entries,address context,
    cfn<u32,address,ArchiveRange> validate) on cpu requires nonzero(validate) {
    u64 offsets=directory.start+4u64;
    u64 indices=offsets+u64(offset_count)*4u64+4u64;
    u64 at=indices+u64(symbol_entries)*2u64;
    for(i in 0..symbol_entries){
        u64 index_at=indices+u64(i)*2u64;
        u32 index=read_byte(bytes,index_at)|read_byte(bytes,index_at+1u64)<<8;
        if(index==0){return ArchiveReadResult(5,index_at);}
        if(index>offset_count){return ArchiveReadResult(6,index_at);}
        ArchiveReadResult name_end=read_terminated(bytes,directory,at-directory.start);
        if(name_end.error!=0){return name_end;}
        u32 offset=read_word(bytes,offsets+u64(index-1)*4u64);
        ArchiveRange name=ArchiveRange(at,name_end.at-at);
        if(validate(context,name)==0){return ArchiveReadResult(11,at);}
        directory_add_symbol(library,ArchiveSymbol(unsafe_address(address_bits(bytes)+at),name.length,u64(offset)));
        at=name_end.at+1u64;
    }
    // The first pass validated every header. The input stays immutable here.
    at=8u64;
    u64 member_index=0u64;
    while(at<length){
        ArchiveReadHeader item=read_header(bytes,length,at);
        if(!header_named(bytes,item.name,1)&&!header_named(bytes,item.name,2)){
            ArchiveRange name=item.name;
            if(name.length!=0u64&&read_byte(bytes,name.start)==47){
                ArchiveReadNumber index=read_decimal(bytes,ArchiveRange(name.start+1u64,name.length-1u64));
                if(!index.valid){return ArchiveReadResult(3,name.start+1u64);}
                ArchiveReadResult name_end=read_terminated(bytes,names,index.value);
                if(name_end.error!=0){return name_end;}
                name=ArchiveRange(names.start+index.value,name_end.at-names.start-index.value);
            }else{
                if(name.length==0u64||read_byte(bytes,name.start+name.length-1u64)!=47){return ArchiveReadResult(9,name.start);}
                name=ArchiveRange(name.start,name.length-1u64);
            }
            if(validate(context,name)==0){return ArchiveReadResult(11,name.start);}
            directory_member_put(library,member_index,LibraryMember(ArchiveReadMember(u32(at),name,item.body),0));
            member_index+=1u64;
        }
        at=item.next;
    }
    return ArchiveReadResult(0,0u64);
}

ArchiveLoad load_error(u32 error,u64 at) on cpu {
    return ArchiveLoad(unsafe_address(0u64),ArchiveReadResult(error,at));
}
extern "C" ArchiveLoad load(address bytes,u64 length,address context,
    cfn<u32,address,ArchiveRange> validate) on cpu requires nonzero(validate) {
    if(length<8u64){return load_error(1,0u64);}
    u64 at=8u64;
    u32 links=0;
    u64 member_count=0u64;
    ArchiveRange directory=ArchiveRange(0u64,0u64);
    ArchiveRange names=ArchiveRange(0u64,0u64);
    while(at<length){
        ArchiveReadHeader item=read_header(bytes,length,at);
        if(item.error!=0){return load_error(item.error,item.at);}
        if(header_named(bytes,item.name,1)){links+=1;if(links==2){directory=item.body;}}
        else if(header_named(bytes,item.name,2)){names=item.body;}
        else {member_count+=1u64;}
        at=item.next;
    }
    if(links<2){return load_error(4,0u64);}
    u64 end=directory.start+directory.length;
    if(directory.length<4u64){return load_error(1,directory.start);}
    u32 offset_count=read_word(bytes,directory.start);
    u64 offsets=directory.start+4u64;
    if(u64(offset_count)>(end-offsets)/4u64){return load_error(1,offsets);}
    at=offsets+u64(offset_count)*4u64;
    if(end-at<4u64){return load_error(1,at);}
    u32 symbol_entries=read_word(bytes,at);
    u64 indices=at+4u64;
    if(u64(symbol_entries)>(end-indices)/2u64){return load_error(1,indices);}

    u64 symbol_offset=directory_member_offset(member_count);
    address library=malloc(symbol_offset+u64(symbol_entries)*size_of<ArchiveSymbol>());
    if(address_bits(library)==0u64){return load_error(12,0u64);}
    unsafe_position<LibraryDirectory>(library)=LibraryDirectory(member_count,0u64,symbol_offset);
    ArchiveReadResult result=fill_directory(library,bytes,length,directory,names,offset_count,symbol_entries,context,validate);
    if(result.error!=0){
        library_destroy(library);
        return ArchiveLoad(unsafe_address(0u64),result);
    }
    return ArchiveLoad(library,result);
}
