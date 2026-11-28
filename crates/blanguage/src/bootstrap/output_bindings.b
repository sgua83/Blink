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

extern "C" void copy_output_bindings(address original,address output) on cpu {
    ordered_tree_copy(original,output,size_of<OutputBindingCell>());
}

extern "C" u32 same_output_bindings(address left,address right) on cpu {
    OrderedTree a=unsafe_position<OrderedTree>(left);
    OrderedTree b=unsafe_position<OrderedTree>(right);
    if(a.count!=b.count){return 0;}
    u64 i=0u64;
    u64 j=0u64;
    while(true){
        OutputBindingEntry x=next_output_binding(left,address_of(i));
        if(x.source==address()){return 1;}
        OutputBindingEntry y=next_output_binding(right,address_of(j));
        if(order_identity(x.source,y.source)!=0||order_identity(x.target,y.target)!=0){return 0;}
    }
}

struct RootResult { address root; bool present; };

RootResult root_of(address roots,address identity,bool linked) on cpu {
    address first=find_output_binding(roots,identity);
    if(first==address()){return RootResult(identity,false);}
    address parent=first;
    if(linked){
        while(true){
            address next=find_output_binding(roots,parent);
            if(next==address()||order_identity(next,parent)==0){break;}
            parent=next;
        }
    }
    return RootResult(parent,true);
}

void flatten_equality_roots(address roots) on cpu {
    u64 cursor=0u64;
    while(true){
        OutputBindingEntry entry=next_output_binding(roots,address_of(cursor));
        if(entry.source==address()){return;}
        // Each parent precedes its members, so the parent's entry is already final.
        address parent=find_output_binding(roots,entry.target);
        if(parent!=address()&&order_identity(parent,entry.target)!=0){
            unsafe_position<IdentityKey>(entry.target)=make_key_identity(capture_key_identity(parent));
        }
    }
}

extern "C" u32 extend_equality_roots(address roots,address context,cfn<address,address> next) on cpu
    requires nonzero(next) {
    bool merged=false;
    u32 changed=0;
    while(true){
        address key=next(context);
        if(key==address()||unsafe_position<FactKind>(key)!=FactKind::Equal){break;}
        FactView equality=read_fact_fields(key);
        RootResult left=root_of(roots,equality.first,merged);
        RootResult right=root_of(roots,equality.second,merged);
        i32 order=order_identity(left.root,right.root);
        if(order==0){
            if(!left.present){write_output_binding(roots,equality.first,left.root);}
            continue;
        }
        RootResult child=order<0 ? right : left;
        // Updating the map can move its cells. Snapshot both identities needed by a write.
        IdentityKey parent_key=make_key_identity(capture_key_identity(order<0 ? left.root : right.root));
        address parent=unsafe_address(address_bits(address_of(parent_key)));
        if(child.present){
            write_output_binding(roots,child.root,parent);
            merged=true;
        }
        changed=1;
        if(!left.present){write_output_binding(roots,equality.first,parent);}
        if(!right.present){write_output_binding(roots,equality.second,parent);}
    }
    if(merged){flatten_equality_roots(roots);}
    return changed;
}
