module bootstrap.proof_facts;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct FactNode { u64 left; u64 right; u64 parent; u64 height; };

// One-based node indices survive growth. The native host only lends typed
// payloads, which occupy the aligned tail of each densely packed tree node.
address offset(address base, u64 bytes) on cpu {
    return unsafe_address(address_bits(base) + bytes);
}

u64 stride(u64 width) on cpu {
    return size_of<FactNode>() + ((width + 15u64) & ~15u64);
}

address node(FactTree tree, u64 index, u64 width) on cpu {
    return offset(tree.records, (index - 1u64) * stride(width));
}

address payload(FactTree tree, u64 index, u64 width) on cpu {
    return offset(node(tree,index,width),size_of<FactNode>());
}

u64 height(FactTree tree, u64 index, u64 width) on cpu {
    if (index == 0u64) { return 0u64; }
    return unsafe_position<FactNode>(node(tree,index,width)).height;
}

void update_height(FactTree tree, u64 index, u64 width) on cpu {
    address entry = node(tree,index,width);
    FactNode value = unsafe_position<FactNode>(entry);
    unsafe_position<FactNode>(entry).height =
        1u64 + max(height(tree,value.left,width),height(tree,value.right,width));
}

void set_parent(FactTree tree, u64 index, u64 parent, u64 width) on cpu {
    if (index != 0u64) { unsafe_position<FactNode>(node(tree,index,width)).parent = parent; }
}

void replace_child(address owner, FactTree tree, u64 parent, u64 old, u64 fresh, u64 width) on cpu {
    if (parent == 0u64) {
        unsafe_position<FactTree>(owner).root = fresh;
    } else {
        address entry = node(tree,parent,width);
        if (unsafe_position<FactNode>(entry).left == old) {
            unsafe_position<FactNode>(entry).left = fresh;
        } else {
            unsafe_position<FactNode>(entry).right = fresh;
        }
    }
    set_parent(tree,fresh,parent,width);
}

u64 rotate(address owner, FactTree tree, u64 low, u64 width, bool left) on cpu {
    address low_node = node(tree,low,width);
    FactNode old = unsafe_position<FactNode>(low_node);
    u64 high = left ? old.right : old.left;
    address high_node = node(tree,high,width);
    FactNode elevated = unsafe_position<FactNode>(high_node);
    u64 middle = left ? elevated.left : elevated.right;
    replace_child(owner,tree,old.parent,low,high,width);
    if (left) {
        unsafe_position<FactNode>(low_node).right = middle;
        unsafe_position<FactNode>(high_node).left = low;
    } else {
        unsafe_position<FactNode>(low_node).left = middle;
        unsafe_position<FactNode>(high_node).right = low;
    }
    set_parent(tree,middle,low,width);
    set_parent(tree,low,high,width);
    update_height(tree,low,width);
    update_height(tree,high,width);
    return high;
}

void rebalance(address owner, u64 start, u64 width) on cpu {
    u64 current = start;
    FactTree tree = unsafe_position<FactTree>(owner);
    while (current != 0u64) {
        FactNode value = unsafe_position<FactNode>(node(tree,current,width));
        u64 left = height(tree,value.left,width);
        u64 right = height(tree,value.right,width);
        u64 top = current;
        if (left > right + 1u64) {
            FactNode child = unsafe_position<FactNode>(node(tree,value.left,width));
            if (height(tree,child.right,width) > height(tree,child.left,width)) {
                rotate(owner,tree,value.left,width,true);
            }
            top = rotate(owner,tree,current,width,false);
        } else if (right > left + 1u64) {
            FactNode child = unsafe_position<FactNode>(node(tree,value.right,width));
            if (height(tree,child.left,width) > height(tree,child.right,width)) {
                rotate(owner,tree,value.right,width,false);
            }
            top = rotate(owner,tree,current,width,true);
        } else {
            unsafe_position<FactNode>(node(tree,current,width)).height = 1u64 + max(left,right);
        }
        current = unsafe_position<FactNode>(node(tree,top,width)).parent;
    }
}

