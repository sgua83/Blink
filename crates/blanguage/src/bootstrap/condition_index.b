module bootstrap.condition_index;
import bootstrap.clause_storage;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct IdentitySlot { u64 key; u64 index_plus_one; };

address graph_slot(address records,u64 stride,u64 index) on cpu {
    return unsafe_address(address_bits(records)+(index-1u64)*stride);
}

i32 ordered_word(u64 left,u64 right) on cpu {
    return left==right?i32(0):(left<right?-i32(1):i32(1));
}

i32 shape_order(NodeFields left,NodeFields right) on cpu {
    i32 order=ordered_word(u64(u32(left.kind)),u64(u32(right.kind)));
    if(order!=i32(0)){return order;}
    if(left.kind==NodeKind::Facts){return fact_tree_compare(left.facts,right.facts);}
    order=ordered_word(left.first,right.first);
    return order!=i32(0)?order:ordered_word(left.second,right.second);
}

u64 intern_condition_shape(address nodes_address,address order_address,NodeFields shape) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(order_address);
    address nodes=unsafe_position<OrderedTree>(nodes_address).records;
    u64 index=tree.root;
    u64 parent=0u64;
    i32 order=i32(0);
    while(index!=0u64){
        NodeFields value=unsafe_position<NodeFields>(graph_slot(nodes,size_of<NodeFields>(),index));
        order=shape_order(shape,value);
        if(order==i32(0)){return index-1u64;}
        parent=index;
        OrderedNode link=unsafe_position<OrderedNode>(graph_slot(tree.records,size_of<OrderedNode>(),index));
        index=order<i32(0)?link.left:link.right;
    }
    address fresh=ordered_tree_prepare(nodes_address,size_of<NodeFields>());
    unsafe_position<NodeFields>(fresh)=shape;
    unsafe_position<OrderedTree>(nodes_address).count+=1u64;
    address link=ordered_tree_prepare(order_address,size_of<OrderedNode>());
    ordered_tree_publish(order_address,size_of<OrderedNode>(),parent,order);
    return tree.count;
}

address identity_slot(IdentityTable table,u64 key) on cpu {
    u64 hash=key^(key>>33u64);
    hash=hash*0xff51afd7ed558ccdu64;
    hash=hash^(hash>>33u64);
    u64 mask=table.capacity-1u64;
    u64 at=hash&mask;
    while(true){
        address slot=unsafe_address(address_bits(table.entries)+at*size_of<IdentitySlot>());
        IdentitySlot value=unsafe_position<IdentitySlot>(slot);
        if(value.index_plus_one==0u64||value.key==key){return slot;}
        at=(at+1u64)&mask;
    }
}

void grow_identity(inout IdentityTable table) on cpu {
    u64 limit=9223372036854775807u64/size_of<IdentitySlot>();
    if(table.capacity>limit/2u64){abort();return;}
    u64 capacity=table.capacity==0u64?8u64:table.capacity*2u64;
    handle heap=GetProcessHeap();
    address entries=HeapAlloc(heap,8,capacity*size_of<IdentitySlot>());
    if(entries==address()){abort();return;}
    IdentityTable grown=IdentityTable(entries,table.count,capacity);
    u64 index=0u64;
    while(index<table.capacity){
        address slot=unsafe_address(address_bits(table.entries)+index*size_of<IdentitySlot>());
        IdentitySlot value=unsafe_position<IdentitySlot>(slot);
        if(value.index_plus_one!=0u64){unsafe_position<IdentitySlot>(identity_slot(grown,value.key))=value;}
        index+=1u64;
    }
    if(table.entries!=address()){HeapFree(heap,0,table.entries);}
    table=grown;
}

u64 bind_condition_identity(inout IdentityTable table,u64 key,u64 result) on cpu {
    if(table.count==table.capacity/2u64){grow_identity(table);}
    address slot=identity_slot(table,key);
    let previous=unsafe_position<IdentitySlot>(slot).index_plus_one;
    if(previous==0u64){table.count+=1u64;}
    unsafe_position<IdentitySlot>(slot)=IdentitySlot(key,result+1u64);
    return previous-1u64;
}

u64 identity_value(IdentityTable table,u64 key) on cpu {
    if(table.capacity==0u64){return 18446744073709551615u64;}
    return unsafe_position<IdentitySlot>(identity_slot(table,key)).index_plus_one-1u64;
}

extern "C" u64 condition_node_index(address graph,u64 key) on cpu {
    return identity_value(unsafe_position<CanonicalGraph>(graph).identities,key);
}

