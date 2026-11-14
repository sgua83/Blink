module bootstrap.stage_fusion;

u64 binding_at(address data, u64 index) on cpu {
    return unsafe_position<u64>(unsafe_address(address_bits(data) + index * 8u64));
}

bool contains_at(BindingSpan span, inout u64 at, u64 binding) on cpu {
    while (at < span.count) {
        u64 current = binding_at(span.data, at);
        if (current >= binding) { return current == binding; }
        at += 1u64;
    }
    return false;
}

bool cross_dependency(StageBindings left, StageBindings right) on cpu {
    u64 left_cross = 0u64;
    u64 right_cross = 0u64;
    u64 left_read = 0u64;
    u64 left_write = 0u64;
    u64 right_read = 0u64;
    u64 right_write = 0u64;
    while (left_cross < left.cross.count || right_cross < right.cross.count) {
        u64 binding;
        if (left_cross < left.cross.count) {
            binding = binding_at(left.cross.data, left_cross);
            if (right_cross < right.cross.count) {
                u64 other = binding_at(right.cross.data, right_cross);
                if (other < binding) { binding = other; }
            }
        } else {
            binding = binding_at(right.cross.data, right_cross);
        }

        bool left_writes = contains_at(left.writes, left_write, binding);
        bool right_writes = contains_at(right.writes, right_write, binding);
        if ((left_writes && (right_writes || contains_at(right.reads, right_read, binding))) ||
            (right_writes && contains_at(left.reads, left_read, binding))) {
            return true;
        }

        if (left_cross < left.cross.count && binding_at(left.cross.data, left_cross) == binding) {
            left_cross += 1u64;
        }
        if (right_cross < right.cross.count && binding_at(right.cross.data, right_cross) == binding) {
            right_cross += 1u64;
        }
    }
    return false;
}

bool has_ordered_operation(FusionBlock block, cfn<FusionNode,FusionBlock,u64> read) on cpu
    requires nonzero(read) {
    u64 next = 0u64;
    while (next < block.count) {
        FusionNode node = read(block, next);
        if (node.ordered || has_ordered_operation(node.first, read) ||
            has_ordered_operation(node.second, read)) {
            return true;
        }
        next = node.next;
    }
    return false;
}

bool same_domain_field(DomainField left, DomainField right) on cpu {
    return left.present == right.present && left.binding == right.binding && left.word == right.word;
}

extern "C" bool can_fuse(FusionStage left, FusionStage right,
    cfn<FusionNode,FusionBlock,u64> read) on cpu requires nonzero(read) {
    return left.map != 0 && right.map != 0 && left.domain == right.domain &&
        right.boundary_before == 0 &&
        same_domain_field(left.prefix, right.prefix) &&
        same_domain_field(left.selection, right.selection) &&
        !cross_dependency(left.bindings, right.bindings) &&
        !has_ordered_operation(left.body, read) && !has_ordered_operation(right.body, read);
}

extern "C" u64 merge_bindings(address output, u64 left_count, BindingSpan right) on cpu {
    u64 left_at = left_count;
    u64 right_at = right.count;
    u64 write_at = left_count + right.count;
    while (left_at != 0u64 && right_at != 0u64) {
        u64 left = binding_at(output, left_at - 1u64);
        u64 next = binding_at(right.data, right_at - 1u64);
        u64 binding = left;
        if (left >= next) { left_at -= 1u64; }
        if (left <= next) { right_at -= 1u64; binding = next; }
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) = binding;
    }
    while (right_at != 0u64) {
        right_at -= 1u64;
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) =
            binding_at(right.data, right_at);
    }
    while (left_at != 0u64 && write_at != left_at) {
        left_at -= 1u64;
        write_at -= 1u64;
        unsafe_position<u64>(unsafe_address(address_bits(output) + write_at * 8u64)) =
            binding_at(output, left_at);
    }
    return write_at - left_at;
}

extern "C" bool extract_uniform(BindingSpan operands, BindingSpan results, address uniform) on cpu {
    u64 index = 0u64;
    while (index < operands.count) {
        u64 register = binding_at(operands.data, index);
        if (unsafe_position<u8>(unsafe_address(address_bits(uniform) + register)) == u8(0)) {
            return false;
        }
        index += 1u64;
    }
    index = 0u64;
    while (index < results.count) {
        u64 register = binding_at(results.data, index);
        unsafe_position<u8>(unsafe_address(address_bits(uniform) + register)) = u8(1);
        index += 1u64;
    }
    return true;
}