u64 extreme(FactTree tree, u64 start, u64 width, bool reverse) on cpu {
    u64 current = start;
    if (current != 0u64) {
        while (true) {
            FactNode value = unsafe_position<FactNode>(node(tree,current,width));
            u64 child = reverse ? value.right : value.left;
            if (child == 0u64) { break; }
            current = child;
        }
    }
    return current;
}

u64 neighbor(FactTree tree, u64 start, u64 width, bool reverse) on cpu {
    u64 current = start;
    FactNode value = unsafe_position<FactNode>(node(tree,current,width));
    u64 child = reverse ? value.left : value.right;
    if (child != 0u64) { return extreme(tree,child,width,reverse); }
    u64 parent = value.parent;
    while (parent != 0u64) {
        FactNode higher = unsafe_position<FactNode>(node(tree,parent,width));
        if ((reverse ? higher.right : higher.left) == current) { break; }
        current = parent;
        parent = higher.parent;
    }
    return parent;
}

struct Search { u64 exact; u64 parent; i32 order; };

Search locate(FactTree tree, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    Search result = Search(0u64,0u64,i32(0));
    u64 current = tree.root;
    while (current != 0u64) {
        result.order = compare(key,payload(tree,current,width));
        if (result.order == 0) {
            result.exact = current;
            break;
        }
        result.parent = current;
        FactNode value = unsafe_position<FactNode>(node(tree,current,width));
        current = result.order < 0 ? value.left : value.right;
    }
    return result;
}

void grow(address owner, FactTree tree, u64 width) on cpu {
    u64 bytes = stride(width);
    if (bytes == 0u64) { abort(); return; }
    u64 limit = 9223372036854775807u64 / bytes;
    if (tree.count == limit) { abort(); return; }
    u64 capacity = tree.capacity == 0u64 ? 4u64 : tree.count + min(tree.count,limit - tree.count);
    address records = tree.capacity == 0u64
        ? HeapAlloc(GetProcessHeap(),0,capacity * bytes)
        : HeapReAlloc(GetProcessHeap(),0,tree.records,capacity * bytes);
    if (records == address()) { abort(); return; }
    unsafe_position<FactTree>(owner).records = records;
    unsafe_position<FactTree>(owner).capacity = capacity;
}

extern "C" void fact_tree_clear(address owner) on cpu {
    address records = unsafe_position<FactTree>(owner).records;
    if (records != address()) { HeapFree(GetProcessHeap(),0,records); }
    unsafe_position<FactTree>(owner) = FactTree(address(),0u64,0u64,0u64);
}

extern "C" void fact_tree_copy(address original, address output, u64 width) on cpu {
    FactTree tree = unsafe_position<FactTree>(original);
    if (tree.count != 0u64) {
        u64 bytes = tree.count * stride(width);
        address records = HeapAlloc(GetProcessHeap(),0,bytes);
        if (records == address()) { abort(); return; }
        RtlMoveMemory(records,tree.records,bytes);
        unsafe_position<FactTree>(output) = FactTree(records,tree.root,tree.count,tree.count);
    }
}

extern "C" u32 fact_tree_insert(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    FactTree tree = unsafe_position<FactTree>(owner);
    Search place = locate(tree,key,width,compare);
    if (place.exact != 0u64) { return 0; }
    if (tree.capacity == tree.count) {
        grow(owner,tree,width);
        tree = unsafe_position<FactTree>(owner);
    }
    u64 fresh = tree.count + 1u64;
    unsafe_position<FactNode>(node(tree,fresh,width)) = FactNode(0u64,0u64,place.parent,1u64);
    RtlMoveMemory(payload(tree,fresh,width),key,width);
    if (place.parent == 0u64) {
        unsafe_position<FactTree>(owner).root = fresh;
    } else {
        address parent = node(tree,place.parent,width);
        if (place.order < 0) {
            unsafe_position<FactNode>(parent).left = fresh;
        } else {
            unsafe_position<FactNode>(parent).right = fresh;
        }
        rebalance(owner,place.parent,width);
    }
    unsafe_position<FactTree>(owner).count = fresh;
    return 1;
}

