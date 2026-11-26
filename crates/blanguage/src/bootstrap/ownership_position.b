module bootstrap.ownership_position;
// mode bit 0 applies application_complete; bit 1 applies application_word first.
// These are stack views of existing nodes, never a materialized projection tree.
struct Projection { address node; u32 mode; PositionIndex index; u32 word; };
PositionIndex absent_index() on cpu { return PositionIndex(0,0,unsafe_address(0u64),0u64);}
Projection original(address node) on cpu { return Projection(node,0,absent_index(),0);}
Projection child(Projection value,u64 at,Host host) on cpu
    requires nonzero(host.child) { Projection result=value;result.node=host.child(value.node,at);return result;}
bool same_index(PositionIndex a,PositionIndex b,Host host) on cpu
    requires nonzero(host.identity_equal) { 
    if(a.kind!=b.kind){return false;}
    if(a.kind==0){return true;}
    if(a.kind==1){return a.value==b.value;}
    return host.identity_equal(a.identity,b.identity)!=0;
}
PositionNode node(Projection value,Host host) on cpu
    requires nonzero(host.inspect) { 
    PositionNode result=host.inspect(value.node);
    if((value.mode&2)!=0){
        if(result.kind==7){result.kind=value.index.kind==0 ? 7 : 8;result.index=value.index;result.word=u64(value.word);}
        else if(result.kind==9){result.index=value.index;result.word=u64(value.word);}
    }
    if((value.mode&1)!=0){
        if(result.kind==8){result.kind=7;result.index=absent_index();}
        else if(result.kind==9){result.index=absent_index();}
    }
    return result;
}
// Only the selected root changes. Mode, index and word remain at their caller.
// Returning them recursively would carry unchanged pointer fields through the
// call contract without an independent consumer.
address normalized(Projection value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal) { 
    if(value.mode==0){return value.node;}
    PositionNode shape=node(value,host);
    if(shape.kind==12||shape.kind==11){
        Projection a=child(value,0u64,host);a.node=normalized(a,host);
        Projection b=child(value,1u64,host);b.node=normalized(b,host);
        if(equal_ready(a,b,host)){return a.node;}
        if(shape.kind==12){
            if(node(a,host).kind==0){return b.node;}
            if(node(b,host).kind==0){return a.node;}
        }else{
            u32 when=host.presence_kind(shape.condition);
            if(when==1){return a.node;}if(when==0){return b.node;}
        }
    }
    return value.node;
}
bool equal_ready(Projection left_view,Projection right_view,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal) { 
    PositionNode left=node(left_view,host);PositionNode right=node(right_view,host);
    if(left.kind!=right.kind){return false;}
    if(left.kind==0){return true;}
    if(left.kind==7){return left.owner==right.owner&&left.word==right.word;}
    if(left.kind==8){return left.owner==right.owner&&left.word==right.word&&same_index(left.index,right.index,host);}
    if(left.kind==9){return left.owner==right.owner&&left.member==right.member&&left.word==right.word&&same_index(left.index,right.index,host);}
    if(left.kind==10||left.kind==11||left.kind==12){
        if(left.count!=right.count){return false;}
        if(left.kind==10&&!same_index(left.index,right.index,host)){return false;}
        if(left.kind==11&&host.presence_equal(left.condition,right.condition)==0){return false;}
        u64 i=0u64;while(i<left.count){if(!equivalent(child(left_view,i,host),child(right_view,i,host),host)){return false;}i+=1u64;}
        return true;
    }
    return host.equal(left_view.node,right_view.node)!=0;
}
bool equivalent(Projection a,Projection b,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal) { 
    Projection left_view=a;left_view.node=normalized(a,host);
    Projection right_view=b;right_view.node=normalized(b,host);
    return equal_ready(left_view,right_view,host);
}
bool completable(Projection value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal) { 
    Projection actual=value;actual.node=normalized(value,host);PositionNode shape=node(actual,host);
    if(shape.kind==0||shape.kind==8||shape.kind==9){return true;}
    if(shape.kind==10||shape.kind==11||shape.kind==12){
        u64 i=0u64;while(i<shape.count){if(!completable(child(actual,i,host),host)){return false;}i+=1u64;}
        return true;
    }
    return false;
}
bool contained(Projection value,address initialized,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) { 
    Projection actual=value;actual.node=normalized(value,host);
    if(actual.mode==0){return host.contains(initialized,actual.node)!=0;}
    PositionNode shape=node(actual,host);
    if(shape.kind==0){return host.contains(initialized,actual.node)!=0;}
    if(shape.kind==7){return host.contains_parameter(initialized,shape.owner,shape.word)!=0;}
    if(shape.kind==8){return host.contains_element(initialized,shape.owner,shape.index,u32(shape.word))!=0;}
    if(shape.kind==9){return host.contains_input(initialized,shape.owner,shape.member,shape.index,u32(shape.word))!=0;}
    address iterator=host.iterate(initialized,unsafe_address(0u64));bool found=false;
    while(true){PositionItem item=host.next(iterator);if(item.present==0){break;}if(equivalent(actual,original(item.node),host)){found=true;break;}}
    host.drop_iterator(iterator);return found;
}
bool initialized(Projection value,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) { 
    Projection actual=value;actual.node=normalized(value,host);
    if(contained(actual,known,host)){return true;}
    if(completable(actual,host)){
        Projection whole=actual;whole.mode|=1;if(contained(whole,known,host)){return true;}
    }
    PositionNode shape=node(actual,host);
    if(shape.kind==0||shape.kind==5||shape.kind==6){return true;}
    if(shape.kind==10||shape.kind==11||shape.kind==12){
        u64 i=0u64;while(i<shape.count){if(!initialized(child(actual,i,host),known,host)){return false;}i+=1u64;}
        return true;
    }
    return false;
}
address materialize(Projection value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position) { 
    Projection actual=value;actual.node=normalized(value,host);PositionNode shape=node(actual,host);
    if(actual.mode==0){return host.clone_position(actual.node);}
    if(shape.kind==10){
        address choices=host.fresh_vector();
        u64 i=0u64;while(i<shape.count){host.push_vector(choices,materialize(child(actual,i,host),host));i+=1u64;}
        return host.make_position(shape,unsafe_address(0u64),unsafe_address(0u64),choices);
    }
    if(shape.kind==11||shape.kind==12){return host.make_position(shape,materialize(child(actual,0u64,host),host),materialize(child(actual,1u64,host),host),unsafe_address(0u64));}
    return host.make_position(shape,unsafe_address(0u64),unsafe_address(0u64),unsafe_address(0u64));
}
PositionIndex element_index(address value,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child) { 
    PositionNode shape=host.inspect(value);
    if(shape.kind==8||shape.kind==9){return shape.index;}
    if(shape.kind==10||shape.kind==11||shape.kind==12){
        u64 i=0u64;while(i<shape.count){PositionIndex index=element_index(host.child(value,i),host);if(index.kind!=0){return index;}i+=1u64;}
    }
    return absent_index();
}
bool prefix(address value,u32 length,u32 word,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) { 
    Projection whole=Projection(value,2,absent_index(),word);if(initialized(whole,known,host)){return true;}
    PositionNode shape=host.inspect(value);
    if(shape.kind==10||shape.kind==11||shape.kind==12){
        bool all=true;u64 i=0u64;while(i<shape.count){if(!prefix(host.child(value,i),length,word,known,host)){all=false;break;}i+=1u64;}
        if(all){return true;}
    }
    // For one index there is exactly one matching projected Position. The real
    // initialized BTreeSet already excludes duplicate positions; count directly.
    address iterator=host.iterate(known,unsafe_address(0u64));u64 count=0u64;
    while(true){
        PositionItem item=host.next(iterator);if(item.present==0){break;}
        PositionIndex index=element_index(item.node,host);
        if(index.kind==1&&index.value<length&&equivalent(Projection(value,2,index,word),original(item.node),host)){count+=1u64;}
    }
    host.drop_iterator(iterator);return count==u64(length);
}
bool scope(address value,u64 owner,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child) { 
    PositionNode shape=host.inspect(value);
    if(shape.kind==7||shape.kind==8){return shape.owner==owner;}
    if(shape.kind==10||shape.kind==11||shape.kind==12){u64 i=0u64;while(i<shape.count){if(scope(host.child(value,i),owner,host)){return true;}i+=1u64;}}
    return false;
}
bool uses_index(address value,PositionIndex index,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_uses_index) { 
    if(index.kind==1){return false;}PositionNode shape=host.inspect(value);
    if(shape.kind==8||shape.kind==9){return same_index(shape.index,index,host);}
    if(shape.kind==10&&same_index(shape.index,index,host)){return true;}
    if(shape.kind==11&&host.presence_uses_index(shape.condition,index)!=0){return true;}
    if(shape.kind==10||shape.kind==11||shape.kind==12){u64 i=0u64;while(i<shape.count){if(uses_index(host.child(value,i),index,host)){return true;}i+=1u64;}}
    return false;
}
extern "C" u32 select(address when,address a,address b,cfn<u32,address,address> equal,cfn<u32,address> presence_kind) on cpu
    requires nonzero(equal)
    requires nonzero(presence_kind) {
    if(equal(a,b)!=0){return 0;}
    u32 kind=presence_kind(when);
    return kind==1 ? 0 : (kind==0 ? 1 : 2);
}
extern "C" u32 both(address a,address b,cfn<u32,address,address> equal,cfn<PositionNode,address> inspect) on cpu
    requires nonzero(equal)
    requires nonzero(inspect) {
    if(equal(a,b)!=0){return 0;}
    if(inspect(a).kind==0){return 1;}
    if(inspect(b).kind==0){return 0;}
    return 2;
}
extern "C" address index(address values,PositionIndex selected,u64 width,Host host) on cpu
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.vector_count)
    requires nonzero(host.vector_at)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position) { 
    address result=host.fresh_vector();u64 count=host.vector_count(values);if(count==0u64){return result;}
    u64 component=0u64;while(component<width){
        if(selected.kind==1){host.push_vector(result,host.clone_position(host.vector_at(values,u64(selected.value)*width+component)));}
        else{
            address choices=host.fresh_vector();u64 i=0u64;while(i<count/width){host.push_vector(choices,host.clone_position(host.vector_at(values,i*width+component)));i+=1u64;}
            PositionNode shape=PositionNode(10,0u64,0u64,0u64,selected,unsafe_address(0u64),count/width);
            host.push_vector(result,host.make_position(shape,unsafe_address(0u64),unsafe_address(0u64),choices));
        }
    component+=1u64;}
    return result;
}
extern "C" address word(address value,PositionIndex selected,u32 field,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position) { return materialize(Projection(value,2,selected,field),host);}
extern "C" u32 ready(address value,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) { return initialized(original(value),known,host) ? 1 : 0;}
extern "C" u32 word_ready(address value,PositionIndex selected,u32 field,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) {
    return initialized(Projection(value,2,selected,field),known,host) ? 1 : 0;
}
extern "C" u32 prefix_ready(address value,u32 length,u32 field,address known,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator) { return prefix(value,length,field,known,host) ? 1 : 0;}
extern "C" u32 uses_scope(address value,u64 owner,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child) { return scope(value,owner,host) ? 1 : 0;}
address intersect(address a, address b, Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_set)
    requires nonzero(host.insert_set) {
    address result = host.fresh_set();
    u32 side = 0;
    while (side < 2) {
        // Preserve the first set's representative of keys present in both.
        address iterator = side == 0
            ? host.iterate(a, unsafe_address(0u64))
            : host.iterate(b, a);
        address other = side == 0 ? b : a;
        while (true) {
            PositionItem item = host.next(iterator);
            if (item.present == 0) { break; }
            if (initialized(original(item.node), other, host)) {
                host.insert_set(result, item.node);
            }
        }
        host.drop_iterator(iterator);
        side += 1;
    }
    return result;
}

