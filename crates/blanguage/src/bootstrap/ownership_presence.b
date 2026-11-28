module bootstrap.ownership_presence;
import bootstrap.ownership_position;
import bootstrap.fact_key;

struct PresenceInputHead { IdentityKey identity; u64 reg; };

Node read_presence(address value, address layout) on cpu {
    let kind = unsafe_position<PresenceTag>(value);
    Node result = Node(u32(kind), 0, omitted_index(), omitted_index(), address(), address(), address());
    if (kind == PresenceTag::None || kind == PresenceTag::All) { return result; }

    let view = unsafe_position<PresenceView>(layout);
    let payload = address_bits(value) + view.payload_offset;
    if (kind == PresenceTag::Input) {
        result.value = unsafe_address(payload);
    } else if (kind == PresenceTag::Selected) {
        let positions = unsafe_position<PositionView>(view.positions);
        let a = payload + view.first_index_offset;
        result.operation = u32(unsafe_position<u8>(unsafe_address(payload)));
        result.a = bootstrap.ownership_position::read_index(unsafe_address(a), positions);
        result.b = bootstrap.ownership_position::read_index(unsafe_address(a + positions.index_width), positions);
    } else if (kind == PresenceTag::Not) {
        result.first = unsafe_position<address>(unsafe_address(payload));
    } else {
        let branches = unsafe_position<PresenceBranches>(unsafe_address(payload));
        result.first = branches.first;
        result.second = branches.second;
    }
    return result;
}

extern "C" void clone_presence(address output,address source,Host host) on cpu
    requires nonzero(host.clone_input)
    requires nonzero(host.drop_payload) {
    if (address_bits(output) == address_bits(source)) { return; }
    let node = read_presence(source, host.layout);
    if (node.kind < 2) { write_truth(output, node.kind, host.layout, host.drop_payload); return; }
    if (node.kind == 2) { host.clone_input(output, node.value); return; }
    if (node.kind == 3) {
        write_comparison(output, Selection(node.operation, node.a, node.b), host.layout, host.drop_payload);
        return;
    }
    clone_presence_branches(output, unsafe_position<PresenceTag>(source),
        PresenceBranches(node.first, node.second), host.layout, host.drop_payload);
}

OwnershipNode read_ownership(address value, address layout) on cpu {
    let kind = unsafe_position<u32>(value);
    if (kind != 3) { return OwnershipNode(kind, address()); }
    let view = unsafe_position<PresenceView>(layout);
    let payload = unsafe_address(address_bits(value) + view.ownership_payload_offset);
    return OwnershipNode(kind, unsafe_position<address>(payload));
}

struct CloneContext { address output; u32 any; u32 prepend; };
struct EitherContext { address output; address branch; Selection original; u32 prepend; };
struct NotContext { u32 mode; };
struct SelectContext { address output; address choice; address wanted; PositionIndex index; u64 at; };
struct SelectBranchContext { address output; address predicate; address choice; address wanted; };

struct ProjectOther { address output; address value; address inputs; u32 looped; u32 any; };
struct ProjectSelected { address output; address inputs; Selection value; PositionIndex omit_a; PositionIndex omit_b; };
struct OwnerPair { address output; address a; address b; address choice; u32 has_choice; };
struct OwnerSecond { OwnerPair pair; address a_presence; };
struct OwnerChoice { address output; address a; address b; address choice; };
struct OwnerChoiceRight { address output; address left; address b; };
struct OwnerRestrict { address output; address owner; address when; };
struct OwnerRestrictRight { address output; address left; };
struct Compatible { address a; address b; address inputs; address path; };
struct Expected { Compatible pair; address presence; };
struct ActualPresence { Compatible pair; address expected; };
struct ActualCondition { Compatible pair; address expected; address presence; };
struct XorExpected { address expected; address actual; address path; };
struct XorActual { address first; address expected; address actual; address path; };

u32 comparison_mask(u32 operation) on cpu {
if(operation==0){return 2;} if(operation==1){return 5;}
    if(operation==2){return 1;} if(operation==3){return 4;}
    return operation==4 ? 3 : 6;
}

