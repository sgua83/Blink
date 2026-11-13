module bootstrap.coloring;
import bootstrap.names;

const u32 UNSEEN = 0;
const u32 IMMEDIATE = 1;
const u32 GPR = 2;
const u32 XMM = 3;
const u32 STACK = 4;
const u32 VARYING = 5;

address place_at(address places, u64 index) on cpu {
    return unsafe_address(address_bits(places) + index * size_of<ColorPlace>());
}

address order_at(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * size_of<array<u64,3>>());
}
address spill_at(address rows, u64 index) on cpu {
    return unsafe_address(address_bits(rows) + index * 4u64);
}
extern "C" bool priority_before(address left, address right, address context) on cpu {
    array<u64,3> a = unsafe_position<array<u64,3>>(left);
    array<u64,3> b = unsafe_position<array<u64,3>>(right);
    if (a[0] != b[0]) { return a[0] > b[0]; }
    if (a[1] != b[1]) { return a[1] > b[1]; }
    return a[2] < b[2];
}
extern "C" bool spill_before(address left, address right, address context) on cpu {
    return unsafe_position<u32>(left) < unsafe_position<u32>(right);
}
bool spill_used(address rows, u64 count, u32 value) on cpu {
    u64 low = 0u64;
    u64 high = count;
    while (low < high) {
        u64 middle = low + (high - low) / 2u64;
        u32 at = unsafe_position<u32>(spill_at(rows, middle));
        if (at < value) { low = middle + 1u64; } else { high = middle; }
    }
    return low < count && unsafe_position<u32>(spill_at(rows, low)) == value;
}

// After coloring, every order row is dead. Reuse that memory first for the
// maximum width of each spill color, then for its final stack offset.
u64 layout_spills(address places, u64 count, address scratch, u64 spills, u64 base) on cpu {
    u64 end = base;
    u64 slot = 0u64;
    while (slot < spills) {
        unsafe_position<u64>(order_at(scratch, slot)) = 0u64;
        slot += 1u64;
    }
    u64 id = 0u64;
    while (id < count) {
        ColorPlace place = unsafe_position<ColorPlace>(place_at(places, id));
        if (place.kind == STACK) {
            address row = order_at(scratch, place.value);
            u64 width = u64(place.width);
            if (unsafe_position<u64>(row) < width) {
                unsafe_position<u64>(row) = width;
            }
        }
        id += 1u64;
    }
    slot = 0u64;
    while (slot < spills) {
        address row = order_at(scratch, slot);
        u64 width = unsafe_position<u64>(row);
        if (width != 0u64) {
            end = ((end + width - 1u64) / width) * width;
        }
        unsafe_position<u64>(row) = end;
        end += width;
        slot += 1u64;
    }
    id = 0u64;
    while (id < count) {
        address row = place_at(places, id);
        if (unsafe_position<ColorPlace>(row).kind == STACK) {
            slot = unsafe_position<ColorPlace>(row).value;
            unsafe_position<ColorPlace>(row).value = unsafe_position<u64>(order_at(scratch, slot));
        }
        id += 1u64;
    }
    return end;
}

extern "C" u64 color(address graph, u64 count, address places, address order, u32 mode, u64 base,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next) on cpu
    requires nonzero(node) requires nonzero(next) {
    u64 pending = 0u64;
    u64 id = 0u64;
    while (id < count) {
        if (unsafe_position<ColorPlace>(place_at(places, id)).kind != IMMEDIATE) {
            ColorNode facts = node(graph, id);
            unsafe_position<array<u64,3>>(order_at(order, pending)) = array<u64,3>(facts.weight, facts.degree, id);
            pending += 1u64;
        }
        id += 1u64;
    }
    sort_rows<array<u64,3>>(order, pending, address(), c_function(priority_before));
    u32 spills = 0;
    u64 at = 0u64;
    while (at < pending) {
        id = unsafe_position<array<u64,3>>(order_at(order, at))[2];
        ColorNode facts = node(graph, id);
        address destination = place_at(places, id);
        u32 bank = GPR;
        u32 allowed = mode == 1 ? 0x3e0 : 0xfe0;
        if (facts.across_call != 0u64) { allowed &= 0xe0; }
        if (unsafe_position<ColorPlace>(destination).floating != 0) {
            bank = XMM;
            allowed = facts.across_call != 0u64 ? 0xffc0 : 0xfffc;
        }
        if (mode == 2) { bank = XMM; allowed = 0xfff0; }

        u32 used = 0;
        u64 occupied = 0u64;
        u64 neighbor = next(graph, id, false, count);
        while (neighbor != count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank) { used |= 1 << u32(place.value); }
            if (place.kind == STACK) {
                // Only earlier nonfixed nodes can have stack colors. Their
                // order rows are dead; the unprocessed suffix remains intact.
                unsafe_position<u32>(spill_at(order, occupied)) = u32(place.value);
                occupied += 1u64;
            }
            neighbor = next(graph, id, false, neighbor);
        }
        u32 available = allowed & ~used;
        u32 chosen = 16;
        neighbor = next(graph, id, true, count);
        while (neighbor != count) {
            ColorPlace place = unsafe_position<ColorPlace>(place_at(places, neighbor));
            if (place.kind == bank && (available & (1 << u32(place.value))) != 0) {
                chosen = u32(place.value);
                break;
            }
            neighbor = next(graph, id, true, neighbor);
        }
        if (chosen == 16) {
            u32 candidate = 0;
            while (candidate < 16) {
                u32 reg = candidate;
                if (bank == GPR) {
                    if (candidate >= 7) { break; }
                    reg = candidate < 4 ? candidate + 8 : (candidate < 6 ? candidate + 2 : 5);
                }
                if ((available & (1 << reg)) != 0) { chosen = reg; break; }
                candidate += 1;
            }
        }
        if (chosen != 16) {
            unsafe_position<ColorPlace>(destination).value = u64(chosen);
            unsafe_position<ColorPlace>(destination).kind = bank;
        } else {
            sort_rows<u32>(order, occupied, address(), c_function(spill_before));
            u32 slot = 0;
            while (spill_used(order, occupied, slot)) { slot += 1; }
            if (spills < slot + 1) { spills = slot + 1; }
            unsafe_position<ColorPlace>(destination).value = u64(slot);
            unsafe_position<ColorPlace>(destination).kind = STACK;
        }
        at += 1u64;
    }
    if (spills == 0) { return base; }
    return layout_spills(places, count, order, u64(spills), base);
}

extern "C" u64 place_private(address places, u64 count, u64 data_end, u64 bytes, u64 alignment)
    on cpu requires nonzero(alignment) {
    u64 base = ((data_end + alignment - 1u64) / alignment) * alignment;
    u64 index = 0u64;
    while (index < count) {
        address at = place_at(places, index);
        unsafe_position<ColorPlace>(at).value += base;
        unsafe_position<ColorPlace>(at).kind = STACK;
        index += 1u64;
    }
    return base + bytes;
}

extern "C" void record_definition(address place, u32 literal, u64 bits) on cpu {
    u32 kind = unsafe_position<ColorPlace>(place).kind;
    if (literal != 0 && (kind == UNSEEN
        || (kind == IMMEDIATE && unsafe_position<ColorPlace>(place).value == bits))) {
        unsafe_position<ColorPlace>(place).kind = IMMEDIATE;
        unsafe_position<ColorPlace>(place).value = bits;
    } else {
        unsafe_position<ColorPlace>(place).kind = VARYING;
    }
}