extern "C" void number_used_slots(address slots, u64 count) on cpu {
    u64 used = 0u64;
    u64 index = 0u64;
    while (index < count) {
        address at = unsafe_address(address_bits(slots) + index * size_of<u64>());
        if (unsafe_position<u64>(at) != 0u64) {
            used += 1u64;
            unsafe_position<u64>(at) = used;
        }
        index += 1u64;
    }
    return;
}

address selection_slot(address slots, u64 index) on cpu {
    return unsafe_address(address_bits(slots) + index * size_of<SelectionSlot>());
}

bool selection_allowed(SelectionPlan input, SelectionNode node) on cpu {
    if (node.kind == SelectionKind::LocalRead) {
        return unsafe_position<SelectionSlot>(selection_slot(input.slots, node.first)).yes == 0u64;
    }
    if (node.kind == SelectionKind::DataRead) {
        address at = unsafe_address(address_bits(input.indices) + node.second);
        return node.first == input.domain && unsafe_position<u8>(at) != u8(0);
    }
    if (node.kind == SelectionKind::Arithmetic) { return node.first == 0u64; }
    return node.kind != SelectionKind::Ordered;
}

extern "C" u64 plan_selection(SelectionPlan input, address context,
    cfn<SelectionNode,address,FusionBlock,u64> read) on cpu requires nonzero(read) {
    u64 count = 0u64;
    u64 index = 0u64;
    while (index < input.yes.count) {
        SelectionNode node = read(context, input.yes, index);
        if (!selection_allowed(input, node)) { return 0u64; }
        if (node.kind == SelectionKind::LocalRead) {
            // Until the no arm, its slot word records reads of unwritten slots.
            address at = selection_slot(input.slots, node.first);
            unsafe_position<SelectionSlot>(at) = SelectionSlot(0u64, 1u64);
        } else if (node.kind == SelectionKind::LocalWrite) {
            address at = selection_slot(input.slots, node.first);
            SelectionSlot slot = unsafe_position<SelectionSlot>(at);
            if (slot.no != 0u64) { return 0u64; }
            if (slot.yes == 0u64) { count += 1u64; }
            unsafe_position<SelectionSlot>(at) = SelectionSlot(node.second + 1u64, 0u64);
        }
        index += 1u64;
    }
    if (count == 0u64 || count > input.no.count || count > input.following.count) { return 0u64; }

    u64 matched = 0u64;
    index = 0u64;
    while (index < input.no.count) {
        SelectionNode node = read(context, input.no, index);
        if (!selection_allowed(input, node)) { return 0u64; }
        if (node.kind == SelectionKind::LocalWrite) {
            address at = selection_slot(input.slots, node.first);
            SelectionSlot slot = unsafe_position<SelectionSlot>(at);
            if (slot.yes == 0u64) { return 0u64; }
            if (slot.no == 0u64) { matched += 1u64; }
            unsafe_position<SelectionSlot>(at) = SelectionSlot(slot.yes, node.second + 1u64);
        }
        index += 1u64;
    }
    if (count != matched) { return 0u64; }
    index = 0u64;
    while (index < count) {
        SelectionNode node = read(context, input.following, index);
        if (node.kind != SelectionKind::LocalRead) { return 0u64; }
        address at = selection_slot(input.slots, node.first);
        SelectionSlot slot = unsafe_position<SelectionSlot>(at);
        if (slot.yes == 0u64) { return 0u64; }
        address target = unsafe_address(address_bits(input.values) + index * size_of<SelectionValue>());
        unsafe_position<SelectionValue>(target) = SelectionValue(
            node.first, node.second, slot.yes - 1u64, slot.no - 1u64);
        // A second read of the same slot is not one of the matched outputs.
        unsafe_position<SelectionSlot>(at) = SelectionSlot(0u64, 0u64);
        index += 1u64;
    }
    return count;
}

address binding_flag(address flags, u64 binding) on cpu {
    return unsafe_address(address_bits(flags) + binding);
}

void write_binding(address output, u64 index, u64 binding) on cpu {
    unsafe_position<u64>(unsafe_address(address_bits(output) + index * 8u64)) = binding;
}