u32 reverse_comparison(u32 operation) on cpu {
if(operation==2){return 3;} if(operation==3){return 2;}
    if(operation==4){return 5;} if(operation==5){return 4;}
    return operation;
}

u32 inverse_comparison(u32 operation) on cpu {
if(operation==0){return 1;} if(operation==1){return 0;}
    if(operation==2){return 5;} if(operation==5){return 2;}
    return operation==3 ? 4 : 3;
}

extern "C" void relation(address output,Selection value,Host host) on cpu
    requires nonzero(host.drop_payload)
     {
Selection current=value;
    if(compare_position_index(current.a,current.b)==0){write_truth(output,(comparison_mask(current.operation)&2)!=0 ? 1 : 0, host.layout, host.drop_payload);return;}
    if(current.a.kind==1&&current.b.kind==1){
        u32 bit=current.a.value<current.b.value ? 1 : (current.a.value==current.b.value ? 2 : 4);
        write_truth(output,(comparison_mask(current.operation)&bit)!=0 ? 1 : 0, host.layout, host.drop_payload);return;
    }
    if(current.a.kind==1){PositionIndex saved=current.a;current.a=current.b;current.b=saved;current.operation=reverse_comparison(current.operation);}
    if(current.b.kind==1&&current.b.value==0){
        if(current.operation==2){write_truth(output,0, host.layout, host.drop_payload);return;}
        if(current.operation==5){write_truth(output,1, host.layout, host.drop_payload);return;}
        if(current.operation==4){current.operation=0;}else if(current.operation==3){current.operation=1;}
    }
    if(current.b.kind==1&&current.b.value==0xffffffff){
        if(current.operation==3){write_truth(output,0, host.layout, host.drop_payload);return;}
        if(current.operation==4){write_truth(output,1, host.layout, host.drop_payload);return;}
        if(current.operation==5){current.operation=0;}else if(current.operation==2){current.operation=1;}
    }
    write_comparison(output,current, host.layout, host.drop_payload);
}

extern "C" i32 compare(address a,address b,Host host) on cpu {
Node left=read_presence(a, host.layout);Node right=read_presence(b, host.layout);
    if(left.kind!=right.kind){return left.kind<right.kind ? i32(-1) : i32(1);}
    if(left.kind==2){return bootstrap.fact_key::order_identity(left.value,right.value);}
    if(left.kind==3){
        if(left.operation!=right.operation){return left.operation<right.operation ? i32(-1) : i32(1);}
        i32 first=compare_position_index(left.a,right.a);
        return first!=0 ? first : compare_position_index(left.b,right.b);
    }
    if(left.kind==4){return compare(left.first,right.first,host);}
    if(left.kind==5||left.kind==6){i32 first=compare(left.first,right.first,host);return first!=0 ? first : compare(left.second,right.second,host);}
    return i32(0);
}

extern "C" u32 uses_index(address value,PositionIndex wanted,Host host) on cpu
    requires nonzero(host.input_mentions) {
Node node=read_presence(value, host.layout);
    if(node.kind==3){return compare_position_index(node.a,wanted)==0||compare_position_index(node.b,wanted)==0 ? 1 : 0;}
    if (node.kind == 2) {
        if (wanted.kind == 1) { return 0; }
        if (bootstrap.fact_key::order_identity(node.value, wanted.identity) == 0) { return 1; }
        return host.input_mentions(node.value, wanted.identity);
    }
    if(node.kind==4){return uses_index(node.first,wanted,host);}
    if(node.kind==5||node.kind==6){return uses_index(node.first,wanted,host)!=0||uses_index(node.second,wanted,host)!=0 ? 1 : 0;}
    return 0;
}

extern "C" void from_value(address output, address value, u32 constant, address layout,
    cfn<void,address,address> capture, cfn<void,address,SharedNodeKind> drop_payload) on cpu
    requires nonzero(capture)
    requires nonzero(drop_payload) {
    if (constant == 0) { capture(output, value); }
    else { write_truth(output, constant == 2 ? 1 : 0, layout, drop_payload); }
}