extern "C" address merge_initialization(address exits, usize count, usize condition_count,
    Host host, InitializationHost initialization) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_set)
    requires nonzero(host.insert_set)
    requires nonzero(initialization.positions)
    requires nonzero(initialization.retain_conditional)
    requires nonzero(initialization.append_conditional)
    requires nonzero(initialization.drop_positions) {
    address common = intersect(initialization.positions(exits, 0u64),
        initialization.positions(exits, 1u64), host);
    usize i = 2u64;
    while (i < count) {
        address previous = common;
        common = intersect(previous, initialization.positions(exits, i), host);
        initialization.drop_positions(previous);
        i += 1u64;
    }

    // Each exit must subtract the same unconditional result before any
    // conditional positions are added to it.
    i = 0u64;
    while (i < condition_count) {
        initialization.retain_conditional(exits, i, common);
        i += 1u64;
    }
    i = 0u64;
    while (i < condition_count) {
        initialization.append_conditional(exits, i, common);
        i += 1u64;
    }
    return common;
}

extern "C" void loop_field_initialization(address context, address output, usize rows, usize states,
    PositionIndex selected, Host host, LoopFieldHost exit) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.fresh_vector)
    requires nonzero(host.push_vector)
    requires nonzero(host.make_position)
    requires nonzero(host.clone_position)
    requires nonzero(exit.row)
    requires nonzero(exit.known)
    requires nonzero(exit.publish) {
    PositionIndex whole = absent_index();
    usize row_index = 0u64;
    while (row_index < rows) {
        LoopFieldRow row = exit.row(context, row_index);
        usize field = 0u64;
        while (field < row.count) {
            u32 offset = unsafe_position<u32>(unsafe_address(address_bits(row.words) + field * 4u64));
            Projection element = Projection(row.position, 2, selected, offset);
            usize edge = 0u64;
            while (edge < states && initialized(element, exit.known(context, edge), host)) {
                edge += 1u64;
            }
            if (edge == states) {
                exit.publish(output, materialize(Projection(row.position, 2, whole, offset), host));
            }
            field += 1u64;
        }
        row_index += 1u64;
    }
}

