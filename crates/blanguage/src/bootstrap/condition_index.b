module bootstrap.condition_index;

struct ShapeSlot { OrderedNode link; NodeFields value; };
struct IdentitySlot { OrderedNode link; u64 key; u64 index; };

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

u64 intern_condition_shape(address tree_address,NodeFields shape) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(tree_address);
    u64 index=tree.root;
    u64 parent=0u64;
    i32 order=i32(0);
    while(index!=0u64){
        ShapeSlot slot=unsafe_position<ShapeSlot>(graph_slot(tree.records,size_of<ShapeSlot>(),index));
        order=shape_order(shape,slot.value);
        if(order==i32(0)){return index-1u64;}
        parent=index;
        index=order<i32(0)?slot.link.left:slot.link.right;
    }
    address fresh=ordered_tree_prepare(tree_address,size_of<ShapeSlot>());
    unsafe_position<ShapeSlot>(fresh).value=shape;
    ordered_tree_publish(tree_address,size_of<ShapeSlot>(),parent,order);
    return tree.count;
}

void bind_condition_identity(address tree_address,u64 key,u64 result) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(tree_address);
    u64 index=tree.root;
    u64 parent=0u64;
    i32 order=i32(0);
    while(index!=0u64){
        address at=graph_slot(tree.records,size_of<IdentitySlot>(),index);
        IdentitySlot slot=unsafe_position<IdentitySlot>(at);
        order=ordered_word(key,slot.key);
        if(order==i32(0)){unsafe_position<IdentitySlot>(at).index=result;return;}
        parent=index;
        index=order<i32(0)?slot.link.left:slot.link.right;
    }
    address fresh=ordered_tree_prepare(tree_address,size_of<IdentitySlot>());
    unsafe_position<IdentitySlot>(fresh).key=key;
    unsafe_position<IdentitySlot>(fresh).index=result;
    ordered_tree_publish(tree_address,size_of<IdentitySlot>(),parent,order);
}

extern "C" u64 condition_node_index(address graph,u64 key) on cpu {
    OrderedTree tree=unsafe_position<CanonicalGraph>(graph).identities;
    u64 index=tree.root;
    while(index!=0u64){
        IdentitySlot slot=unsafe_position<IdentitySlot>(graph_slot(tree.records,size_of<IdentitySlot>(),index));
        if(key==slot.key){return slot.index;}
        index=key<slot.key?slot.link.left:slot.link.right;
    }
    return 18446744073709551615u64;
}

extern "C" NodeFields condition_node_at(address graph,u64 index) on cpu {
    return unsafe_position<ShapeSlot>(graph_slot(unsafe_position<CanonicalGraph>(graph).nodes.records,
        size_of<ShapeSlot>(),index+1u64)).value;
}

extern "C" void condition_graph_drop(address graph) on cpu {
    fact_tree_clear(address_of(unsafe_position<CanonicalGraph>(graph).nodes));
    fact_tree_clear(address_of(unsafe_position<CanonicalGraph>(graph).identities));
}

u64 intern_condition_pair(address graph,address shapes,NodeKind kind,ClauseFields source,u32 keep_pairs) on cpu {
    u64 left=condition_node_index(graph,source.first);
    u64 right=condition_node_index(graph,source.second);
    if(keep_pairs==0&&left==right){return left;}
    return intern_condition_shape(shapes,NodeFields(kind,address(),left,right));
}

u64 intern_condition_node(address graph,address shapes,ClauseFields source,u32 keep_pairs) on cpu {
    if(source.kind==ClauseKind::Facts){
        return intern_condition_shape(shapes,NodeFields(NodeKind::Facts,source.facts,0u64,0u64));
    }
    if(source.kind==ClauseKind::Both||source.kind==ClauseKind::Branch){
        return intern_condition_pair(graph,shapes,NodeKind::Both,source,keep_pairs);
    }
    if(source.kind==ClauseKind::Either){
        return intern_condition_pair(graph,shapes,NodeKind::Either,source,keep_pairs);
    }
    return intern_condition_shape(shapes,NodeFields(NodeKind::Impossible,address(),0u64,0u64));
}

extern "C" void condition_node_ids(address context,address graph,cfn<ClauseFields,address,address> next,u32 keep_pairs)
 on cpu requires nonzero(next) {
    address shapes=address_of(unsafe_position<CanonicalGraph>(graph).nodes);
    address identities=address_of(unsafe_position<CanonicalGraph>(graph).identities);
    ClauseFields source=next(context,graph);
    while(source.present!=0){
        u64 index=intern_condition_node(graph,shapes,source,keep_pairs);
        bind_condition_identity(identities,source.key,index);
        source=next(context,graph);
    }
}