u32 pair_mask(Selection left,Node right,u32 any) on cpu {
if(right.kind!=3){return 8;}
    u32 mask=comparison_mask(right.operation);
    if(compare_position_index(left.a,right.b)==0&&compare_position_index(left.b,right.a)==0){mask=((mask&1)<<2)|(mask&2)|((mask&4)>>2);}
    else if(compare_position_index(left.a,right.a)!=0||compare_position_index(left.b,right.b)!=0){return 8;}
    return any!=0 ? (comparison_mask(left.operation)|mask) : (comparison_mask(left.operation)&mask);
}

void masked_relation(address output,Selection value,u32 mask,Host host) on cpu
    requires nonzero(host.drop_payload)
     {
Selection current=value;
    if(mask==0||mask==7){write_truth(output,mask==7 ? 1 : 0, host.layout, host.drop_payload);return;}
    if(mask==1){current.operation=2;}else if(mask==2){current.operation=0;}
    else if(mask==4){current.operation=3;}else if(mask==3){current.operation=4;}
    else{current.operation=mask==5 ? 1 : 5;}
    relation(output,current,host);
}

extern "C" u32 clone_callback(address temporary,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
CloneContext context=unsafe_position<CloneContext>(data);
    if(context.prepend!=0){combine(temporary,context.output,context.any,host);move_presence(context.output,temporary, host.layout, host.drop_payload);}
    else{combine(context.output,temporary,context.any,host);}
    return 1;
}

void combine_cloned(address output,address other,u32 any,u32 prepend,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
CloneContext context=CloneContext(output,any,prepend);
    host.clone_scope(other,address_of(context),host,c_function(clone_callback));
}

extern "C" u32 either_callback(address temporary,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
EitherContext context=unsafe_position<EitherContext>(data);
    relation(temporary,context.original,host);
    combine_cloned(temporary,context.branch,0,0,host);
    if(context.prepend!=0){combine(temporary,context.output,1,host);move_presence(context.output,temporary, host.layout, host.drop_payload);}
    else{combine(context.output,temporary,1,host);}
    return 1;
}

struct PresenceReductionFrame { address first; address second; u32 kind; u32 right; };
struct PresenceReduction { u32 mask; usize depth; };

PresenceReduction find_presence_match(address other, Selection original, address layout,
    inout PositionFrames frames) on cpu {
    usize depth = 0u64;
    address current = other;
    while (true) {
        let node = read_presence(current, layout);
        let mask = pair_mask(original, node, 0);
        if (mask != 8) { return PresenceReduction(mask, depth); }
        if (node.kind == 5 || node.kind == 6) {
            let slot = reserve_position_frame(frames, depth, size_of<PresenceReductionFrame>());
            unsafe_position<PresenceReductionFrame>(slot) = PresenceReductionFrame(node.first, node.second, node.kind, 0);
            depth += 1u64;
            current = node.first;
            continue;
        }

        bool next = false;
        while (depth != 0u64) {
            let slot = position_frame(frames.data, depth - 1u64, size_of<PresenceReductionFrame>());
            if (unsafe_position<PresenceReductionFrame>(slot).right == 0) {
                unsafe_position<PresenceReductionFrame>(slot).right = 1;
                current = unsafe_position<PresenceReductionFrame>(slot).second;
                next = true;
                break;
            }
            depth -= 1u64;
        }
        if (!next) { return PresenceReduction(8, 0u64); }
    }
}

void apply_presence_match(address output, Selection original, PresenceReduction selected,
    PositionFrames frames, Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
    masked_relation(output, original, selected.mask, host);
    usize depth = selected.depth;
    while (depth != 0u64) {
        depth -= 1u64;
        let slot = position_frame(frames.data, depth, size_of<PresenceReductionFrame>());
        let frame = unsafe_position<PresenceReductionFrame>(slot);
        let sibling = frame.right == 0 ? frame.second : frame.first;
        if (frame.kind == 5) {
            combine_cloned(output, sibling, 0, frame.right, host);
        } else {
            EitherContext context = EitherContext(output, sibling, original, frame.right);
            host.truth_scope(0, address_of(context), host, c_function(either_callback));
        }
    }
}