extern "C" void array_initialization(address context, address output, usize count, ArrayRange range,
    PositionIndex selected, Host host, ArrayExitHost exits) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.equal)
    requires nonzero(host.identity_equal)
    requires nonzero(host.presence_kind)
    requires nonzero(host.presence_equal)
    requires nonzero(host.presence_uses_index)
    requires nonzero(host.contains)
    requires nonzero(host.contains_parameter)
    requires nonzero(host.contains_element)
    requires nonzero(host.contains_input)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(exits.next)
    requires nonzero(exits.state)
    requires nonzero(host.insert_set) {
    while (true) {
        PositionItem item = exits.next(context);
        if (item.present == 0) { break; }
        PositionNode shape = host.inspect(item.node);
        bool whole = range.nonempty != 0 && !uses_index(item.node, selected, host);
        bool elements = range.begin < range.limit && shape.kind == 10
            && u64(range.limit) <= shape.count && same_index(shape.index, selected, host);
        if (!whole && !elements) { continue; }

        usize at = 0u64;
        while (at < count) {
            ArrayState state = exits.state(context, at, item.node);
            bool ready = address_bits(state.known) == 0u64 ? state.ready != 0
                : initialized(original(item.node), state.known, host);
            if (!ready) { break; }
            at += 1u64;
        }
        if (at != count) { continue; }
        if (whole) { host.insert_set(output, item.node); }
        if (elements) {
            for (index in range.begin..range.limit) {
                host.insert_set(output, host.child(item.node, u64(index)));
            }
        }
    }
}


