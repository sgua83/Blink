module bootstrap.build_digest;

address at(address base,u64 offset) on cpu {
    return unsafe_address(address_bits(base)+offset);
}

u32 byte(address input,u64 offset) on cpu {
    return u32(unsafe_position<u8>(at(input,offset)));
}

u32 big_word(address input,u64 offset) on cpu {
    return (byte(input,offset)<<24) | (byte(input,offset+1u64)<<16)
        | (byte(input,offset+2u64)<<8) | byte(input,offset+3u64);
}

u32 rotate(u32 value,u32 distance) on cpu {
    return (value>>distance) | (value<<(32-distance));
}

void compress(inout array<u32,8> state,address block) on cpu {
    array<u32,64> constants=array<u32,64>(
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
    );
    array<u32,16> words=array<u32,16>(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0);
    u32 a=state[0];
    u32 b=state[1];
    u32 c=state[2];
    u32 d=state[3];
    u32 e=state[4];
    u32 f=state[5];
    u32 g=state[6];
    u32 h=state[7];
    for (round in 0..64) {
        u32 slot=round&15;
        u32 word;
        if(round<16){
            word=big_word(block,u64(round)*4u64);
        }else{
            u32 left=words[(round+1)&15];
            u32 right=words[(round+14)&15];
            u32 small0=rotate(left,7)^rotate(left,18)^(left>>3);
            u32 small1=rotate(right,17)^rotate(right,19)^(right>>10);
            word=words[slot]+small0+words[(round+9)&15]+small1;
        }
        words[slot]=word;
        u32 large1=rotate(e,6)^rotate(e,11)^rotate(e,25);
        u32 choose=(e&f)^((~e)&g);
        u32 sum=h+large1+choose+constants[round]+word;
        u32 large0=rotate(a,2)^rotate(a,13)^rotate(a,22);
        u32 majority=(a&b)^(a&c)^(b&c);
        h=g;
        g=f;
        f=e;
        e=d+sum;
        d=c;
        c=b;
        b=a;
        a=sum+large0+majority;
    }
    state[0]+=a;
    state[1]+=b;
    state[2]+=c;
    state[3]+=d;
    state[4]+=e;
    state[5]+=f;
    state[6]+=g;
    state[7]+=h;
}

void put_byte(address output,u64 offset,u8 value) on cpu {
    unsafe_position<u8>(at(output,offset))=value;
}

array<u32,8> initial_state() on cpu {
    return array<u32,8>(
        0x6a09e667,0xbb67ae85,0x3c6ef372,0xa54ff53a,
        0x510e527f,0x9b05688c,0x1f83d9ab,0x5be0cd19);
}

void copy_bytes(address output,address input,u64 length) on cpu {
    u64 index=0u64;
    while(index<length){
        put_byte(output,index,u8(byte(input,index)));
        index+=1u64;
    }
}

u64 absorb(inout array<u32,8> state,address tail,u64 pending,address input,u64 length) on cpu {
    u64 offset=0u64;
    if(pending!=0u64){
        u64 room=64u64-pending;
        u64 copied=length<room ? length : room;
        copy_bytes(at(tail,pending),input,copied);
        if(copied<room){return pending+copied;}
        compress(state,tail);
        offset=copied;
    }
    while(length-offset>=64u64){
        compress(state,at(input,offset));
        offset+=64u64;
    }
    u64 remainder=length-offset;
    copy_bytes(tail,at(input,offset),remainder);
    return remainder;
}

void finish_digest(inout array<u32,8> state,address tail,u64 remainder,u64 length,address output) on cpu {
    put_byte(tail,remainder,u8(128));
    u64 end=remainder<56u64 ? 64u64 : 128u64;
    u64 offset=remainder+1u64;
    while(offset<end-8u64){
        put_byte(tail,offset,u8(0));
        offset+=1u64;
    }
    u64 bits=length*8u64;
    for (part in 0..8) {
        put_byte(tail,end-1u64-u64(part),u8(bits>>(u64(part)*8u64)));
    }
    compress(state,tail);
    if(end==128u64){compress(state,at(tail,64u64));}

    for (word in 0..8) {
        u32 value=state[word];
        u64 start=u64(word)*4u64;
        put_byte(output,start,u8(value>>24));
        put_byte(output,start+1u64,u8(value>>16));
        put_byte(output,start+2u64,u8(value>>8));
        put_byte(output,start+3u64,u8(value));
    }
}

extern "C" void sha256(address input,u64 length,address output) on cpu {
    array<u32,8> state=initial_state();
    array<u64,16> scratch=array<u64,16>(
        0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,
        0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64);
    address tail=address_of(scratch[0]);
    u64 pending=absorb(state,tail,0u64,input,length);
    finish_digest(state,tail,pending,length,output);
}

extern "C" void sha256_fields(address fields,u64 count,address output) on cpu {
    array<u32,8> state=initial_state();
    array<u64,16> scratch=array<u64,16>(
        0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,
        0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64);
    address tail=address_of(scratch[0]);
    u64 pending=0u64;
    u64 length=0u64;
    u64 index=0u64;
    while(index<count){
        DigestField field=unsafe_position<DigestField>(at(fields,index*size_of<DigestField>()));
        // The compiler host is little endian. Borrow the field length as the next eight bytes.
        u64 prefix=field.length;
        pending=absorb(state,tail,pending,address_of(prefix),8u64);
        pending=absorb(state,tail,pending,field.bytes,field.length);
        length+=8u64+field.length;
        index+=1u64;
    }
    finish_digest(state,tail,pending,length,output);
}