extern "C" u32 within_selected(address output, address other, Selection original, Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
    PositionFrames frames = PositionFrames(address(), 0u64);
    let selected = find_presence_match(other, original, host.layout, frames);
    if (selected.mask != 8) { apply_presence_match(output, original, selected, frames, host); }
    release_position_frames(frames);
    return selected.mask != 8 ? 1 : 0;
}

u32 with_saved_selection(address output, address other, Selection value, Host host,
    cfn<u32,address,address,Selection,Host>
    requires nonzero(argument3.input_mentions)
    requires nonzero(argument3.drop_payload)
    requires nonzero(argument3.clone_input)
    requires nonzero(argument3.children_scope)
    requires nonzero(argument3.clone_scope)
    requires nonzero(argument3.truth_scope)
    requires nonzero(argument3.position_equal)
    requires nonzero(argument3.condition_scope)
    requires nonzero(argument3.condition_clone_scope)
    requires nonzero(argument3.condition_truth)
    requires nonzero(argument3.condition_input)
    requires nonzero(argument3.condition_index)
    requires nonzero(argument3.condition_negate)
    requires nonzero(argument3.condition_logical)
    requires nonzero(argument3.condition_substitute)
    requires nonzero(argument3.input_offset)
    requires nonzero(argument3.condition_truth_in) apply) on cpu
    requires nonzero(apply)
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
    SavedIdentities storage = selection_storage();
    let saved = save_selection(value, address_of(storage.a), address_of(storage.b));
    return apply(output, other, saved, host);
}

u32 within(address output,address other,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
Node node=read_presence(output, host.layout);
    if(node.kind!=3){return 0;}
    return with_saved_selection(output, other, Selection(node.operation, node.a, node.b),
        host, c_function(within_selected));
}

extern "C" void combine(address left,address right,u32 any,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
if(compare(left,right,host)==0){return;}
    Node a=read_presence(left, host.layout);Node b=read_presence(right, host.layout);
    if((a.kind==4&&compare(a.first,right,host)==0)||(b.kind==4&&compare(left,b.first,host)==0)){write_truth(left,any, host.layout, host.drop_payload);return;}
    if(any==0){
        if(within(left,right,host)!=0){return;}
        if(within(right,left,host)!=0){move_presence(left,right, host.layout, host.drop_payload);return;}
        if(a.kind==0||b.kind==0){write_truth(left,0, host.layout, host.drop_payload);return;}
        if(a.kind==1){move_presence(left,right, host.layout, host.drop_payload);return;}if(b.kind==1){return;}
    }else{
        if(a.kind==3){u32 mask=pair_mask(Selection(a.operation,a.a,a.b),b,1);if(mask!=8){masked_relation(left,Selection(a.operation,a.a,a.b),mask,host);return;}}
        if(a.kind==1||b.kind==1){write_truth(left,1, host.layout, host.drop_payload);return;}
        if(a.kind==0){move_presence(left,right, host.layout, host.drop_payload);return;}if(b.kind==0){return;}
    }
    pair_presence(left,right,any,host.layout);
}

extern "C" u32 not_children(address output,address left,address right,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
NotContext context=unsafe_position<NotContext>(data);
    if(context.mode!=0){negate(left,host);negate(right,host);combine(left,right,context.mode==1 ? 1 : 0,host);}
    move_presence(output,left, host.layout, host.drop_payload);return 1;
}

extern "C" void negate(address value,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
Node node=read_presence(value, host.layout);
    if(node.kind==0||node.kind==1){write_truth(value,node.kind==0 ? 1 : 0, host.layout, host.drop_payload);return;}
    if (node.kind == 3) {
        relation(value, Selection(inverse_comparison(node.operation), node.a, node.b), host);
        return;
    }
    if(node.kind==4||node.kind==5||node.kind==6){NotContext context=NotContext(node.kind==4 ? 0 : (node.kind==5 ? 1 : 2));host.children_scope(value,address_of(context),host,c_function(not_children));return;}
    wrap_not_presence(value,host.layout);
}