bool visit_effect(address position, address context, Host host, InitializationEffectsHost sink) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(sink.enter)
    requires nonzero(sink.leave)
    requires nonzero(sink.effect) {
    PositionNode shape = host.inspect(position);
    if (shape.kind == 2 || shape.kind == 3) { return sink.effect(context, position) != 0; }
    if (shape.kind == 10 || shape.kind == 11 || shape.kind == 12) {
        bool branches = shape.kind != 12;
        u64 at = 0u64;
        while (at < shape.count) {
            if (branches) { sink.enter(context, shape, at); }
            bool done = visit_effect(host.child(position, at), context, host, sink);
            if (branches) { sink.leave(context); }
            if (done) { return true; }
            at += 1u64;
        }
    }
    return false;
}

extern "C" u32 visit_initialization_effects(address initialized, address context,
    Host host, InitializationEffectsHost sink) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.child)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(sink.enter)
    requires nonzero(sink.leave)
    requires nonzero(sink.effect) {
    address iterator = host.iterate(initialized, unsafe_address(0u64));
    bool done = false;
    while (true) {
        PositionItem item = host.next(iterator);
        if (item.present == 0) { break; }
        done = visit_effect(item.node, context, host, sink);
        if (done) { break; }
    }
    host.drop_iterator(iterator);
    return done ? 1 : 0;
}


