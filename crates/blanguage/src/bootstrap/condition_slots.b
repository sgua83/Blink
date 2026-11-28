module bootstrap.condition_slots;
import bootstrap.proof_facts;
import bootstrap.clause_storage;

address slot_at(address owner,usize index) on cpu {
    return unsafe_address(address_bits(unsafe_position<OrderedTree>(owner).records)+index*size_of<address>());
}

address take_condition_slot(address owner,usize index) on cpu {
    address slot=slot_at(owner,index);
    address previous=unsafe_position<address>(slot);
    unsafe_position<address>(slot)=address();
    return previous;
}

void release_condition_slot(address owner,usize index) on cpu {
    address previous=take_condition_slot(owner,index);
    if(previous!=address()){clause_release(previous);}
}

extern "C" void condition_slots_push(address owner,address payload) on cpu {
    address slot=ordered_tree_prepare(owner,size_of<address>());
    unsafe_position<address>(slot)=payload;
    unsafe_position<OrderedTree>(owner).count+=1u64;
}

extern "C" void condition_slots_resize(address owner,usize width) on cpu {
    while(unsafe_position<OrderedTree>(owner).count>width){
        unsafe_position<OrderedTree>(owner).count-=1u64;
        release_condition_slot(owner,unsafe_position<OrderedTree>(owner).count);
    }
    while(unsafe_position<OrderedTree>(owner).count<width){
        condition_slots_push(owner,address());
    }
}

extern "C" void condition_slots_clear(address owner) on cpu {
    condition_slots_resize(owner,0u64);
    fact_tree_clear(owner);
}

extern "C" void condition_slots_copy(address original,address result) on cpu {
    ordered_tree_copy(original,result,size_of<address>());
    usize index=0u64;
    while(index<unsafe_position<OrderedTree>(result).count){
        address payload=unsafe_position<address>(slot_at(result,index));
        if(payload!=address()){clause_retain(payload);}
        index+=1u64;
    }
}

extern "C" void condition_slots_set(address owner,usize index,address payload) on cpu {
    release_condition_slot(owner,index);
    unsafe_position<address>(slot_at(owner,index))=payload;
}

extern "C" address condition_slots_take(address owner,usize index) on cpu {
    return take_condition_slot(owner,index);
}

address retained_condition_case(address node) on cpu {
    clause_retain(node);
    return node;
}

extern "C" void condition_slots_conjoin(address owner,usize index,address no,address yes,cfn<u32,address> conflict) on cpu
    requires nonzero(conflict) {
    address original=unsafe_position<address>(slot_at(owner,index));
    address first=address();
    address second=address();
    if(original==address()){
        first=retained_condition_case(no);
        second=retained_condition_case(yes);
    }else{
        SourceNode current=clause_source(address_of(original));
        first=combine_clause_both(unsafe_position<address>(current.left),no,conflict);
        second=combine_clause_both(unsafe_position<address>(current.right),yes,conflict);
    }
    condition_slots_set(owner,index,clause_pair(StoredClauseKind::Condition,0,first,second));
}

extern "C" void condition_slots_deny(address owner,usize index,u32 branch) on cpu {
    address original=unsafe_position<address>(slot_at(owner,index));
    address other=address();
    if(original==address()){
        OrderedTree facts=OrderedTree(address(),0u64,0u64,0u64);
        other=clause_facts(address_of(facts));
    }else{
        SourceNode current=clause_source(address_of(original));
        other=retained_condition_case(unsafe_position<address>(branch==0 ? current.right : current.left));
    }
    address blocked=clause_impossible();
    address no=branch==0 ? blocked : other;
    address yes=branch==0 ? other : blocked;
    condition_slots_set(owner,index,clause_pair(StoredClauseKind::Condition,0,no,yes));
}
