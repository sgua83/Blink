module bootstrap.ownership_payload;

extern "C" void visit_values(Item item,address context,cfn<void,address,address> visit,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.value) requires nonzero(host.child)
 requires nonzero(host.data_length) requires nonzero(visit) {
 if(item.kind==2){visit(item.node,context);}
 else if(item.kind==3){visit(host.data_length(item.node),context);}
 else if(item.kind==4){
  visit(host.value(item.node),context);
  u64 count=host.inspect(item.node).count;u64 at=0u64;
  while(at<count){visit_values(host.child(item.node,at),context,visit,host);at+=1u64;}
 }
}
// Info flags: variants=1, own-presence=2. Item tags: absent=0, Unit=1,
// ordinary Value=2, Dataset=3, OwnedValuePayload=4. All storage is original.
void join(address left,address right,u32 stored,address choice,address presence,u32 merge_metadata,Host host) on cpu
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
    requires nonzero(host.join_ownership)
    requires nonzero(host.move_child) { 
    if(merge_metadata!=0){
        address a=host.ownership(left);address b=host.ownership(right);
        if(address_bits(presence)!=0u64){host.join_ownership(a,b,presence);}
        else if(host.state_equal(a,b)==0){host.state_set_moved(a);}
    }
    address av=host.value(left);address bv=host.value(right);
    if(stored!=0){host.proof_join_stored(av,bv,choice);}else{host.proof_join_common(av,bv,choice);}
    host.set_uniform(av,host.uniform(av)&host.uniform(bv));
    if(host.constant_equal(av,bv)==0){host.clear_constant(av);}
    u64 count=host.inspect(left).count;u64 other=host.inspect(right).count;
    if(other<count){count=other;}u64 i=0u64;
    while(i<count){
        Item source=host.child(right,i);
        if(source.kind!=0){if(host.child(left,i).kind==0){host.move_child(left,right,i);}else{host.join_child(left,right,i,stored,choice,presence,merge_metadata);}}
        i+=1u64;
    }
}
void join_value(address left,address right,u32 stored,address choice,Host host) on cpu
    requires nonzero(host.proof_join_stored) requires nonzero(host.proof_join_common)
    requires nonzero(host.uniform) requires nonzero(host.set_uniform)
    requires nonzero(host.constant_equal) requires nonzero(host.clear_constant) {
    if(stored!=0){host.proof_join_stored(left,right,choice);}else{host.proof_join_common(left,right,choice);}
    host.set_uniform(left,host.uniform(left)&host.uniform(right));
    if(host.constant_equal(left,right)==0){host.clear_constant(left);}
}
void dataset_presence(address left,address right,address presence,u32 take_right,address choice,DataPresenceHost host) on cpu
    requires nonzero(host.moved) requires nonzero(host.ownership)
    requires nonzero(host.initialization) requires nonzero(host.residency) {
    u32 first=host.moved(left);u32 second=0;
    if(first==0){second=host.moved(right);}
    host.ownership(left,right,presence,take_right);
    if(first!=0){
        host.initialization(left,right,DataInitial::Right,presence,take_right);
        host.residency(left,right,DataJoin::Right,address(),take_right);
    }else if(second!=0){
        host.initialization(left,right,DataInitial::Left,presence,take_right);
        host.residency(left,right,DataJoin::Left,address(),take_right);
    }else{
        host.initialization(left,right,DataInitial::Selected,presence,take_right);
        if(address_bits(choice)!=0u64){host.residency(left,right,DataJoin::Selected,choice,take_right);}
    }
}
extern "C" void join_returned(Item left,Item right,u32 stored,address choice,address presence,u32 merge_metadata,ReturnedHost host) on cpu
    requires nonzero(host.values.inspect) requires nonzero(host.values.value)
    requires nonzero(host.values.ownership) requires nonzero(host.values.child)
    requires nonzero(host.values.state_equal) requires nonzero(host.values.state_set_moved)
    requires nonzero(host.values.proof_join_stored) requires nonzero(host.values.proof_join_common)
    requires nonzero(host.values.uniform) requires nonzero(host.values.set_uniform)
    requires nonzero(host.values.constant_equal) requires nonzero(host.values.clear_constant)
    requires nonzero(host.values.join_child) requires nonzero(host.values.join_ownership) requires nonzero(host.values.move_child)
    requires nonzero(host.values.data_root) requires nonzero(host.values.data_length) requires nonzero(host.values.data_ownership)
    requires nonzero(host.data.moved) requires nonzero(host.data.ownership)
    requires nonzero(host.data.initialization) requires nonzero(host.data.residency)
    requires nonzero(host.save) requires nonzero(host.restore) requires nonzero(host.maximum)
    requires nonzero(host.elements) requires nonzero(host.prefixes) {
    if(left.kind==1){return;}
    if(left.kind==4){join(left.node,right.node,stored,choice,presence,merge_metadata,host.values);return;}
    if(left.kind==2){
        address value=right.node;
        if(right.kind==3){value=host.values.data_length(right.node);}
        join_value(left.node,value,stored,choice,host.values);return;
    }
    if(address_bits(choice)!=0u64){host.save(host.context,left.node,right.node);}
    if(merge_metadata!=0){
        if(address_bits(presence)!=0u64){dataset_presence(left.node,right.node,presence,0,choice,host.data);}
        else{
            address first=host.values.data_ownership(left.node);address second=host.values.data_ownership(right.node);
            if(host.values.state_equal(first,second)==0){host.values.state_set_moved(first);}
            host.data.residency(left.node,right.node,address_bits(choice)!=0u64?DataJoin::Selected:DataJoin::Common,choice,0);
        }
    }
    address first=host.values.data_root(left.node);address second=host.values.data_root(right.node);
    if(stored!=0){host.values.proof_join_stored(first,second,address());}
    else{host.values.proof_join_common(first,second,address());}
    if(address_bits(choice)!=0u64){host.restore(host.context,left.node,choice);}
    DataJoin elements=DataJoin::Common;
    if(host.maximum(left.node)==0){elements=DataJoin::Right;}
    else if(host.maximum(right.node)==0){elements=DataJoin::Left;}
    else if(address_bits(choice)!=0u64){elements=DataJoin::Selected;}
    host.elements(left.node,right.node,elements,choice,presence);
    host.prefixes(left.node,right.node,choice);
    join_value(host.values.data_length(left.node),host.values.data_length(right.node),stored,choice,host.values);
}