extern "C" void resolve_position(address value,address context,ResolutionHost resolver) on cpu
    requires nonzero(resolver.inspect)
    requires nonzero(resolver.child)
    requires nonzero(resolver.equal)
    requires nonzero(resolver.presence_kind)
    requires nonzero(resolver.truth)
    requires nonzero(resolver.assume)
    requires nonzero(resolver.leave)
    requires nonzero(resolver.mark)
    requires nonzero(resolver.query_choice)
    requires nonzero(resolver.assume_choice)
    requires nonzero(resolver.release_choices)
    requires nonzero(resolver.emit)
    requires nonzero(resolver.pair)
    requires nonzero(resolver.finish_pair)
    requires nonzero(resolver.result)
    requires nonzero(resolver.finish_indexed) {
    PositionNode shape=resolver.inspect(value);
    if(shape.kind==11){
        u32 truth=resolver.truth(context,shape.condition);
        if(truth!=2){
            resolve_position(resolver.child(value,truth==1 ? 0u64 : 1u64),context,resolver);
            return;
        }
        resolver.assume(context,shape.condition,0);
        resolve_position(resolver.child(value,0u64),context,resolver);
        resolver.leave(context);
        resolver.assume(context,shape.condition,1);
        resolve_position(resolver.child(value,1u64),context,resolver);
        resolver.leave(context);
        ResolutionPair pair=resolver.pair(context);
        resolver.finish_pair(context,value,select(shape.condition,pair.left,pair.right,resolver.equal,resolver.presence_kind));
        return;
    }
    if(shape.kind==12){
        resolve_position(resolver.child(value,0u64),context,resolver);
        resolve_position(resolver.child(value,1u64),context,resolver);
        ResolutionPair pair=resolver.pair(context);
        resolver.finish_pair(context,value,both(pair.left,pair.right,resolver.equal,resolver.inspect));
        return;
    }
    if(shape.kind!=10){
        resolver.emit(context,value);
        return;
    }

    // A proven index supersedes every unresolved candidate in this same selection.
    ResolutionBase base=resolver.mark(context);
    u64 at=0u64;
    while(at<shape.count){
        if(resolver.query_choice(context,shape.index,at)==1){
            resolver.release_choices(context,base.choices);
            resolve_position(resolver.child(value,at),context,resolver);
            return;
        }
        at+=1u64;
    }
    at=0u64;
    while(at<shape.count){
        if(resolver.assume_choice(context,base.choices+at)!=0){
            resolve_position(resolver.child(value,at),context,resolver);
            resolver.leave(context);
        }else{
            resolver.emit(context,unsafe_address(0u64));
        }
        at+=1u64;
    }
    resolver.release_choices(context,base.choices);
    address first=resolver.result(context,base.results);
    at=1u64;
    while(at<shape.count){
        if(resolver.equal(first,resolver.result(context,base.results+at))==0){
            resolver.finish_indexed(context,shape.index,base.results,0);
            return;
        }
        at+=1u64;
    }
    resolver.finish_indexed(context,shape.index,base.results,1);
}