// Erase the current key and return its successor's new index for retain.
// Keeping live nodes packed avoids a separate free list and copies only the
// live allocation when a query first needs to own borrowed facts.
u64 erase(address owner, FactTree tree, u64 erased, u64 width) on cpu {
    u64 current = erased;
    u64 next = neighbor(tree,current,width,false);
    FactNode value = unsafe_position<FactNode>(node(tree,current,width));
    if (value.left != 0u64 && value.right != 0u64) {
        RtlMoveMemory(payload(tree,current,width),payload(tree,next,width),width);
        u64 successor = next;
        next = current;
        current = successor;
        value = unsafe_position<FactNode>(node(tree,current,width));
    }
    u64 child = value.left == 0u64 ? value.right : value.left;
    replace_child(owner,tree,value.parent,current,child,width);
    rebalance(owner,value.parent,width);
    if (current != tree.count) {
        FactNode moved = unsafe_position<FactNode>(node(tree,tree.count,width));
        RtlMoveMemory(node(tree,current,width),node(tree,tree.count,width),stride(width));
        replace_child(owner,tree,moved.parent,tree.count,current,width);
        set_parent(tree,moved.left,current,width);
        set_parent(tree,moved.right,current,width);
        if (next == tree.count) { next = current; }
    }
    unsafe_position<FactTree>(owner).count = tree.count - 1u64;
    return next;
}

extern "C" u32 fact_tree_remove(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 index = locate(tree,key,width,compare).exact;
    if (index == 0u64) { return 0; }
    erase(owner,tree,index,width);
    return 1;
}

extern "C" u32 fact_tree_contains(address owner, address key, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    return u32(locate(unsafe_position<FactTree>(owner),key,width,compare).exact != 0u64);
}

extern "C" void fact_tree_retain(address owner, u64 width, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 index = extreme(tree,tree.root,width,false);
    while (index != 0u64) {
        if (keep(context,payload(tree,index,width)) == 0) {
            index = erase(owner,tree,index,width);
            tree = unsafe_position<FactTree>(owner);
        } else {
            index = neighbor(tree,index,width,false);
        }
    }
}

extern "C" FactCursor fact_tree_cursor(address owner, u64 width) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    return FactCursor(extreme(tree,tree.root,width,false),extreme(tree,tree.root,width,true));
}

u64 bound(FactTree tree, u64 width, FactBound limit, bool reverse, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    if (limit.key == address()) { return extreme(tree,tree.root,width,reverse); }
    u64 result = 0u64;
    u64 current = tree.root;
    while (current != 0u64) {
        i32 order = compare(payload(tree,current,width),limit.key);
        if (order == 0 && limit.exclusive == 0) { return current; }
        FactNode value = unsafe_position<FactNode>(node(tree,current,width));
        if (reverse ? order < 0 : order > 0) {
            result = current;
            current = reverse ? value.right : value.left;
        } else {
            current = reverse ? value.left : value.right;
        }
    }
    return result;
}

extern "C" FactCursor fact_tree_range(address owner, u64 width, FactBound start, FactBound end,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare) {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 first = bound(tree,width,start,false,compare);
    u64 last = bound(tree,width,end,true,compare);
    if (first == 0u64 || last == 0u64 || compare(payload(tree,first,width),payload(tree,last,width)) > 0) {
        return FactCursor(0u64,0u64);
    }
    return FactCursor(first,last);
}

address head(FactTree tree, FactCursor cursor, u64 width) on cpu {
    if (cursor.front == 0u64) { return address(); }
    return payload(tree,cursor.front,width);
}

FactCursor advance(FactTree tree, FactCursor previous, u64 width, bool reverse) on cpu {
    FactCursor state = previous;
    if (state.front == state.back) { return FactCursor(0u64,0u64); }
    if (reverse) {
        state.back = neighbor(tree,state.back,width,true);
    } else {
        state.front = neighbor(tree,state.front,width,false);
    }
    return state;
}

