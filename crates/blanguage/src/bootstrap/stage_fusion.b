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