extern "C" u32 select_choice(address actual,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
SelectBranchContext context=unsafe_position<SelectBranchContext>(data);
    selected(actual,context.choice,context.wanted,host);
    combine(context.predicate,actual,0,host);combine(context.output,context.predicate,1,host);return 1;
}

extern "C" u32 select_predicate(address predicate,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
SelectContext context=unsafe_position<SelectContext>(data);
    PositionIndex constant=PositionIndex(1,u32(context.at),address(),0u64);
    relation(predicate,Selection(0,context.index,constant),host);
    SelectBranchContext child=SelectBranchContext(context.output,predicate,context.choice,context.wanted);
    return host.truth_scope(0,address_of(child),host,c_function(select_choice));
}

void project_position_choices(address output, address position, address wanted,
    PositionIndex index, u64 count, PositionView view, Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
    u64 at = 0u64;
    while (at < count) {
        let choice = bootstrap.ownership_position::position_child(position, at, view);
        SelectContext context = SelectContext(output, choice, wanted, index, at);
        host.truth_scope(0, address_of(context), host, c_function(select_predicate));
        at += 1u64;
    }
}

extern "C" void selected(address output,address position,address wanted,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
    let layout = unsafe_position<PresenceView>(host.layout);
    let view = unsafe_position<PositionView>(layout.positions);
    let node = bootstrap.ownership_position::inspect_position(position, view);
    if (node.kind != 10) {
        write_truth(output, host.position_equal(position, wanted), host.layout, host.drop_payload);
        return;
    }
    write_truth(output, 0, host.layout, host.drop_payload);
    if (node.index.kind == 1) {
        let at = u64(node.index.value);
        if (at < node.count) {
            selected(output, bootstrap.ownership_position::position_child(position, at, view), wanted, host);
        }
        return;
    }
    project_position_choices(output, position, wanted, node.index, node.count, view, host);
}

PositionIndex omitted_index() on cpu {
return PositionIndex(0,0,address(),0u64);
}

void index_condition(address output,Selection value,Host host) on cpu
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate) {
if(value.operation==0||value.operation==1){host.condition_index(output,0,value.a,value.b);if(value.operation==1){host.condition_negate(output);}return;}
    if(value.operation==2||value.operation==4){host.condition_index(output,value.operation==2 ? 1 : 2,value.a,value.b);return;}
    host.condition_index(output,value.operation==3 ? 1 : 2,value.b,value.a);
}

extern "C" u32 project_other(address temporary,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
ProjectOther context=unsafe_position<ProjectOther>(data);
    project(temporary,context.value,context.inputs,context.looped,host);
    host.condition_logical(context.output,temporary,context.any);return 1;
}

extern "C" u32 project_selected(address temporary,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
ProjectSelected context=unsafe_position<ProjectSelected>(data);
    relation(temporary,context.value,host);
    project(context.output,temporary,address(),0,host);
    host.condition_substitute(context.output,context.inputs,context.omit_a,context.omit_b);return 1;
}

void project(address output,address value,address inputs,u32 looped,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
Node node=read_presence(value, host.layout);
    if(node.kind==0||node.kind==1){host.condition_truth(output,node.kind);if(looped!=0){host.condition_substitute(output,inputs,omitted_index(),omitted_index());}return;}
    if(node.kind==2){host.condition_input(output,node.value);if(looped!=0){host.condition_substitute(output,inputs,omitted_index(),omitted_index());}return;}
    if(node.kind==3){
        Selection actual=Selection(node.operation,node.a,node.b);
        if(looped==0){index_condition(output,actual,host);return;}
        PositionIndex omit_a=omitted_index();PositionIndex omit_b=omitted_index();
        if(host.input_offset(inputs,node.b)!=0){
            if(actual.operation==5){actual.operation=3;omit_b=node.b;}
            else if(actual.operation==2){actual.operation=4;omit_b=node.b;}
        }
        if(host.input_offset(inputs,node.a)!=0){
            if(actual.operation==4){actual.operation=2;omit_a=node.a;}
            else if(actual.operation==3){actual.operation=5;omit_a=node.a;}
        }
        ProjectSelected context=ProjectSelected(output,inputs,actual,omit_a,omit_b);
        host.truth_scope(0,address_of(context),host,c_function(project_selected));return;
    }
    if(node.kind==4){project(output,node.first,inputs,looped,host);host.condition_negate(output);return;}
    project(output,node.first,inputs,looped,host);
    ProjectOther context=ProjectOther(output,node.second,inputs,looped,node.kind==6 ? 1 : 0);
    host.condition_scope(address_of(context),host,c_function(project_other));
}