extern "C" address fact_tree_next(address owner, u64 width, address cursor, u32 reverse) on cpu {
    FactCursor state = unsafe_position<FactCursor>(cursor);
    if (state.front == 0u64) { return address(); }
    FactTree tree = unsafe_position<FactTree>(owner);
    address item = payload(tree,reverse == 0 ? state.front : state.back,width);
    unsafe_position<FactCursor>(cursor) = advance(tree,state,width,reverse != 0);
    return item;
}

// Modes are union (0), intersection (1) and left difference (2).
extern "C" address fact_tree_merge(address left, address right, address cursors, u64 width, u32 operation,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare) {
    FactTree a = unsafe_position<FactTree>(left);
    FactTree b = unsafe_position<FactTree>(right);
    address left_cursor = cursors;
    address right_cursor = offset(cursors,size_of<FactCursor>());
    while (true) {
        FactCursor x = unsafe_position<FactCursor>(left_cursor);
        FactCursor y = unsafe_position<FactCursor>(right_cursor);
        address first = head(a,x,width);
        address second = head(b,y,width);
        if (first == address()) {
            if (operation != 0 || second == address()) { return address(); }
            unsafe_position<FactCursor>(right_cursor) = advance(b,y,width,false);
            return second;
        }
        if (second == address()) {
            if (operation == 1) { return address(); }
            unsafe_position<FactCursor>(left_cursor) = advance(a,x,width,false);
            return first;
        }
        i32 order = compare(first,second);
        if (order <= 0) { unsafe_position<FactCursor>(left_cursor) = advance(a,x,width,false); }
        if (order >= 0) { unsafe_position<FactCursor>(right_cursor) = advance(b,y,width,false); }
        if (order == 0 ? operation != 2 : order < 0 ? operation != 1 : operation == 0) {
            return order <= 0 ? first : second;
        }
    }
}

extern "C" i32 fact_tree_compare(address left, address right, u64 width, cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    if (left == right) { return i32(0); }
    FactTree a = unsafe_position<FactTree>(left);
    FactTree b = unsafe_position<FactTree>(right);
    u64 x = extreme(a,a.root,width,false);
    u64 y = extreme(b,b.root,width,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = compare(payload(a,x,width),payload(b,y,width));
        if (order != 0) { return order; }
        x = neighbor(a,x,width,false);
        y = neighbor(b,y,width,false);
    }
    return x != 0u64 ? i32(1) : y != 0u64 ? -i32(1) : i32(0);
}

extern "C" u32 fact_tree_relation(address left, address right, u64 width, u32 subset,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare) {
    FactTree a = unsafe_position<FactTree>(left);
    FactTree b = unsafe_position<FactTree>(right);
    if (subset != 0 && a.count > b.count) { return 0; }
    if (subset == 0 && a.count > b.count) {
        FactTree temporary = a;
        a = b;
        b = temporary;
    }
    if (a.count == 0u64) { return 1; }
    if (b.count == 0u64) { return u32(subset == 0); }
    u64 x = extreme(a,a.root,width,false);
    // A sparse operand can probe the other root without walking its full order.
    if (a.count <= b.count / max(1u64,height(b,b.root,width))) {
        while (x != 0u64) {
            bool found = locate(b,payload(a,x,width),width,compare).exact != 0u64;
            if (found != (subset != 0)) { return 0; }
            x = neighbor(a,x,width,false);
        }
        return 1;
    }
    u64 y = extreme(b,b.root,width,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = compare(payload(a,x,width),payload(b,y,width));
        if (order == 0 && subset == 0 || order < 0 && subset != 0) { return 0; }
        if (order <= 0) { x = neighbor(a,x,width,false); }
        if (order >= 0) { y = neighbor(b,y,width,false); }
    }
    return u32(subset == 0 || x == 0u64);
}
