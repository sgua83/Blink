module bootstrap.ownership_payload;
// Info flags: variants=1, own-presence=2. Item tags: absent=0, Unit=1,
// ordinary Value=2, Dataset=3, OwnedValuePayload=4. All storage is original.
extern "C" void join(address left,address right,u32 stored,address choice,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_equal)
    requires nonzero(host.state_set_moved)
    requires nonzero(host.proof_join_stored)
    requires nonzero(host.proof_join_common)
    requires nonzero(host.uniform)
    requires nonzero(host.set_uniform)
    requires nonzero(host.constant_equal)
    requires nonzero(host.clear_constant)
    requires nonzero(host.join_child)
    requires nonzero(host.move_child) { 
    address a=host.ownership(left);address b=host.ownership(right);
    if(host.state_equal(a,b)==0){host.state_set_moved(a);}
    address av=host.value(left);address bv=host.value(right);
    if(stored!=0){host.proof_join_stored(av,bv,choice);}else{host.proof_join_common(av,bv,choice);}
    host.set_uniform(av,host.uniform(av)&host.uniform(bv));
    if(host.constant_equal(av,bv)==0){host.clear_constant(av);}
    u64 count=host.inspect(left).count;u64 other=host.inspect(right).count;
    if(other<count){count=other;}u64 i=0u64;
    while(i<count){
        Item source=host.child(right,i);
        if(source.kind!=0){if(host.child(left,i).kind==0){host.move_child(left,right,i);}else{host.join_child(left,right,i,stored,choice);}}
        i+=1u64;
    }
}
extern "C" u32 available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state_available)
    requires nonzero(host.data_ownership) { 
    if(host.state_available(host.ownership(value))==0){return 0;}
    Info shape=host.inspect(value);u64 i=0u64;
    while(i<shape.count){
        if((shape.flags&1)==0&&host.field_owned(value,i)==0){i+=1u64;continue;}
        Item item=host.child(value,i);
        if(item.kind==0){if((shape.flags&1)==0){return 0;}}
        else if(item.kind==3){if(host.state_available(host.data_ownership(item.node))==0){return 0;}}
        else if(item.kind==4){if(available(item.node,host)==0){return 0;}}
        i+=1u64;
    }
    return 1;
}
extern "C" u32 all_available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state_available)
    requires nonzero(host.state_owned)
    requires nonzero(host.data_ownership) { 
    if(host.state_owned(host.ownership(value))==0){return 0;}return available(value,host);
}
extern "C" void borrow(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_set_borrowed)
    requires nonzero(host.view_empty)
    requires nonzero(host.install_source)
    requires nonzero(host.data_root)
    requires nonzero(host.data_ownership)
    requires nonzero(host.data_read) { 
    host.state_set_borrowed(host.ownership(value));Info shape=host.inspect(value);u64 i=0u64;
    while(i<shape.count){
        Item item=host.child(value,i);
        if(item.kind==3){
            host.state_set_borrowed(host.data_ownership(item.node));host.data_read(item.node);
            address root=host.data_root(item.node);if(host.view_empty(root)!=0){host.install_source(root);}
        }else if(item.kind==4){borrow(item.node,host);}
        i+=1u64;
    }
}
extern "C" u32 incomplete(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state_moved)
    requires nonzero(host.data_ownership) { 
    Info shape=host.inspect(value);u64 i=0u64;
    while(i<shape.count){
        if((shape.flags&1)==0&&host.field_owned(value,i)==0){i+=1u64;continue;}
        Item item=host.child(value,i);
        if(item.kind==0){return 1;}
        if(item.kind==4&&incomplete(item.node,host)!=0){return 1;}
        if(item.kind==3&&host.state_moved(host.data_ownership(item.node))!=0){return 1;}
        i+=1u64;
    }
    return 0;
}
LoopNeed loop_next(address state,LoopCursor original,Host host) on cpu
    requires nonzero(host.child) {
    LoopCursor cursor=original;
    while(cursor.index<cursor.count){
        Item a=host.child(cursor.before,cursor.index);Item b=host.child(cursor.after,cursor.index);
        cursor.index+=1u64;
        if((a.kind==3&&b.kind==3)||(a.kind==4&&b.kind==4)){
            unsafe_position<LoopCursor>(state)=cursor;
            return LoopNeed(a.kind,0,a.node,b.node);
        }
        if(a.kind==0&&b.kind!=0){return LoopNeed(0,0,unsafe_address(0u64),unsafe_address(0u64));}
    }
    return LoopNeed(0,1,unsafe_address(0u64),unsafe_address(0u64));
}
extern "C" LoopNeed loop_start(address state,address before,address after,address context,address path,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.ownership_loop)
    requires nonzero(host.loop_path)
    requires nonzero(host.path_possible) {
    if(host.ownership_loop(host.ownership(before),host.ownership(after),context,path)==0){
        return LoopNeed(0,0,unsafe_address(0u64),unsafe_address(0u64));
    }
    address next=host.loop_path(host.ownership(after),context,path);
    if(host.path_possible(next)==0){return LoopNeed(0,1,unsafe_address(0u64),unsafe_address(0u64));}
    u64 count=host.inspect(before).count;u64 other=host.inspect(after).count;
    if(other<count){count=other;}
    return loop_next(state,LoopCursor(before,after,next,count,0u64),host);
}
extern "C" LoopNeed loop_step(address state,u32 compatible,Host host) on cpu
    requires nonzero(host.child) {
    if(compatible==0){return LoopNeed(0,0,unsafe_address(0u64),unsafe_address(0u64));}
    return loop_next(state,unsafe_position<LoopCursor>(state),host);
}
void clear_value(address value,u32 keep_view,Host host) on cpu
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear) { 
    host.clear_constant(value);host.set_uniform(value,0);
    if(keep_view!=0){host.proof_borrow(value);}else{host.proof_clear(value);}
}
void reset_data(address data,Host host) on cpu
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear)
    requires nonzero(host.length_constant)
    requires nonzero(host.data_root)
    requires nonzero(host.data_length)
    requires nonzero(host.data_count) { 
    host.proof_borrow(host.data_root(data));address length=host.data_length(data);Count count=host.data_count(data);
    if(count.known!=0){host.length_constant(length,count.value);}else{host.proof_clear(length);}
    host.clear_constant(length);host.set_uniform(length,0);
}
extern "C" void invalidate(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.child)
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear)
    requires nonzero(host.length_constant)
    requires nonzero(host.data_root)
    requires nonzero(host.data_length)
    requires nonzero(host.data_count) { 
    clear_value(host.value(value),1,host);u64 count=host.inspect(value).count;u64 i=0u64;
    while(i<count){
        Item item=host.child(value,i);
        if(item.kind==3){reset_data(item.node,host);}
        else if(item.kind==4){invalidate(item.node,host);}
        else if(item.kind==2){clear_value(item.node,0,host);}
        i+=1u64;
    }
}
bool selected(Indices components,u64 at,Host host) on cpu
    requires nonzero(host.index_at) { 
    u64 i=0u64;while(i<components.count){if(host.index_at(components,i)==at){return true;}i+=1u64;}return false;
}
bool overlaps(address parent,Field field,Indices components,Host host) on cpu
    requires nonzero(host.component_count)
    requires nonzero(host.component_word)
    requires nonzero(host.component_index)
    requires nonzero(host.index_at) { 
    u64 count=host.component_count(field.ty);u64 i=0u64;
    while(i<count){u64 at=host.component_index(parent,field.offset+host.component_word(field.ty,i));if(selected(components,at,host)){return true;}i+=1u64;}
    return false;
}
// Only a real recursive Proof::assign consumer receives an owned selection.
address projection(address parent,Field field,Indices components,Host host) on cpu
    requires nonzero(host.component_count)
    requires nonzero(host.component_word)
    requires nonzero(host.component_index)
    requires nonzero(host.index_at)
    requires nonzero(host.fresh_indices)
    requires nonzero(host.push_index) { 
    address result=unsafe_address(0u64);u64 count=host.component_count(field.ty);u64 i=0u64;
    while(i<count){
        u64 at=host.component_index(parent,field.offset+host.component_word(field.ty,i));
        if(selected(components,at,host)){if(address_bits(result)==0u64){result=host.fresh_indices();}host.push_index(result,i);}
        i+=1u64;
    }
    return result;
}
extern "C" void invalidate_components(address value,Indices components,address borrowed,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.value)
    requires nonzero(host.child)
    requires nonzero(host.field)
    requires nonzero(host.aggregate_count)
    requires nonzero(host.component_count)
    requires nonzero(host.component_word)
    requires nonzero(host.component_index)
    requires nonzero(host.index_at)
    requires nonzero(host.set_uniform)
    requires nonzero(host.clear_constant)
    requires nonzero(host.proof_borrow)
    requires nonzero(host.proof_clear)
    requires nonzero(host.proof_assign)
    requires nonzero(host.length_constant)
    requires nonzero(host.data_root)
    requires nonzero(host.data_length)
    requires nonzero(host.data_count)
    requires nonzero(host.data_identity)
    requires nonzero(host.fresh_indices)
    requires nonzero(host.push_index)
    requires nonzero(host.indices_view)
    requires nonzero(host.drop_indices) { 
    Info shape=host.inspect(value);if((shape.flags&1)!=0){invalidate(value,host);return;}
    address scalar=host.value(value);host.proof_assign(scalar,components);host.clear_constant(scalar);host.set_uniform(scalar,0);
    u64 count=host.aggregate_count(value);u64 i=0u64;
    while(i<count){
        Field field=host.field(value,i);if(field.owns==0){i+=1u64;continue;}
        if(!overlaps(scalar,field,components,host)){i+=1u64;continue;}
        Item item=host.child(value,i);if(item.kind==0){i+=1u64;continue;}
        if(item.kind==3){
            if(address_bits(borrowed)==0u64||host.data_identity(item.node,borrowed)==0){reset_data(item.node,host);}
        }else if(item.kind==4){
            if((host.inspect(item.node).flags&1)!=0){invalidate(item.node,host);}
            else{address into=projection(scalar,field,components,host);if(address_bits(into)!=0u64){invalidate_components(item.node,host.indices_view(into),borrowed,host);host.drop_indices(into);}}
        }
        i+=1u64;
    }
}
extern "C" void mark_moved(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_set_owned)
    requires nonzero(host.state_set_moved)
    requires nonzero(host.data_ownership)
    requires nonzero(host.data_clear_residency) { 
    if((host.inspect(value).flags&2)!=0){host.state_set_moved(host.ownership(value));return;}
    host.state_set_owned(host.ownership(value));u64 count=host.inspect(value).count;u64 i=0u64;
    while(i<count){
        Item item=host.child(value,i);
        if(item.kind==3){host.state_set_moved(host.data_ownership(item.node));host.data_clear_residency(item.node);}
        else if(item.kind==4){mark_moved(item.node,host);}
        i+=1u64;
    }
}
void walk(address value,u32 retain_moved,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_moved)
    requires nonzero(visit) { 
    if(retain_moved==0&&host.state_moved(host.ownership(value))!=0){return;}
    u64 count=host.inspect(value).count;u64 i=0u64;
    while(i<count){Item item=host.child(value,i);if(item.kind==3){visit(item.node,context);}else if(item.kind==4){walk(item.node,retain_moved,context,visit,host);}i+=1u64;}
}
extern "C" void handles(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_moved)
    requires nonzero(visit) { walk(value,1,context,visit,host);}
