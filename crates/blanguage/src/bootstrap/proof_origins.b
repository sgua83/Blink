module bootstrap.proof_origins;
import bootstrap.proof_facts;
import bootstrap.fact_key;

extern "C" void RtlMoveMemory(address output,address input,u64 bytes) on cpu from "KERNEL32.dll";

address origin_at(address records,usize index) on cpu {
    return unsafe_address(address_bits(records)+index*size_of<IdentityKey>());
}

extern "C" void origin_list_insert(address list,usize index,address identity) on cpu {
    ordered_tree_prepare(list,size_of<IdentityKey>());
    OrderedTree current=unsafe_position<OrderedTree>(list);
    if(index<current.count){
        RtlMoveMemory(origin_at(current.records,index+1u64),origin_at(current.records,index),
            (current.count-index)*size_of<IdentityKey>());
    }
    RtlMoveMemory(origin_at(current.records,index),identity,size_of<IdentityKey>());
    unsafe_position<OrderedTree>(list).count+=1u64;
}

extern "C" void origin_list_copy(address original,address output) on cpu {
    ordered_tree_copy(original,output,size_of<IdentityKey>());
}

extern "C" void origin_list_truncate(address list,usize length) on cpu {
    unsafe_position<OrderedTree>(list).count=min(length,unsafe_position<OrderedTree>(list).count);
}

extern "C" void origin_list_retain(address list,address context,cfn<u32,address,address> accept) on cpu
    requires nonzero(accept) {
    OrderedTree input=unsafe_position<OrderedTree>(list);
    usize write=0u64;
    usize read=0u64;
    while(read<input.count){
        address source=origin_at(input.records,read);
        if(accept(context,source)!=0){
            if(write!=read){RtlMoveMemory(origin_at(input.records,write),source,size_of<IdentityKey>());}
            write+=1u64;
        }
        read+=1u64;
    }
    unsafe_position<OrderedTree>(list).count=write;
}

address origin_row(address rows,usize index) on cpu {
    return unsafe_address(address_bits(unsafe_position<OrderedTree>(rows).records)+index*size_of<OrderedTree>());
}

void append_origin_row(address rows,OrderedTree list) on cpu {
    address next=ordered_tree_prepare(rows,size_of<OrderedTree>());
    unsafe_position<OrderedTree>(next)=list;
    unsafe_position<OrderedTree>(rows).count+=1u64;
}

extern "C" void origin_rows_push(address rows,address list) on cpu {
    append_origin_row(rows,unsafe_position<OrderedTree>(list));
    unsafe_position<OrderedTree>(list)=OrderedTree(address(),0u64,0u64,0u64);
}

extern "C" void origin_rows_resize(address rows,usize length) on cpu {
    while(unsafe_position<OrderedTree>(rows).count>length){
        unsafe_position<OrderedTree>(rows).count-=1u64;
        fact_tree_clear(origin_row(rows,unsafe_position<OrderedTree>(rows).count));
    }
    while(unsafe_position<OrderedTree>(rows).count<length){
        append_origin_row(rows,OrderedTree(address(),0u64,0u64,0u64));
    }
}

extern "C" void origin_rows_copy(address original,address output) on cpu {
    ordered_tree_copy(original,output,size_of<OrderedTree>());
    usize count=unsafe_position<OrderedTree>(original).count;
    usize index=0u64;
    while(index<count){
        address slot=origin_row(output,index);
        unsafe_position<OrderedTree>(slot)=OrderedTree(address(),0u64,0u64,0u64);
        origin_list_copy(origin_row(original,index),slot);
        index+=1u64;
    }
}

extern "C" void origin_rows_clear(address rows) on cpu {
    origin_rows_resize(rows,0u64);
    fact_tree_clear(rows);
}