extern "C" void condition(address output,address value,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
project(output,value,address(),0,host);
}

extern "C" void loop_condition(address output,address value,address inputs,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
project(output,value,inputs,1,host);
}

extern "C" u32 ownership_may_own(address value,Host host) on cpu
    {
OwnershipNode node=read_ownership(value, host.layout);return node.kind==1||node.kind==3 ? 1 : 0;
}

extern "C" u32 ownership_available(address value,Host host) on cpu
    {
OwnershipNode node=read_ownership(value, host.layout);return node.kind==1||node.kind==2 ? 1 : 0;
}

extern "C" void ownership_presence(address output,address value,Host host) on cpu
    requires nonzero(host.clone_input)
    requires nonzero(host.drop_payload) {
OwnershipNode node=read_ownership(value, host.layout);
    if(node.kind==3){clone_presence(output,node.presence,host);}else{write_truth(output,node.kind==0 ? 0 : 1, host.layout, host.drop_payload);}
}

extern "C" void ownership_from_presence(address output,address value,Host host) on cpu
    requires nonzero(host.drop_payload)
     {
Node node=read_presence(value, host.layout);
    if(node.kind==0||node.kind==1){write_ownership_constant(output,node.kind, host.layout, host.drop_payload);}else{conditional_ownership(output,value, host.layout, host.drop_payload);}
}

u32 same_ownership(address a,address b,Host host) on cpu {
OwnershipNode left=read_ownership(a, host.layout);OwnershipNode right=read_ownership(b, host.layout);
    if(left.kind!=right.kind){return 0;}
    return left.kind!=3||compare(left.presence,right.presence,host)==0 ? 1 : 0;
}

extern "C" u32 choice_right(address right,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerChoiceRight context=unsafe_position<OwnerChoiceRight>(data);
    negate(right,host);combine(right,context.b,0,host);combine(context.left,right,1,host);
    ownership_from_presence(context.output,context.left,host);return 1;
}

extern "C" u32 choice_left(address left,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerChoice context=unsafe_position<OwnerChoice>(data);
    combine(left,context.a,0,host);
    OwnerChoiceRight next=OwnerChoiceRight(context.output,left,context.b);
    return host.clone_scope(context.choice,address_of(next),host,c_function(choice_right));
}

extern "C" u32 owner_second(address b,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerSecond context=unsafe_position<OwnerSecond>(data);
    ownership_presence(b,context.pair.b,host);
    if(context.pair.has_choice==0){combine(context.a_presence,b,0,host);ownership_from_presence(context.pair.output,context.a_presence,host);return 1;}
    OwnerChoice choice=OwnerChoice(context.pair.output,context.a_presence,b,context.pair.choice);
    return host.clone_scope(context.pair.choice,address_of(choice),host,c_function(choice_left));
}

extern "C" u32 owner_first(address a,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerPair context=unsafe_position<OwnerPair>(data);
    ownership_presence(a,context.a,host);
    OwnerSecond next=OwnerSecond(context,a);
    return host.truth_scope(0,address_of(next),host,c_function(owner_second));
}

extern "C" void ownership_either(address output,address a,address b,address choice,u32 has_choice,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
if(same_ownership(a,b,host)!=0){clone_ownership(output,a,host.layout,host.drop_payload);return;}
    OwnershipNode left=read_ownership(a, host.layout);OwnershipNode right=read_ownership(b, host.layout);
    if(left.kind==2||right.kind==2){write_ownership_constant(output,0, host.layout, host.drop_payload);return;}
    OwnerPair context=OwnerPair(output,a,b,choice,has_choice);
    host.truth_scope(0,address_of(context),host,c_function(owner_first));
}