bool usable(PayloadState state) on cpu {
    return state==PayloadState::Owned || state==PayloadState::Borrowed;
}

u32 available_children(address value,Info shape,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state)
    requires nonzero(host.data_ownership) { 
    u64 i=0u64;
    while(i<shape.count){
        if((shape.flags&1)==0&&host.field_owned(value,i)==0){i+=1u64;continue;}
        Item item=host.child(value,i);
        if(item.kind==0){if((shape.flags&1)==0){return 0;}}
        else if(item.kind==3){if(!usable(host.state(host.state_context,host.data_ownership(item.node)))){return 0;}}
        else if(item.kind==4){if(available(item.node,host)==0){return 0;}}
        i+=1u64;
    }
    return 1;
}
extern "C" u32 available(address value,Host host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.ownership) requires nonzero(host.child)
    requires nonzero(host.field_owned) requires nonzero(host.state) requires nonzero(host.data_ownership) {
    if(!usable(host.state(host.state_context,host.ownership(value)))){return 0;}
    return available_children(value,host.inspect(value),host);
}
extern "C" u32 all_available(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.field_owned)
    requires nonzero(host.state)
    requires nonzero(host.data_ownership) { 
    if(host.state(host.state_context,host.ownership(value))!=PayloadState::Owned){return 0;}
    return available_children(value,host.inspect(value),host);
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
    requires nonzero(host.state)
    requires nonzero(host.data_ownership) { 
    Info shape=host.inspect(value);u64 i=0u64;
    while(i<shape.count){
        if((shape.flags&1)==0&&host.field_owned(value,i)==0){i+=1u64;continue;}
        Item item=host.child(value,i);
        if(item.kind==0){return 1;}
        if(item.kind==4&&incomplete(item.node,host)!=0){return 1;}
        if(item.kind==3&&host.state(host.state_context,host.data_ownership(item.node))==PayloadState::Moved){return 1;}
        i+=1u64;
    }
    return 0;
}
extern "C" InitialWrite initial_write(Item item,u32 shape,Host host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.ownership) requires nonzero(host.child)
    requires nonzero(host.field_owned) requires nonzero(host.state) requires nonzero(host.data_ownership) {
    if(item.kind==3){
        return usable(host.state(host.state_context,host.data_ownership(item.node)))
            ? InitialWrite::Ready : InitialWrite::Unavailable;
    }
    if(item.kind==4){
        if(!usable(host.state(host.state_context,host.ownership(item.node)))){return InitialWrite::Unavailable;}
        Info layout=host.inspect(item.node);
        if((layout.flags&1)!=0){
            if(shape!=0&&incomplete(item.node,host)!=0){return InitialWrite::IncompleteShape;}
        }else if(available_children(item.node,layout,host)==0){return InitialWrite::Unavailable;}
    }
    return InitialWrite::Ready;
}
extern "C" u32 initial_root_incomplete(Item item,Host host) on cpu
    requires nonzero(host.inspect) requires nonzero(host.ownership) requires nonzero(host.child)
    requires nonzero(host.field_owned) requires nonzero(host.state) requires nonzero(host.data_ownership) {
    if(item.kind==3){return host.state(host.state_context,host.data_ownership(item.node))==PayloadState::Moved ? 1 : 0;}
    if(item.kind==4){
        return (host.state(host.state_context,host.ownership(item.node))==PayloadState::Moved || incomplete(item.node,host)!=0) ? 1 : 0;
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
extern "C" address data_at(address value, Indices path, u32 live, DataPathHost host) on cpu
    requires nonzero(host.index_at)
    requires nonzero(host.child)
    requires nonzero(host.moved) {
    address absent = unsafe_address(0u64);
    Item item = Item(value, 4);
    u64 at = 0u64;
    while (at < path.count) {
        if (item.kind != 4 || (live != 0 && host.moved(item) != 0)) {
            return absent;
        }
        item = host.child(item.node, host.index_at(path, at));
        at += 1u64;
    }
    if (item.kind != 3 || (live != 0 && host.moved(item) != 0)) {
        return absent;
    }
    return item.node;
}

void walk(address value,u32 retain_moved,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state)
    requires nonzero(visit) { 
    if(retain_moved==0&&host.state(host.state_context,host.ownership(value))==PayloadState::Moved){return;}
    u64 count=host.inspect(value).count;u64 i=0u64;
    while(i<count){Item item=host.child(value,i);if(item.kind==3){visit(item.node,context);}else if(item.kind==4){walk(item.node,retain_moved,context,visit,host);}i+=1u64;}
}
extern "C" void handles(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state)
    requires nonzero(visit) { walk(value,1,context,visit,host);}
extern "C" void resources(address value,address context,cfn<void,address,address> visit,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.ownership)
    requires nonzero(host.child)
    requires nonzero(host.state)
    requires nonzero(visit) { walk(value,0,context,visit,host);}

extern "C" u32 live_paths(address value, u32 independent, address context, LivePathHost host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.moved)
    requires nonzero(host.push)
    requires nonzero(host.pop)
    requires nonzero(host.visit) {
    if (host.moved(Item(value, 4)) != 0) { return 0; }
    Info info = host.inspect(value);
    u32 separate = independent != 0 && (info.flags & 1) == 0 ? 1 : 0;
    u64 index = 0u64;
    while (index < info.count) {
        Item item = host.child(value, index);
        if (item.kind == 4 || (item.kind == 3 && host.moved(item) == 0)) {
            host.push(context, index);
            u32 stop = item.kind == 4
                ? live_paths(item.node, separate, context, host)
                : host.visit(context, item.node, separate);
            host.pop(context);
            if (stop != 0) { return stop; }
        }
        index += 1u64;
    }
    return 0;
}

extern "C" void select_presence(Item target,address inputs,u64 count,SelectionHost host) on cpu
    requires nonzero(host.alternative)
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
    address first=host.alternative(inputs,0u64).owner;
    u32 same=1;
    u32 borrowed=host.mode(first)==2 ? 1 : 0;
    u64 at=1u64;
    while(at<count){
        address owner=host.alternative(inputs,at).owner;
        if(same!=0){same=host.equal(first,owner);}
        if(host.mode(owner)==2){borrowed=1;}
        at+=1u64;
    }
    if(same!=0){
        host.commit(target,first,inputs,0);
    }else if(borrowed!=0){
        host.commit(target,first,inputs,1);
    }else{
        at=0u64;
        while(at<count){
            host.accumulate(inputs,host.alternative(inputs,at));
            at+=1u64;
        }
        host.commit(target,first,inputs,2);
    }

    if(target.kind==3){host.reset_residency(target);}
    at=0u64;
    while(at<count){
        Alternative candidate=host.alternative(inputs,at);
        host.push_proof(inputs,candidate);
        if(target.kind==3&&host.mode(candidate.owner)!=0){
            host.join_residency(target,candidate);
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

extern "C" void collect_owned_places(address context,LookupNode node,LookupHost host) on cpu
    requires nonzero(host.matches_place)
    requires nonzero(host.capture)
    requires nonzero(host.inspect)
    requires nonzero(host.field)
    requires nonzero(host.visit_child) {
    if(address_bits(node.place)!=0u64&&host.matches_place(context,node.place)!=0){
        host.capture(context,node);
        return;
    }
    if(node.item.kind!=4){return;}
    Info shape=host.inspect(node.ty);
    if((shape.flags&1)!=0){return;}
    u64 i=0u64;
    while(i<shape.count){
        Field field=host.field(node.ty,i);
        if(field.owns!=0){host.visit_child(context,node,i,field);}
        i+=1u64;
    }
}

extern "C" void retain_presence(PayloadPair pair,address presence,u32 take_right,RetainedHost host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.ownership)
    requires nonzero(host.dataset)
    requires nonzero(host.data.moved) requires nonzero(host.data.ownership)
    requires nonzero(host.data.initialization) requires nonzero(host.data.residency) {
    if(pair.left.kind!=pair.right.kind){return;}
    if(pair.left.kind==3){
        dataset_presence(pair.left.node,pair.right.node,presence,take_right,address(),host.data);
        PayloadPair child=host.dataset(pair);
        retain_presence(child,presence,take_right,host);
    }else if(pair.left.kind==4){
        host.ownership(pair,presence,take_right);
        address retained=take_right!=0 ? pair.right.node : pair.left.node;
        u64 count=host.inspect(retained).count;
        u64 i=0u64;
        while(i<count){
            PayloadPair child=PayloadPair(host.child(pair.left.node,i),host.child(pair.right.node,i));
            retain_presence(child,presence,take_right,host);
            i+=1u64;
        }
    }
}

extern "C" void join_place_candidates(address context,u64 count,PlaceChoiceHost host) on cpu
    requires nonzero(host.condition)
    requires nonzero(host.merge) {
    u32 have=0;
    u64 at=0u64;
    while(at<count){
        if(host.condition(context,at)!=0){
            host.merge(context,at,have);
            have=1;
        }
        at+=1u64;
    }
}
