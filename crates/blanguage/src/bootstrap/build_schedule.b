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
u64 take_ready(address ready,inout u64 count) on cpu {
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

void complete_unit(address ready,inout u64 count,address waiting,address followers,u64 length) on cpu {
    u64 index=0u64;
    while (index<length) {
        u64 follower=word(followers,index);
        u64 pending=word(waiting,follower)-1u64;
        put_word(waiting,follower,pending);
        if (pending==0u64) { push_ready(ready,count,follower); }
        index+=1u64;
    }
}

const u64 NO_UNIT=0xffffffffffffffffu64;

extern "C" void run_worker(address context,WorkHost host) on cpu
 requires nonzero(host.acquire) requires nonzero(host.wait) requires nonzero(host.execute)
 requires nonzero(host.retire) requires nonzero(host.release) {
    while (true) {
        WorkQueue queue=host.acquire(context);
        u64 index=NO_UNIT;
        while (true) {
            if (queue.failure!=NO_UNIT || unsafe_position<ScheduleCounts>(queue.counts).unfinished==0u64) { return; }
            index=take_ready(queue.ready,unsafe_position<ScheduleCounts>(queue.counts).ready);
            if (index!=NO_UNIT) { break; }
            queue=host.wait(context);
        }
        WorkResult result=host.execute(context,index);
        bool keep=result.success!=0 || index+1u64<result.failure;
        queue=host.retire(context,keep ? 1 : 0);
        unsafe_position<ScheduleCounts>(queue.counts).unfinished-=1u64;
        if (result.success!=0) {
            UnitRange following=unsafe_position<UnitRange>(offset(queue.following,index*16u64));
            complete_unit(queue.ready,unsafe_position<ScheduleCounts>(queue.counts).ready,queue.waiting,
                offset(queue.followers,following.start*8u64),following.count);
        }
        host.release(context);
    }
}

extern "C" ResourceJob resource_job(address rows,u64 count,u64 stride,u64 ordinal) on cpu {
    u64 low=0u64;
    u64 high=count;
    // Empty programs can repeat an end. A claimed ordinal belongs to the first greater end.
    while(low<high) {
        u64 middle=low+(high-low)/2u64;
        u64 end=unsafe_position<u64>(offset(rows,middle*stride));
        if(end<=ordinal) { low=middle+1u64; } else { high=middle; }
    }
    u64 start=low==0u64 ? 0u64 : unsafe_position<u64>(offset(rows,(low-1u64)*stride));
    return ResourceJob(low,ordinal-start);
}

extern "C" void compile_resource_images(u64 count,address context,ResourceHost host) on cpu
 requires nonzero(host.describe) requires nonzero(host.cpu_range) requires nonzero(host.compile_cpu)
 requires nonzero(host.prepare_gpu) requires nonzero(host.compile_gpu)
 requires nonzero(host.emit_cpu) requires nonzero(host.emit_gpu) {
    u64 end=0u64;
    u64 index=0u64;
    while (index<count) {
        ResourceProgram current=host.describe(context,index);
        if (current.cpu!=0 && current.stages!=0u64) {
            end+=current.stages;
            host.cpu_range(context,index,end);
        }
        index+=1u64;
    }
    if (host.compile_cpu(context,end)==0) { return; }
    end=0u64;
    index=0u64;
    while (index<count) {
        ResourceProgram current=host.describe(context,index);
        if (current.cpu==0) {
            PreparedGpu prepared=host.prepare_gpu(context,index);
            // A page error occupies the original program's first work position.
            end+=prepared.ready!=0 ? current.stages : 1u64;
            unsafe_position<u64>(prepared.end)=end;
        }
        index+=1u64;
    }
    if (host.compile_gpu(context,end)==0) { return; }
    u64 gpu=0u64;
    index=0u64;
    while (index<count) {
        ResourceProgram current=host.describe(context,index);
        if (current.cpu!=0) { host.emit_cpu(context,index,current.stages); }
        else {
            host.emit_gpu(context,gpu,current.stages);
            gpu+=1u64;
        }
        index+=1u64;
    }
}