extern "C" u32 restrict_right(address right,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerRestrictRight context=unsafe_position<OwnerRestrictRight>(data);
    combine(context.left,right,0,host);ownership_from_presence(context.output,context.left,host);return 1;
}

extern "C" u32 restrict_left(address left,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnerRestrict context=unsafe_position<OwnerRestrict>(data);
    ownership_presence(left,context.owner,host);
    OwnerRestrictRight next=OwnerRestrictRight(context.output,left);
    return host.clone_scope(context.when,address_of(next),host,c_function(restrict_right));
}

extern "C" void ownership_restricted(address output,address value,address when,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnershipNode node=read_ownership(value, host.layout);
    if(node.kind==2){write_ownership_constant(output,2, host.layout, host.drop_payload);return;}
    OwnerRestrict context=OwnerRestrict(output,value,when);
    host.truth_scope(0,address_of(context),host,c_function(restrict_left));
}

extern "C" u32 xor_actual(address right,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
XorActual context=unsafe_position<XorActual>(data);
    host.condition_negate(right);host.condition_logical(context.first,right,0);
    host.condition_negate(context.expected);host.condition_logical(context.expected,context.actual,0);
    host.condition_logical(context.first,context.expected,1);
    return host.condition_truth_in(context.first,context.path)==1 ? 1 : 0;
}

extern "C" u32 xor_expected(address left,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
XorExpected context=unsafe_position<XorExpected>(data);
    XorActual next=XorActual(left,context.expected,context.actual,context.path);
    return host.condition_clone_scope(context.actual,address_of(next),host,c_function(xor_actual));
}

extern "C" u32 actual_condition(address actual,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
ActualCondition context=unsafe_position<ActualCondition>(data);
    condition(actual,context.presence,host);
    XorExpected next=XorExpected(context.expected,actual,context.pair.path);
    return host.condition_clone_scope(context.expected,address_of(next),host,c_function(xor_expected));
}

extern "C" u32 actual_presence(address presence,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
ActualPresence context=unsafe_position<ActualPresence>(data);
    ownership_presence(presence,context.pair.b,host);
    ActualCondition next=ActualCondition(context.pair,context.expected,presence);
    return host.condition_scope(address_of(next),host,c_function(actual_condition));
}

extern "C" u32 expected_condition(address expected,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
Expected context=unsafe_position<Expected>(data);
    loop_condition(expected,context.presence,context.pair.inputs,host);
    ActualPresence next=ActualPresence(context.pair,expected);
    return host.truth_scope(0,address_of(next),host,c_function(actual_presence));
}

extern "C" u32 expected_presence(address presence,address data,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
Compatible context=unsafe_position<Compatible>(data);
    ownership_presence(presence,context.a,host);
    Expected next=Expected(context,presence);
    return host.condition_scope(address_of(next),host,c_function(expected_condition));
}

extern "C" u32 ownership_compatible(address a,address b,address inputs,address path,Host host) on cpu
    requires nonzero(host.input_mentions)
    requires nonzero(host.drop_payload)
    requires nonzero(host.clone_input)
    requires nonzero(host.children_scope)
    requires nonzero(host.clone_scope)
    requires nonzero(host.truth_scope)
    requires nonzero(host.position_equal)
    requires nonzero(host.condition_scope)
    requires nonzero(host.condition_clone_scope)
    requires nonzero(host.condition_truth)
    requires nonzero(host.condition_input)
    requires nonzero(host.condition_index)
    requires nonzero(host.condition_negate)
    requires nonzero(host.condition_logical)
    requires nonzero(host.condition_substitute)
    requires nonzero(host.input_offset)
    requires nonzero(host.condition_truth_in) {
OwnershipNode left=read_ownership(a, host.layout);OwnershipNode right=read_ownership(b, host.layout);
    if((left.kind==2&&right.kind==2)||(left.kind==1&&right.kind==1)){return 1;}
    if(left.kind==2||right.kind==2){return 0;}
    Compatible context=Compatible(a,b,inputs,path);
    return host.truth_scope(0,address_of(context),host,c_function(expected_presence));
}