extern "C" void resources(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state_moved)
    requires nonzero(visit) { walk(value,0,context,visit,host);}

extern "C" void select_presence(Item target,address inputs,u64 count,SelectionHost host) on cpu
    requires nonzero(host.alternative)
    requires nonzero(host.owner)
    requires nonzero(host.mode)
    requires nonzero(host.equal)
    requires nonzero(host.accumulate)
    requires nonzero(host.commit)
    requires nonzero(host.reset_residency)
    requires nonzero(host.join_residency)
    requires nonzero(host.push_proof)
    requires nonzero(host.finish_proofs)
    requires nonzero(host.inspect)
    requires nonzero(host.mutable_child)
    requires nonzero(host.select_child) {
    if(target.kind!=3&&target.kind!=4){return;}
    u64 start=0u64;
    Alternative selected=host.alternative(inputs,start);
    while(selected.item.kind!=target.kind){
        start+=1u64;
        selected=host.alternative(inputs,start);
    }
    address first=host.owner(selected.item);
    u32 same=1;
    u32 borrowed=host.mode(first)==2 ? 1 : 0;
    u64 at=start+1u64;
    while(at<count){
        Alternative candidate=host.alternative(inputs,at);
        if(candidate.item.kind==target.kind){
            address owner=host.owner(candidate.item);
            if(same!=0){same=host.equal(first,owner);}
            if(host.mode(owner)==2){borrowed=1;}
        }
        at+=1u64;
    }
    if(same!=0){
        host.commit(target,first,inputs,0);
    }else if(borrowed!=0){
        host.commit(target,first,inputs,1);
    }else{
        at=start;
        while(at<count){
            Alternative candidate=host.alternative(inputs,at);
            if(candidate.item.kind==target.kind){
                host.accumulate(inputs,candidate,host.owner(candidate.item));
            }
            at+=1u64;
        }
        host.commit(target,first,inputs,2);
    }

    if(target.kind==3){host.reset_residency(target);}
    at=start;
    while(at<count){
        Alternative candidate=host.alternative(inputs,at);
        if(candidate.item.kind==target.kind){
            host.push_proof(inputs,candidate);
            if(target.kind==3&&host.mode(host.owner(candidate.item))!=0){
                host.join_residency(target,candidate);
            }
        }
        at+=1u64;
    }
    host.finish_proofs(target,inputs);
    if(target.kind==4){
        u64 fields=host.inspect(target.node).count;
        at=0u64;
        while(at<fields){
            Item child=host.mutable_child(target.node,at);
            if(child.kind==3||child.kind==4){host.select_child(child,inputs,at);}
            at+=1u64;
        }
    }
}