extern "C" MapAccessCounts map_access(MapAccess input) on cpu {
    u64 reads = 0u64;
    u64 writes = 0u64;
    u64 binding = 0u64;
    while (binding < input.binding_count) {
        u8 flag = unsafe_position<u8>(binding_flag(input.flags, binding));
        if ((flag & u8(1)) != u8(0)) {
            write_binding(input.reads, reads, binding);
            reads += 1u64;
        }
        if ((flag & u8(2)) != u8(0)) {
            write_binding(input.writes, writes, binding);
            writes += 1u64;
        }
        binding += 1u64;
    }
    u64 cross = 0u64;
    u64 at = 0u64;
    while (at < input.cross_count) {
        binding = binding_at(input.cross, at);
        if (unsafe_position<u8>(binding_flag(input.flags, binding)) != u8(0)) {
            write_binding(input.cross, cross, binding);
            cross += 1u64;
        }
        at += 1u64;
    }
    return MapAccessCounts(reads, writes, cross);
}

address tile_end(address plane, u64 slot) on cpu {
    return unsafe_address(address_bits(plane) + slot * size_of<i64>());
}

extern "C" void place_tile(address rows, u64 count, address narrow, address wide) on cpu {
    u64 narrow_used = 0u64;
    u64 wide_used = 0u64;
    u64 index = 0u64;
    while (index < count) {
        address row = unsafe_address(address_bits(rows) + index * size_of<TilePlacement>());
        TilePlacement item = unsafe_position<TilePlacement>(row);
        address plane = narrow;
        u64 used = narrow_used;
        if (item.plane != 0u64) {
            plane = wide;
            used = wide_used;
        }
        u64 slot = 0u64;
        u64 next = 0u64;
        while (next < used && next < slot + item.width) {
            if (unsafe_position<i64>(tile_end(plane, next)) >= item.begin) {
                slot = next + 1u64;
            }
            next += 1u64;
        }
        next = slot;
        while (next < slot + item.width) {
            unsafe_position<i64>(tile_end(plane, next)) = item.end;
            next += 1u64;
        }
        used = max(used, next);
        if (item.plane == 0u64) {
            narrow_used = used;
        } else {
            wide_used = used;
        }
        item.slot = slot;
        unsafe_position<TilePlacement>(row) = item;
        index += 1u64;
    }
}

address stage_write_at(address writes, u64 index) on cpu {
    return unsafe_address(address_bits(writes) + index * size_of<StageWrite>());
}

u64 coalesce_stage_writes(address writes, u64 count) on cpu {
    u64 used = 0u64;
    u64 index = 0u64;
    while (index < count) {
        StageWrite next = unsafe_position<StageWrite>(stage_write_at(writes, index));
        bool merged = false;
        if (used != 0u64) {
            address tail = stage_write_at(writes, used - 1u64);
            StageWrite prior = unsafe_position<StageWrite>(tail);
            if (prior.binding == next.binding && next.start <= prior.end) {
                prior.end = max(prior.end, next.end);
                unsafe_position<StageWrite>(tail) = prior;
                merged = true;
            }
        }
        if (!merged) {
            unsafe_position<StageWrite>(stage_write_at(writes, used)) = next;
            used += 1u64;
        }
        index += 1u64;
    }
    return used;
}

bool overwrites_stage_guarantee(address writes, u64 count, StageGuarantee guarantee) on cpu {
    u64 begin = 0u64;
    u64 end = count;
    while (begin < end) {
        u64 middle = begin + (end - begin) / 2u64;
        StageWrite write = unsafe_position<StageWrite>(stage_write_at(writes, middle));
        if (write.binding < guarantee.binding
            || (write.binding == guarantee.binding && u64(write.end) <= guarantee.start)) {
            begin = middle + 1u64;
        } else {
            end = middle;
        }
    }
    if (begin == count) { return false; }
    StageWrite write = unsafe_position<StageWrite>(stage_write_at(writes, begin));
    return write.binding == guarantee.binding && u64(write.start) < guarantee.end;
}

extern "C" void retain_stage_guarantees(address writes, u64 write_count,
    address guarantees, u64 guarantee_count) on cpu {
    u64 count = coalesce_stage_writes(writes, write_count);
    u64 index = 0u64;
    while (index < guarantee_count) {
        address row = unsafe_address(address_bits(guarantees) + index * size_of<StageGuarantee>());
        StageGuarantee guarantee = unsafe_position<StageGuarantee>(row);
        guarantee.retained = u64(!overwrites_stage_guarantee(writes, count, guarantee));
        unsafe_position<StageGuarantee>(row) = guarantee;
        index += 1u64;
    }
}