u64 emit_index(address context,PositionIndex index,cfn<u64,address,u32,u32> literal) on cpu
    requires nonzero(literal) {
    if(index.kind==1){return literal(context,index.value,1);}
    return index.reg;
}

extern "C" u64 emit_presence(address context,address value,EmitHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.compare)
    requires nonzero(host.invert)
    requires nonzero(host.slot)
    requires nonzero(host.load)
    requires nonzero(host.scoped_branch) {
    Node node=read_presence(value, host.layout);
    if(node.kind<=1){return host.constant(context,node.kind,0);}
    if(node.kind==2){return unsafe_position<PresenceInputHead>(node.value).reg;}
    if(node.kind==3){
        u64 a=emit_index(context,node.a,host.constant);
        u64 b=emit_index(context,node.b,host.constant);
        return host.compare(context,node.operation,a,b);
    }
    if(node.kind==4){
        u64 input=emit_presence(context,node.first,host);
        return host.invert(context,input);
    }
    u64 condition=emit_presence(context,node.first,host);
    u64 slot=host.slot(context);
    host.scoped_branch(context,EmitBranch(node.second,slot,condition,node.kind==6 ? 1 : 0));
    return host.load(context,slot);
}

extern "C" void emit_presence_branch(address context,EmitBranch work,address right,address fixed,EmitHost host) on cpu
    requires nonzero(host.constant)
    requires nonzero(host.compare)
    requires nonzero(host.invert)
    requires nonzero(host.slot)
    requires nonzero(host.store)
    requires nonzero(host.load)
    requires nonzero(host.scoped_branch)
    requires nonzero(host.swap_code)
    requires nonzero(host.fixed_store)
    requires nonzero(host.branch) {
    host.swap_code(context,right);
    u64 rhs=emit_presence(context,work.value,host);
    host.store(context,work.slot,rhs);
    host.swap_code(context,right);
    u64 bit=host.constant(context,work.either,0);
    host.fixed_store(context,fixed,work.slot,bit);
    if(work.either==0){
        host.branch(context,work.condition,right,fixed);
    }else{
        host.branch(context,work.condition,fixed,right);
    }
}

extern "C" u32 task_source_select(address input,u64 count,address output,TaskSourceHost host) on cpu
    requires nonzero(host.matches)
    requires nonzero(host.next_control)
    requires nonzero(host.partition)
    requires nonzero(host.truth) {
    bool matching=false;
    bool other=false;
    u64 index=0u64;
    while(index<count && !(matching && other)){
        if(host.matches(input,index)!=0){matching=true;}
        else{other=true;}
        index+=1u64;
    }
    if(!matching || !other){
        host.truth(output,u32(matching));
        return 1;
    }
    address control=host.next_control(input);
    while(address_bits(control)!=0u64){
        TaskSplit split=host.partition(input,control,output);
        if(split==TaskSplit::Selected){return 1;}
        if(split==TaskSplit::Unresolved){return 0;}
        control=host.next_control(input);
    }
    return 0;
}

extern "C" TaskSplit task_source_split(u64 count,address control,TaskSourceBranches branches,address output,
    TaskSourceHost host) on cpu
    requires nonzero(host.matches)
    requires nonzero(host.next_control)
    requires nonzero(host.partition)
    requires nonzero(host.truth)
    requires nonzero(host.merge) {
    if(branches.yes_count>=count || branches.no_count>=count){return TaskSplit::Unchanged;}
    if(task_source_select(branches.yes,branches.yes_count,branches.yes_result,host)==0){return TaskSplit::Unresolved;}
    if(task_source_select(branches.no,branches.no_count,branches.no_result,host)==0){return TaskSplit::Unresolved;}
    host.merge(output,control,branches.yes_result,branches.no_result);
    return TaskSplit::Selected;
}