void drop_identity(inout IdentityTable table) on cpu {
    if(table.entries!=address()){HeapFree(GetProcessHeap(),0,table.entries);}
    table=IdentityTable(address(),0u64,0u64);
}

extern "C" u64 contract_memo_imported(address memo,address clause) on cpu {
    return identity_value(unsafe_position<ContractClauseMemo>(memo).imported,address_bits(clause));
}

extern "C" void remember_contract_import(address memo,address clause,u64 node) on cpu {
    u64 previous=bind_condition_identity(unsafe_position<ContractClauseMemo>(memo).imported,address_bits(clause),node);
    // The live input prevents address reuse for the lifetime of the memo.
    if(previous==18446744073709551615u64){clause_retain(clause);}
}

extern "C" address contract_memo_emitted(address memo,u64 node,u32 retain) on cpu {
    u64 value=identity_value(unsafe_position<ContractClauseMemo>(memo).emitted,node);
    if(value==18446744073709551615u64){return address();}
    address result=unsafe_address(value);
    if(retain!=0){clause_retain(result);}
    return result;
}

extern "C" void remember_contract_emission(address memo,u64 node,address owned) on cpu {
    u64 previous=bind_condition_identity(unsafe_position<ContractClauseMemo>(memo).emitted,node,address_bits(owned));
    if(previous!=18446744073709551615u64){clause_release(unsafe_address(previous));}
}

extern "C" u64 contract_memo_negative(address memo,u64 node) on cpu {
    return identity_value(unsafe_position<ContractClauseMemo>(memo).negatives,node);
}

extern "C" void remember_contract_negative(address memo,u64 node,u64 negative) on cpu {
    bind_condition_identity(unsafe_position<ContractClauseMemo>(memo).negatives,node,negative);
}

extern "C" void start_contract_clause_memo(address memo) on cpu {
    OrderedTree facts=OrderedTree(address(),0u64,0u64,0u64);
    remember_contract_emission(memo,0u64,clause_impossible());
    remember_contract_emission(memo,1u64,clause_facts(address_of(facts)));
    remember_contract_negative(memo,0u64,1u64);
    remember_contract_negative(memo,1u64,0u64);
}

void drop_contract_clauses(inout IdentityTable table,bool keys_are_clauses) on cpu {
    u64 index=0u64;
    while(index<table.capacity){
        address slot=unsafe_address(address_bits(table.entries)+index*size_of<IdentitySlot>());
        let value=unsafe_position<IdentitySlot>(slot);
        if(value.index_plus_one!=0u64){
            u64 bits=keys_are_clauses?value.key:value.index_plus_one-1u64;
            clause_release(unsafe_address(bits));
        }
        index+=1u64;
    }
    drop_identity(table);
}

extern "C" void drop_contract_clause_memo(address memo) on cpu {
    drop_contract_clauses(unsafe_position<ContractClauseMemo>(memo).imported,true);
    drop_contract_clauses(unsafe_position<ContractClauseMemo>(memo).emitted,false);
    drop_identity(unsafe_position<ContractClauseMemo>(memo).negatives);
}

extern "C" NodeFields condition_node_at(address graph,u64 index) on cpu {
    return unsafe_position<NodeFields>(graph_slot(unsafe_position<CanonicalGraph>(graph).nodes.records,
        size_of<NodeFields>(),index+1u64));
}

extern "C" ConditionNodes condition_seal(address graph) on cpu {
    drop_identity(unsafe_position<CanonicalGraph>(graph).identities);
    OrderedTree nodes=unsafe_position<CanonicalGraph>(graph).nodes;
    return ConditionNodes(nodes.records,nodes.count);
}

extern "C" void condition_graph_drop(address graph) on cpu {
    fact_tree_clear(address_of(unsafe_position<CanonicalGraph>(graph).nodes));
    drop_identity(unsafe_position<CanonicalGraph>(graph).identities);
}

void condition_index_insert(address graph,address order,u64 key,NodeFields shape,u32 keep_pairs) on cpu {
    address nodes=address_of(unsafe_position<CanonicalGraph>(graph).nodes);
    u64 index=keep_pairs==0&&(shape.kind==NodeKind::Both||shape.kind==NodeKind::Either)&&shape.first==shape.second
        ? shape.first : intern_condition_shape(nodes,order,shape);
    bind_condition_identity(unsafe_position<CanonicalGraph>(graph).identities,key,index);
}