address local_value_at(address values, u64 index) on cpu {
    return unsafe_address(address_bits(values) + index * size_of<LocalValue>());
}

extern "C" u64 intersect_local_values(address left, u64 left_count,
    address right, u64 right_count) on cpu {
    u64 kept = 0u64;
    u64 a = 0u64;
    u64 b = 0u64;
    while (a < left_count && b < right_count) {
        LocalValue lhs = unsafe_position<LocalValue>(local_value_at(left, a));
        LocalValue rhs = unsafe_position<LocalValue>(local_value_at(right, b));
        if (lhs.slot == rhs.slot && lhs.value == rhs.value) {
            unsafe_position<LocalValue>(local_value_at(left, kept)) = lhs;
            kept += 1u64;
        }
        if (lhs.slot <= rhs.slot) { a += 1u64; }
        if (rhs.slot <= lhs.slot) { b += 1u64; }
    }
    return kept;
}

extern "C" void private_reference(LiveReferenceKind kind,address live,u64 slot,
    cfn<void,LiveSetAction,address,address,u64> change) on cpu requires nonzero(change) {
    LiveSetAction action;
    if (kind==LiveReferenceKind::Result) { action=LiveSetAction::RemoveRegister; }
    else if (kind==LiveReferenceKind::Operand) { action=LiveSetAction::InsertRegister; }
    else { action=LiveSetAction::InsertLocal; }
    change(action,live,unsafe_address(0u64),slot);
}

extern "C" void private_loop(FusionBlock condition,u64 test,FusionBlock body,
    address after,address break_live,address continue_live,address exit_live,
    address result,address empty,LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    // Pointwise gen/kill transfers close the backedge from an empty header.
    // after may be result: it remains borrowed until the final swap.
    address need=host.clone_set(empty);
    private_before(body,need,after,empty,exit_live,empty,host);
    host.change(LiveSetAction::Union,need,after,0u64);
    host.change(LiveSetAction::InsertRegister,need,unsafe_address(0u64),test);
    private_before(condition,need,break_live,continue_live,exit_live,empty,host);
    host.change(LiveSetAction::Swap,result,need,0u64);
    host.drop_set(need);
}

extern "C" void private_before(FusionBlock code,address live,address break_live,
    address continue_live,address exit_live,address empty,LiveHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.references)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.change) {
    u64 index=code.count;
    while (index!=0u64) {
        index-=1u64;
        LiveNode node=host.read_node(code,index);
        if (node.kind==LiveKind::If) {
            address first=host.clone_set(live);
            private_before(node.first,first,break_live,continue_live,exit_live,empty,host);
            private_before(node.second,live,break_live,continue_live,exit_live,empty,host);
            host.change(LiveSetAction::Union,live,first,0u64);
            host.drop_set(first);
            host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),node.first_slot);
        } else if (node.kind==LiveKind::Loop) {
            private_loop(node.first,node.first_slot,node.second,live,break_live,continue_live,exit_live,live,empty,host);
        } else if (node.kind==LiveKind::Scope) {
            address after=host.clone_set(live);
            private_before(node.first,live,break_live,continue_live,after,empty,host);
            host.drop_set(after);
        } else if (node.kind==LiveKind::Exit) {
            host.change(LiveSetAction::Copy,live,exit_live,0u64);
        } else if (node.kind==LiveKind::Break) {
            host.change(LiveSetAction::Copy,live,break_live,0u64);
        } else if (node.kind==LiveKind::Continue) {
            host.change(LiveSetAction::Copy,live,continue_live,0u64);
        } else if (node.kind==LiveKind::LocalRead) {
            host.change(LiveSetAction::RemoveRegister,live,unsafe_address(0u64),node.first_slot);
            host.change(LiveSetAction::InsertLocal,live,unsafe_address(0u64),node.second_slot);
        } else if (node.kind==LiveKind::LocalWrite) {
            host.change(LiveSetAction::RemoveLocal,live,unsafe_address(0u64),node.first_slot);
            host.change(LiveSetAction::InsertRegister,live,unsafe_address(0u64),node.second_slot);
        } else {
            host.references(node.instruction,live);
        }
    }
}

