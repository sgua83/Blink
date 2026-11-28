module bootstrap.output_bindings;
import bootstrap.fact_key;
import bootstrap.proof_facts;
import bootstrap.proof_origins;

struct OutputBindingCell { OrderedNode links; IdentityKey source; IdentityKey target; };
struct OutputBindingPosition { address match_cell; u64 parent; i32 order; };

address output_binding_cell(OrderedTree tree,u64 index) on cpu {
    return unsafe_address(address_bits(tree.records)+(index-1u64)*size_of<OutputBindingCell>());
}

OutputBindingPosition output_binding_position(OrderedTree tree,address identity) on cpu {
    u64 parent=0u64;
    i32 order=i32(0);
    u64 index=tree.root;
    while(index!=0u64){
        address cell=output_binding_cell(tree,index);
        address key=unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).source)));
        order=order_identity(identity,key);
        if(order==i32(0)){return OutputBindingPosition(cell,parent,order);}
        parent=index;
        OrderedNode links=unsafe_position<OutputBindingCell>(cell).links;
        index=order<i32(0) ? links.left : links.right;
    }
    return OutputBindingPosition(address(),parent,order);
}

void store_output_binding(address owner,IdentityKey source,IdentityKey target,OutputBindingPosition position) on cpu {
    address cell=ordered_tree_prepare(owner,size_of<OutputBindingCell>());
    unsafe_position<OutputBindingCell>(cell).source=source;
    unsafe_position<OutputBindingCell>(cell).target=target;
    ordered_tree_publish(owner,size_of<OutputBindingCell>(),position.parent,position.order);
}

extern "C" void write_output_binding(address owner,address source,address target) on cpu {
    OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
    IdentityKey value=make_key_identity(capture_key_identity(target));
    if(position.match_cell==address()){
        store_output_binding(owner,make_key_identity(capture_key_identity(source)),value,position);
    }else{
        unsafe_position<OutputBindingCell>(position.match_cell).target=value;
    }
}

void collect_output_row(address owner,address origins,usize count,usize result,usize entry) on cpu {
    IdentityKey target=make_key_identity(NumericIdentity(IdentityKind::Register,result,0u64,0u64));
    usize index=0u64;
    while(index<count){
        address source=unsafe_address(address_bits(origins)+index*size_of<IdentityKey>());
        NumericIdentity key=capture_key_identity(source);
        if(key.kind==IdentityKind::TaskCompletion||
            (key.kind==IdentityKind::Register&&key.first>=entry&&key.first!=result)){
            OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
            if(position.match_cell==address()){store_output_binding(owner,make_key_identity(key),target,position);}
        }
        index+=1u64;
    }
}

extern "C" void collect_output_bindings(address owner,address rows,address registers,usize count,usize entry) on cpu {
    usize width=min(unsafe_position<OrderedTree>(rows).count,count);
    usize component=0u64;
    while(component<width){
        OrderedTree row=unsafe_position<OrderedTree>(origin_row(rows,component));
        address reg=unsafe_address(address_bits(registers)+component*size_of<usize>());
        collect_output_row(owner,row.records,row.count,unsafe_position<usize>(reg),entry);
        component+=1u64;
    }
}

extern "C" address find_output_binding(address owner,address source) on cpu {
    OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
    if(position.match_cell==address()){return address();}
    return unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(position.match_cell).target)));
}

extern "C" OutputBindingEntry next_output_binding(address owner,address cursor) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 previous=unsafe_position<u64>(cursor);
    u64 index=previous==0u64 ? ordered_tree_first(tree,size_of<OutputBindingCell>())
        : ordered_tree_next(tree,size_of<OutputBindingCell>(),previous);
    unsafe_position<u64>(cursor)=index;
    if(index==0u64){return OutputBindingEntry(address(),address());}
    address cell=output_binding_cell(tree,index);
    return OutputBindingEntry(unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).source))),
        unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).target))));
}
