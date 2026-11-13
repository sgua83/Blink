module bootstrap.build_schedule;

address offset(address data,u64 bytes) on cpu {
    return unsafe_address(address_bits(data)+bytes);
}
u64 word(address data,u64 index) on cpu {
    return unsafe_position<u64>(offset(data,index*8u64));
}
void put_word(address data,u64 index,u64 value) on cpu {
    unsafe_position<u64>(offset(data,index*8u64))=value;
}
// Initial indices in increasing order are already a min-heap.
extern "C" u64 initialize_ready(address ready,address waiting,u64 length) on cpu {
    u64 count=0u64;
    u64 index=0u64;
    while (index<length) {
        if (word(waiting,index)==0u64) {
            put_word(ready,count,index);
            count+=1u64;
        }
        index+=1u64;
    }
    return count;
}

extern "C" u64 take_ready(address ready,inout u64 count) on cpu {
    if (count==0u64) { return 0xffffffffffffffffu64; }
    u64 unit=word(ready,0u64);
    count-=1u64;
    u64 last=word(ready,count);
    u64 hole=0u64;
    u64 child=1u64;
    while (child<count) {
        u64 right=child+1u64;
        if (right<count && word(ready,right)<word(ready,child)) { child=right; }
        u64 next=word(ready,child);
        if (last<=next) { break; }
        put_word(ready,hole,next);
        hole=child;
        child=hole*2u64+1u64;
    }
    put_word(ready,hole,last);
    return unit;
}

void push_ready(address ready,inout u64 count,u64 unit) on cpu {
    u64 hole=count;
    count+=1u64;
    while (hole>0u64) {
        u64 parent=(hole-1u64)/2u64;
        u64 next=word(ready,parent);
        if (next<=unit) { break; }
        put_word(ready,hole,next);
        hole=parent;
    }
    put_word(ready,hole,unit);
}

extern "C" void complete_unit(address ready,inout u64 count,address waiting,address followers,u64 length) on cpu {
    u64 index=0u64;
    while (index<length) {
        u64 follower=word(followers,index);
        u64 pending=word(waiting,follower)-1u64;
        put_word(waiting,follower,pending);
        if (pending==0u64) { push_ready(ready,count,follower); }
        index+=1u64;
    }
}
