module bootstrap.coloring;
import bootstrap.names;

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

extern "C" u32 color(address context, u64 count, address order, u32 mode,
    cfn<ColorNode,address,u64> node,
    cfn<u64,address,u64,bool,u64> next,
    cfn<ColorPlace,address,u64> get,
    cfn<void,address,u64,bool,u32> put) on cpu
    requires nonzero(node) requires nonzero(next) requires nonzero(get) requires nonzero(put) {
    u64 pending = 0u64;
    u64 id = 0u64;
    while (id < count) {
        if (get(context, id).kind == 0) {
            ColorNode facts = node(context, id);
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
        ColorNode facts = node(context, id);
        u32 bank = 2;
        u32 allowed = mode == 1 ? 0x3e0 : 0xfe0;
        if ((facts.flags & 2u64) != 0u64) { allowed &= 0xe0; }
        if ((facts.flags & 1u64) != 0u64) {
            bank = 3;
            allowed = (facts.flags & 2u64) != 0u64 ? 0xffc0 : 0xfffc;
        }
        if (mode == 2) { bank = 3; allowed = 0xfff0; }

        u32 used = 0;
        u64 occupied = 0u64;
        u64 neighbor = next(context, id, false, count);
        while (neighbor != count) {
            ColorPlace place = get(context, neighbor);
            if (place.kind == bank) { used |= 1 << u32(place.value); }
            if (place.kind == 4) {
                // Only earlier nonfixed nodes can have stack colors. Their
                // order rows are dead; the unprocessed suffix remains intact.
                unsafe_position<u32>(spill_at(order, occupied)) = u32(place.value);
                occupied += 1u64;
            }
            neighbor = next(context, id, false, neighbor);
        }
        u32 available = allowed & ~used;
        u32 chosen = 16;
        neighbor = next(context, id, true, count);
        while (neighbor != count) {
            ColorPlace place = get(context, neighbor);
            if (place.kind == bank && (available & (1 << u32(place.value))) != 0) {
                chosen = u32(place.value);
                break;
            }
            neighbor = next(context, id, true, neighbor);
        }
        if (chosen == 16) {
            u32 candidate = 0;
            while (candidate < 16) {
                u32 reg = candidate;
                if (bank == 2) {
                    if (candidate >= 7) { break; }
                    reg = candidate < 4 ? candidate + 8 : (candidate < 6 ? candidate + 2 : 5);
                }
                if ((available & (1 << reg)) != 0) { chosen = reg; break; }
                candidate += 1;
            }
        }
        if (chosen != 16) {
            put(context, id, false, chosen);
        } else {
            sort_rows<u32>(order, occupied, address(), c_function(spill_before));
            u32 slot = 0;
            u32 stride = mode == 2 ? 16 : 1;
            while (spill_used(order, occupied, slot * stride)) { slot += 1; }
            if (spills < slot + 1) { spills = slot + 1; }
            put(context, id, true, slot);
        }
        at += 1u64;
    }
    return spills;
}
