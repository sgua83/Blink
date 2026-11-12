module blink.unicode_scalar;
bool is_scalar(u32 value) on cpu {
    return value<=0x10ffff&&(value<0xd800||value>0xdfff);
}
Utf8Units utf8_units(u32 value) on cpu {
    if(value<0x80){return Utf8Units(1,value);}
    if(value<0x800){return Utf8Units(2,(0xc0|(value>>6))|((0x80|(value&0x3f))<<8));}
    if(value<0x10000){return Utf8Units(3,(0xe0|(value>>12))|((0x80|((value>>6)&0x3f))<<8)|((0x80|(value&0x3f))<<16));}
    return Utf8Units(4,(0xf0|(value>>18))|((0x80|((value>>12)&0x3f))<<8)|((0x80|((value>>6)&0x3f))<<16)|((0x80|(value&0x3f))<<24));
}