extern "C" u32 dce_result(inout DceResults results,address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    results.present=1;
    results.needed=contains(needed,0,reg);
    return results.needed==0 ? 1 : 0;
}
extern "C" u32 dce_result_needed(address needed,u64 reg,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    return contains(needed,0,reg);
}
bool dce_any_local(address needed,u64 first,u64 length,
    cfn<u32,address,u32,u64> contains) on cpu requires nonzero(contains) {
    u64 index=0u64;
    while(index<length){
        if(contains(needed,1,first+index)!=0){return true;}
        index+=1u64;
    }
    return false;
}
bool dce_scope_observable(FusionBlock code,address after,DceHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.read_block)
    requires nonzero(host.results) requires nonzero(host.contains) requires nonzero(host.change)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block)
    requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    u64 index=0u64;
    while(index<code.count){
        DceNode node=host.read_node(code,index);
        if(node.kind==DceKind::LocalWrite){
            if(host.contains(after,1,node.first_slot)!=0){return true;}
        } else if(node.kind==DceKind::IndexedWrite){
            if(dce_any_local(after,node.first_slot,node.second_slot,host.contains)){return true;}
        } else if(node.kind==DceKind::Scope){
            if(dce_scope_observable(node.first,after,host)){return true;}
        } else if(node.kind==DceKind::If){
            if(dce_scope_observable(node.first,after,host)||dce_scope_observable(node.second,after,host)){return true;}
        } else if((node.flags&2)!=0){return true;}
        else {
            DceResults results=host.results(node.instruction,after);
            if(results.needed!=0){return true;}
        }
        index+=1u64;
    }
    return false;
}

extern "C" bool eliminate_instruction(address instruction,address needed,
    address break_live,address continue_live,address exit_live,address empty,
    address context,DceHost host) on cpu
    requires nonzero(host.read_node) requires nonzero(host.read_block)
    requires nonzero(host.results) requires nonzero(host.contains) requires nonzero(host.change)
    requires nonzero(host.clone_set) requires nonzero(host.drop_set) requires nonzero(host.eliminate_block)
    requires nonzero(host.live_before) requires nonzero(host.loop_live) requires nonzero(host.record)
    requires nonzero(host.prune_division) requires nonzero(host.prune_collective) {
    DceNode node=host.read_node(FusionBlock(instruction,1u64),0u64);
    if(node.kind==DceKind::Collective){host.prune_collective(node.payload,needed);}
    if(node.kind==DceKind::Division){
        u32 retained=0;
        if((node.flags&4)!=0 && host.contains(needed,0,node.quotient)!=0){retained|=1;}
        if((node.flags&8)!=0 && host.contains(needed,0,node.remainder)!=0){retained|=2;}
        if(retained==0){
            host.record(context,instruction,DceRemoval(DceRemovalKind::Division,0u64,0u64,0u64,0u64,node.payload));
            return false;
        }
        if(retained!=((node.flags>>2)&3)){
            DceDivisionChoice choice;
            if(retained==1){choice=DceDivisionChoice::Quotient;}
            else if(retained==2){choice=DceDivisionChoice::Remainder;}
            else {choice=DceDivisionChoice::Both;}
            host.prune_division(instruction,node.payload,choice,node.quotient,node.remainder);
        }
    }
    if((node.flags&1)==0){
        DceResults results=host.results(instruction,needed);
        if(results.present!=0 && results.needed==0){
            host.record(context,instruction,DceRemoval(DceRemovalKind::Values,0u64,0u64,0u64,0u64,unsafe_address(0u64)));
            return false;
        }
    }
    if(node.kind==DceKind::LocalWrite && host.contains(needed,1,node.first_slot)==0){
        host.record(context,instruction,DceRemoval(DceRemovalKind::DirectWrite,node.value,0u64,0u64,0u64,unsafe_address(0u64)));
        return false;
    }
    if(node.kind==DceKind::IndexedWrite && !dce_any_local(needed,node.first_slot,node.second_slot,host.contains)){
        host.record(context,instruction,DceRemoval(DceRemovalKind::IndexedWrite,node.value,node.index,node.second_slot,node.word,unsafe_address(0u64)));
        return false;
    }
    if(node.kind==DceKind::Scope){
        address after=host.clone_set(needed);
        host.eliminate_block(node.first_code,needed,break_live,continue_live,after,context);
        bool observable=dce_scope_observable(host.read_block(node.first_code),after,host);
        if(!observable){host.change(LiveSetAction::Copy,needed,after,0u64);}
        host.drop_set(after);
        if(!observable){return false;}
    } else if(node.kind==DceKind::If){
        address first=host.clone_set(needed);
        host.eliminate_block(node.first_code,first,break_live,continue_live,exit_live,context);
        host.eliminate_block(node.second_code,needed,break_live,continue_live,exit_live,context);
        FusionBlock yes=host.read_block(node.first_code);
        FusionBlock no=host.read_block(node.second_code);
        if(yes.count==0u64 && no.count==0u64){host.drop_set(first);return false;}
        host.change(LiveSetAction::Union,needed,first,0u64);
        host.drop_set(first);
        host.change(LiveSetAction::InsertRegister,needed,unsafe_address(0u64),node.first_slot);
    } else if(node.kind==DceKind::Loop){
        address header=host.loop_live(node.first,node.first_slot,node.second,needed,break_live,continue_live,exit_live);
        address before_body=host.clone_set(header);
        host.eliminate_block(node.second_code,before_body,needed,header,exit_live,context);
        host.change(LiveSetAction::Union,before_body,needed,0u64);
        host.change(LiveSetAction::InsertRegister,before_body,unsafe_address(0u64),node.first_slot);
        host.eliminate_block(node.first_code,before_body,break_live,continue_live,exit_live,context);
        host.change(LiveSetAction::Swap,needed,before_body,0u64);
        host.drop_set(before_body);
        host.drop_set(header);
    } else {
        host.live_before(FusionBlock(instruction,1u64),needed,break_live,continue_live,exit_live);
    }
    return true;
}

