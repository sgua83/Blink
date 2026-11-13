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
        if (node.kind == SelectionKind::LocalWrite) {
            address at = selection_slot(input.slots, node.first);
            SelectionSlot slot = unsafe_position<SelectionSlot>(at);
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
    while (index < input.yes.count) {
        if (!selection_allowed(input, read(context, input.yes, index))) { return 0u64; }
        index += 1u64;
    }
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
