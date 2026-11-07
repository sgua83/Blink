module bootstrap.parser;

u32 byte_at(address source,u64 at) on cpu {
    return u32(unsafe_position<u8>(unsafe_address(address_bits(source)+at)));
}
TokenSpan span_at(address tokens,u64 at) on cpu {
    return unsafe_position<TokenSpan>(unsafe_address(address_bits(tokens)+at*16u64));
}
bool accepted(u64 length,u32 first,u32 second) on cpu {
    return first>=65&&first<=90 || first>=97&&first<=122 || first>=48&&first<=57 || first==95 || first==34
        || length==1u64 && (first==44||first==46||first==43||first==45||first==42||first==47
            ||first==37||first==38||first==124||first==94||first==126||first==33||first==63||first==58)
        || length==2u64 && (first==58&&second==58 || first==60&&second==60
            ||first==61&&second==61 ||first==33&&second==61 ||first==60&&second==61
            ||first==62&&second==61 ||first==38&&second==38 ||first==124&&second==124);
}
extern "C" AngleEnd angle_end(address source,address tokens,u64 count,u64 at) on cpu {
    if(at>=count){return AngleEnd(0u64,0);}
    TokenSpan start=span_at(tokens,at);
    if(start.end-start.start!=1u64||byte_at(source,start.start)!=60){return AngleEnd(0u64,0);}
    i64 depth=i64(0);u64 parentheses=0u64;u64 brackets=0u64;u64 index=at;
    while(index<count){
        TokenSpan token=span_at(tokens,index);u64 length=token.end-token.start;
        u32 first=0;u32 second=0;
        if(length>0u64){first=byte_at(source,token.start);}
        if(length>1u64){second=byte_at(source,token.start+1u64);}
        if(parentheses!=0u64||brackets!=0u64){
            if(length==1u64){
                if(first==40){parentheses+=1u64;}
                else if(first==41){if(parentheses==0u64){return AngleEnd(0u64,0);}parentheses-=1u64;}
                else if(first==91){brackets+=1u64;}
                else if(first==93){if(brackets==0u64){return AngleEnd(0u64,0);}brackets-=1u64;}
            }
            index+=1u64;continue;
        }
        if(length==1u64&&first==40){parentheses=1u64;index+=1u64;continue;}
        if(length==1u64&&first==91){brackets=1u64;index+=1u64;continue;}
        if(length==1u64&&first==60){depth+=i64(1);}
        else if(length==1u64&&first==62){depth-=i64(1);}
        else if(length==2u64&&first==62&&second==62){depth-=i64(2);}
        else if(!accepted(length,first,second)){return AngleEnd(0u64,0);}
        if(depth==i64(0)){return AngleEnd(index+1u64,1);}
        if(depth<i64(0)){return AngleEnd(0u64,0);}
        index+=1u64;
    }
    return AngleEnd(0u64,0);
}

extern "C" LinkStringResult decode_link_string(address source,u64 length,address output) on cpu {
    u64 at=0u64;u64 count=0u64;
    while(at<length){
        u32 c=byte_at(source,at);at+=1u64;
        if(c==92){
            if(at==length){return LinkStringResult(0u64,1);}
            c=byte_at(source,at);at+=1u64;
            if(c!=92&&c!=34){return LinkStringResult(0u64,1);}
        }
        if(c==0||c>127){return LinkStringResult(0u64,2);}
        unsafe_position<u8>(unsafe_address(address_bits(output)+count))=u8(c);
        count+=1u64;
    }
    if(count==0u64){return LinkStringResult(0u64,3);}
    return LinkStringResult(count,0);
}