extern "C" u64 value_root(address values,u64 reg,cfn<VfOptional,address,VfRegMap,u64> get) on cpu requires nonzero(get) {
    u64 result=reg;
    VfOptional next=get(values,VfRegMap::Aliases,result);
    while(next.present!=0){result=next.value;next=get(values,VfRegMap::Aliases,result);}
    return result;
}
extern "C" u32 value_memory_keeps(u32 mode,u64 binding,u64 index,u32 word,u64 target,u64 at,u32 field) on cpu {
    if(mode==0){return binding!=target ? 1 : 0;}
    return binding!=target || (index==at && word!=field) ? 1 : 0;
}
extern "C" void value_alias(address values,u32 kind,u64 first,u64 length,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(kind==0){host.remove_range(values,first,first+length);}
    else {host.memory_filter(values,0,first,0u64,0);}
}
extern "C" void value_call_effect(address values,u32 kind,u64 first,u64 length,u32 writes,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(writes==0){return;}
    if(kind==0){host.remove_range(values,first,first+1u64);}
    else if(kind==1 || kind==4){host.memory_filter(values,0,first,0u64,0);}
    else if(kind==2){host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);}
    else {host.remove_range(values,first,first+length);}
}
void vf_merge(inout address left,address right,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(address_bits(right)!=0u64){
        if(address_bits(left)!=0u64){host.intersect(left,right);}
        else {left=right;}
    }
}
bool vf_resume(address values,inout VfFlow flow,VfFlow step,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    vf_merge(flow.exits,step.exits,host);
    bool continuing=address_bits(step.next)!=0u64;
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Locals);
    if(continuing){host.apply(values,step.next);}
    host.state(VfStateOp::Clear,values,unsafe_address(0u64),VfMap::Memory);
    return continuing;
}
address vf_child(address values,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    address child=host.state(VfStateOp::New,unsafe_address(0u64),unsafe_address(0u64),VfMap::Aliases);
    host.state(VfStateOp::Move,child,values,VfMap::Aliases);
    host.state(VfStateOp::Copy,child,values,VfMap::Expressions);
    host.state(VfStateOp::Move,child,values,VfMap::Locals);
    host.state(VfStateOp::Move,child,values,VfMap::Memory);
    return child;
}
extern "C" VfAction value_instruction(address instruction,address values,inout VfFlow flow,address context,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    host.remap(instruction,values);
    VfNode node=host.read(instruction,context);
    address none=unsafe_address(0u64);
    VfDivision empty=VfDivision(0u64,0u64,0u64,0,0);
    if(node.kind==VfKind::If || node.kind==VfKind::Loop || node.kind==VfKind::Scope || node.kind==VfKind::Exit || node.kind==VfKind::Raise ||
        node.kind==VfKind::Publish || node.kind==VfKind::Collective || node.kind==VfKind::Barrier || node.kind==VfKind::Break || node.kind==VfKind::Continue){
        host.division(context,VfDivisionOp::Clear,0u64,0u64,0u64,empty);
    }
    if(node.kind==VfKind::Division){
        u32 retained=0;
        u32 paired=node.flags==3 ? 1 : 0;
        if((node.flags&1)!=0){
            VfExpression key=VfExpression(10,0,node.first,node.second,node.third,node.arguments,0u64);
            VfOptional old=host.expression(values,key,node.dst);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,node.dst,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Quotient,paired));
            } else {retained|=1;}
        }
        if((node.flags&2)!=0){
            VfExpression key=VfExpression(10,1,node.first,node.second,node.third,node.arguments,0u64);
            VfOptional old=host.expression(values,key,node.extra);
            if(old.present!=0){
                host.put(values,VfRegMap::Aliases,node.extra,old.value);
                host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Remainder,paired));
            } else {retained|=2;}
        }
        if(retained==0){return VfAction::Discard;}
        VfDivisionChoice choice;
        if(retained==1){choice=VfDivisionChoice::Quotient;}
        else if(retained==2){choice=VfDivisionChoice::Remainder;}
        else {choice=VfDivisionChoice::Both;}
        host.rewrite_division(node.division_result,choice,node.dst,node.extra);
        VfDivision previous=host.division(context,VfDivisionOp::Take,node.first,node.second,node.third,empty);
        if(previous.present!=0 && ((previous.mask==1 && retained==2)||(previous.mask==2 && retained==1))){
            u64 quotient=retained==1 ? node.dst : previous.quotient;
            u64 remainder=retained==2 ? node.extra : previous.remainder;
            host.pair(context,instruction,previous,node.divisor,node.first,quotient,remainder);
            return VfAction::Discard;
        }
        if(retained!=3){
            VfDivision next=VfDivision(host.position(context),node.dst,node.extra,retained,1);
            host.division(context,VfDivisionOp::Put,node.first,node.second,node.third,next);
        }
    }
    VfExpression expression=VfExpression(0,node.operation,node.first,node.second,node.third,node.arguments,0u64);
    if(node.kind==VfKind::Constant){expression.kind=1;}
    else if(node.kind==VfKind::Index){expression.kind=2;}
    else if(node.kind==VfKind::Uniform){expression.kind=3;}
    else if(node.kind==VfKind::BufferStride){expression.kind=4;}
    else if(node.kind==VfKind::Unary){expression.kind=5;}
    else if(node.kind==VfKind::Binary){expression.kind=6;}
    else if(node.kind==VfKind::Cast){expression.kind=7;}
    else if(node.kind==VfKind::Math){expression.kind=8;expression.count=node.count;}
    else if(node.kind==VfKind::Select){expression.kind=9;}
    if(expression.kind!=0){
        VfOptional old=host.expression(values,expression,node.dst);
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.dst,old.value);
            host.note(context,old.value,instruction,VfReuse(none,VfReuseKind::Expression,VfRole::Value,0));
            return VfAction::Discard;
        }
    }
    bool continuing=true;
    if(node.kind==VfKind::Scope){
        address scoped=vf_child(values,host);
        VfFlow scope=host.block(node.first_code,scoped,context);
        host.state(VfStateOp::Move,values,scoped,VfMap::Aliases);
        vf_merge(scope.next,scope.exits,host);
        scope.exits=none;
        continuing=vf_resume(values,flow,scope,host);
        host.state(VfStateOp::Drop,scoped,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Raise){continuing=false;}
    else if(node.kind==VfKind::Exit){vf_merge(flow.exits,host.snapshot(values),host);continuing=false;}
    else if(node.kind==VfKind::Break || node.kind==VfKind::Continue){continuing=false;}
    else if(node.kind==VfKind::Atomic){
        if(node.flags!=0){host.aliases(values,node.payload);}
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
    } else if(node.kind==VfKind::Append || node.kind==VfKind::Barrier){host.state(VfStateOp::Clear,values,none,VfMap::Memory);}
    else if(node.kind==VfKind::LocalRead){
        VfOptional old=host.get(values,VfRegMap::Locals,node.first);
        if(old.present!=0){
            u64 value=value_root(values,old.value,host.get);
            host.put(values,VfRegMap::Aliases,node.dst,value);
            host.note(context,value,instruction,VfReuse(none,VfReuseKind::LocalRead,VfRole::Value,0));
        }
        host.put(values,VfRegMap::Locals,node.first,node.dst);
    } else if(node.kind==VfKind::LocalWrite){host.put(values,VfRegMap::Locals,node.first,node.second);}
    else if(node.kind==VfKind::LocalIndexedWrite){host.remove_range(values,node.first,node.first+node.second);}
    else if(node.kind==VfKind::IndexedWrite){host.aliases(values,node.payload);}
    else if(node.kind==VfKind::Load){
        VfMemory old=host.memory_get(values,node.first,node.second,u32(node.third));
        if(old.present!=0){
            host.put(values,VfRegMap::Aliases,node.dst,old.value);
            VfReuseKind kind;
            if(address_bits(old.origin)!=0u64){kind=VfReuseKind::IndexedStoreRead;}
            else {kind=VfReuseKind::IndexedRead;}
            host.note(context,old.value,instruction,VfReuse(old.origin,kind,VfRole::Value,0));
            return VfAction::Discard;
        }
        host.memory_put(values,node.first,node.second,u32(node.third),node.dst,none);
    } else if(node.kind==VfKind::Store){
        host.memory_filter(values,1,node.first,node.second,u32(node.third));
        host.memory_put(values,node.first,node.second,u32(node.third),node.extra,instruction);
    } else if(node.kind==VfKind::If){
        address yes=host.state(VfStateOp::Clone,none,values,VfMap::Aliases);
        address no=vf_child(values,host);
        VfFlow branches=host.block(node.first_code,yes,context);
        VfFlow other=host.block(node.second_code,no,context);
        vf_merge(branches.next,other.next,host);
        vf_merge(branches.exits,other.exits,host);
        host.state(VfStateOp::Move,values,yes,VfMap::Aliases);
        host.state(VfStateOp::Extend,values,no,VfMap::Aliases);
        continuing=vf_resume(values,flow,branches,host);
        host.state(VfStateOp::Drop,yes,none,VfMap::Aliases);
        host.state(VfStateOp::Drop,no,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Loop){
        host.written(values,node.first_code,node.second_code);
        address after=host.snapshot(values);
        host.state(VfStateOp::Clear,values,none,VfMap::Memory);
        address cycle=vf_child(values,host);
        VfFlow condition=host.block(node.first_code,cycle,context);
        unsafe_position<u64>(node.arguments)=value_root(cycle,unsafe_position<u64>(node.arguments),host.get);
        VfFlow loop_flow=VfFlow(none,condition.exits);
        if(address_bits(condition.next)!=0u64){
            host.drop_snapshot(condition.next);
            loop_flow.next=after;
            VfFlow body=host.block(node.second_code,cycle,context);
            if(address_bits(body.next)!=0u64){host.drop_snapshot(body.next);}
            vf_merge(loop_flow.exits,body.exits,host);
        } else {host.drop_snapshot(after);}
        host.state(VfStateOp::Move,values,cycle,VfMap::Aliases);
        continuing=vf_resume(values,flow,loop_flow,host);
        host.state(VfStateOp::Drop,cycle,none,VfMap::Aliases);
    } else if(node.kind==VfKind::Call){host.call(values,node.payload);}
    return continuing ? VfAction::Keep : VfAction::Stop;
}
extern "C" VfFlow value_finish(address values,VfFlow flow,u32 continuing,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    address next=flow.next;
    if(continuing!=0){next=host.snapshot(values);}
    return VfFlow(next,flow.exits);
}
extern "C" void value_discard_flow(VfFlow flow,VfHost host) on cpu 
    requires nonzero(host.read) requires nonzero(host.remap) requires nonzero(host.block) requires nonzero(host.state)
    requires nonzero(host.get) requires nonzero(host.put) requires nonzero(host.remove_range) requires nonzero(host.snapshot)
    requires nonzero(host.intersect) requires nonzero(host.drop_snapshot) requires nonzero(host.apply) requires nonzero(host.expression)
    requires nonzero(host.division) requires nonzero(host.rewrite_division) requires nonzero(host.pair) requires nonzero(host.position)
    requires nonzero(host.note) requires nonzero(host.memory_get) requires nonzero(host.memory_put) requires nonzero(host.memory_filter)
    requires nonzero(host.aliases) requires nonzero(host.written) requires nonzero(host.call) {
    if(address_bits(flow.next)!=0u64){host.drop_snapshot(flow.next);}
    if(address_bits(flow.exits)!=0u64){host.drop_snapshot(flow.exits);}
}
