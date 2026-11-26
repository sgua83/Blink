module bootstrap.proof_facts;
import bootstrap.fact_key;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct FactNode { u64 left; u64 right; u64 parent; u64 height; };
struct FactCell { FactNode node; FactKey key; };

// One-based node indices survive growth. The actual key occupies the
// aligned tail of each cell and is compared in place.
address offset(address base, u64 bytes) on cpu {
    return unsafe_address(address_bits(base) + bytes);
}

address node(address records, u64 index) on cpu {
    return offset(records, (index - 1u64) * size_of<FactCell>());
}

address payload(address records, u64 index) on cpu {
    return offset(node(records,index),size_of<FactNode>());
}

u64 height(address records, u64 index) on cpu {
    if (index == 0u64) { return 0u64; }
    return unsafe_position<FactNode>(node(records,index)).height;
}

void update_height(address records, u64 index) on cpu {
    address entry = node(records,index);
    FactNode value = unsafe_position<FactNode>(entry);
    unsafe_position<FactNode>(entry).height =
        1u64 + max(height(records,value.left),height(records,value.right));
}

void set_parent(address records, u64 index, u64 parent) on cpu {
    if (index != 0u64) { unsafe_position<FactNode>(node(records,index)).parent = parent; }
}

void replace_child(address owner, address records, u64 parent, u64 old, u64 fresh) on cpu {
    if (parent == 0u64) {
        unsafe_position<FactTree>(owner).root = fresh;
    } else {
        address entry = node(records,parent);
        if (unsafe_position<FactNode>(entry).left == old) {
            unsafe_position<FactNode>(entry).left = fresh;
        } else {
            unsafe_position<FactNode>(entry).right = fresh;
        }
    }
    set_parent(records,fresh,parent);
}

u64 rotate(address owner, address records, u64 low, bool left) on cpu {
    address low_node = node(records,low);
    FactNode old = unsafe_position<FactNode>(low_node);
    u64 high = left ? old.right : old.left;
    address high_node = node(records,high);
    FactNode elevated = unsafe_position<FactNode>(high_node);
    u64 middle = left ? elevated.left : elevated.right;
    replace_child(owner,records,old.parent,low,high);
    if (left) {
        unsafe_position<FactNode>(low_node).right = middle;
        unsafe_position<FactNode>(high_node).left = low;
    } else {
        unsafe_position<FactNode>(low_node).left = middle;
        unsafe_position<FactNode>(high_node).right = low;
    }
    set_parent(records,middle,low);
    set_parent(records,low,high);
    update_height(records,low);
    update_height(records,high);
    return high;
}

void rebalance(address owner, u64 start) on cpu {
    u64 current = start;
    address records = unsafe_position<FactTree>(owner).records;
    while (current != 0u64) {
        FactNode value = unsafe_position<FactNode>(node(records,current));
        u64 left = height(records,value.left);
        u64 right = height(records,value.right);
        u64 top = current;
        if (left > right + 1u64) {
            FactNode child = unsafe_position<FactNode>(node(records,value.left));
            if (height(records,child.right) > height(records,child.left)) {
                rotate(owner,records,value.left,true);
            }
            top = rotate(owner,records,current,false);
        } else if (right > left + 1u64) {
            FactNode child = unsafe_position<FactNode>(node(records,value.right));
            if (height(records,child.left) > height(records,child.right)) {
                rotate(owner,records,value.right,false);
            }
            top = rotate(owner,records,current,true);
        } else {
            unsafe_position<FactNode>(node(records,current)).height = 1u64 + max(left,right);
        }
        current = unsafe_position<FactNode>(node(records,top)).parent;
    }
}

u64 extreme(address records, u64 start, bool reverse) on cpu {
    u64 current = start;
    if (current != 0u64) {
        while (true) {
            FactNode value = unsafe_position<FactNode>(node(records,current));
            u64 child = reverse ? value.right : value.left;
            if (child == 0u64) { break; }
            current = child;
        }
    }
    return current;
}

u64 neighbor(address records, u64 start, bool reverse) on cpu {
    u64 current = start;
    FactNode value = unsafe_position<FactNode>(node(records,current));
    u64 child = reverse ? value.left : value.right;
    if (child != 0u64) { return extreme(records,child,reverse); }
    u64 parent = value.parent;
    while (parent != 0u64) {
        FactNode higher = unsafe_position<FactNode>(node(records,parent));
        if ((reverse ? higher.right : higher.left) == current) { break; }
        current = parent;
        parent = higher.parent;
    }
    return parent;
}

struct Search { u64 exact; u64 parent; i32 order; };

Search locate(address records, u64 root, address key) on cpu {
    Search result = Search(0u64,0u64,i32(0));
    u64 current = root;
    while (current != 0u64) {
        result.order = order_fact(key,payload(records,current));
        if (result.order == 0) {
            result.exact = current;
            break;
        }
        result.parent = current;
        FactNode value = unsafe_position<FactNode>(node(records,current));
        current = result.order < 0 ? value.left : value.right;
    }
    return result;
}

void grow(address owner, FactTree tree) on cpu {
    u64 bytes = size_of<FactCell>();
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

extern "C" void fact_tree_copy(address original, address output) on cpu {
    FactTree tree = unsafe_position<FactTree>(original);
    if (tree.count != 0u64) {
        u64 bytes = tree.count * size_of<FactCell>();
        address records = HeapAlloc(GetProcessHeap(),0,bytes);
        if (records == address()) { abort(); return; }
        RtlMoveMemory(records,tree.records,bytes);
        unsafe_position<FactTree>(output) = FactTree(records,tree.root,tree.count,tree.count);
    }
}

extern "C" u32 fact_tree_insert(address owner, address key) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    Search place = locate(tree.records,tree.root,key);
    if (place.exact != 0u64) { return 0; }
    if (tree.capacity == tree.count) {
        grow(owner,tree);
        tree = unsafe_position<FactTree>(owner);
    }
    u64 fresh = tree.count + 1u64;
    unsafe_position<FactNode>(node(tree.records,fresh)) = FactNode(0u64,0u64,place.parent,1u64);
    RtlMoveMemory(payload(tree.records,fresh),key,size_of<FactKey>());
    if (place.parent == 0u64) {
        unsafe_position<FactTree>(owner).root = fresh;
    } else {
        address parent = node(tree.records,place.parent);
        if (place.order < 0) {
            unsafe_position<FactNode>(parent).left = fresh;
        } else {
            unsafe_position<FactNode>(parent).right = fresh;
        }
        rebalance(owner,place.parent);
    }
    unsafe_position<FactTree>(owner).count = fresh;
    return 1;
}

// Erase the current key and return its successor's new index for retain.
// Keeping live nodes packed avoids a separate free list and copies only the
// live allocation when a query first needs to own borrowed facts.
u64 erase(address owner, address records, u64 count, u64 erased) on cpu {
    u64 current = erased;
    u64 next = neighbor(records,current,false);
    FactNode value = unsafe_position<FactNode>(node(records,current));
    if (value.left != 0u64 && value.right != 0u64) {
        RtlMoveMemory(payload(records,current),payload(records,next),size_of<FactKey>());
        u64 successor = next;
        next = current;
        current = successor;
        value = unsafe_position<FactNode>(node(records,current));
    }
    u64 child = value.left == 0u64 ? value.right : value.left;
    replace_child(owner,records,value.parent,current,child);
    rebalance(owner,value.parent);
    if (current != count) {
        FactNode moved = unsafe_position<FactNode>(node(records,count));
        RtlMoveMemory(node(records,current),node(records,count),size_of<FactCell>());
        replace_child(owner,records,moved.parent,count,current);
        set_parent(records,moved.left,current);
        set_parent(records,moved.right,current);
        if (next == count) { next = current; }
    }
    unsafe_position<FactTree>(owner).count = count - 1u64;
    return next;
}

extern "C" u32 fact_tree_remove(address owner, address key) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 index = locate(tree.records,tree.root,key).exact;
    if (index == 0u64) { return 0; }
    erase(owner,tree.records,tree.count,index);
    return 1;
}

extern "C" u32 fact_tree_contains(address owner, address key) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    return u32(locate(tree.records,tree.root,key).exact != 0u64);
}

extern "C" void fact_tree_retain(address owner, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 index = extreme(tree.records,tree.root,false);
    while (index != 0u64) {
        if (keep(context,payload(tree.records,index)) == 0) {
            index = erase(owner,tree.records,tree.count,index);
            tree = unsafe_position<FactTree>(owner);
        } else {
            index = neighbor(tree.records,index,false);
        }
    }
}

extern "C" FactCursor fact_tree_cursor(address owner) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    return FactCursor(extreme(tree.records,tree.root,false),extreme(tree.records,tree.root,true));
}

u64 bound(address records, u64 root, FactBound limit, bool reverse) on cpu {
    if (limit.key == address()) { return extreme(records,root,reverse); }
    u64 result = 0u64;
    u64 current = root;
    while (current != 0u64) {
        i32 order = order_fact(payload(records,current),limit.key);
        if (order == 0 && limit.exclusive == 0) { return current; }
        FactNode value = unsafe_position<FactNode>(node(records,current));
        if (reverse ? order < 0 : order > 0) {
            result = current;
            current = reverse ? value.right : value.left;
        } else {
            current = reverse ? value.left : value.right;
        }
    }
    return result;
}

extern "C" FactCursor fact_tree_range(address owner, FactBound start, FactBound end) on cpu {
    FactTree tree = unsafe_position<FactTree>(owner);
    u64 first = bound(tree.records,tree.root,start,false);
    u64 last = bound(tree.records,tree.root,end,true);
    if (first == 0u64 || last == 0u64 || order_fact(payload(tree.records,first),payload(tree.records,last)) > 0) {
        return FactCursor(0u64,0u64);
    }
    return FactCursor(first,last);
}

address head(address records, FactCursor cursor) on cpu {
    if (cursor.front == 0u64) { return address(); }
    return payload(records,cursor.front);
}

FactCursor advance(address records, FactCursor previous, bool reverse) on cpu {
    FactCursor state = previous;
    if (state.front == state.back) { return FactCursor(0u64,0u64); }
    if (reverse) {
        state.back = neighbor(records,state.back,true);
    } else {
        state.front = neighbor(records,state.front,false);
    }
    return state;
}

extern "C" address fact_tree_next(address owner, address cursor, u32 reverse) on cpu {
    FactCursor state = unsafe_position<FactCursor>(cursor);
    if (state.front == 0u64) { return address(); }
    FactTree tree = unsafe_position<FactTree>(owner);
    address item = payload(tree.records,reverse == 0 ? state.front : state.back);
    unsafe_position<FactCursor>(cursor) = advance(tree.records,state,reverse != 0);
    return item;
}

// Modes are union (0), intersection (1) and left difference (2).
extern "C" address fact_tree_merge(address left, address right, address cursors, u32 operation) on cpu {
    FactTree a = unsafe_position<FactTree>(left);
    FactTree b = unsafe_position<FactTree>(right);
    address left_cursor = cursors;
    address right_cursor = offset(cursors,size_of<FactCursor>());
    while (true) {
        FactCursor x = unsafe_position<FactCursor>(left_cursor);
        FactCursor y = unsafe_position<FactCursor>(right_cursor);
        address first = head(a.records,x);
        address second = head(b.records,y);
        if (first == address()) {
            if (operation != 0 || second == address()) { return address(); }
            unsafe_position<FactCursor>(right_cursor) = advance(b.records,y,false);
            return second;
        }
        if (second == address()) {
            if (operation == 1) { return address(); }
            unsafe_position<FactCursor>(left_cursor) = advance(a.records,x,false);
            return first;
        }
        i32 order = order_fact(first,second);
        if (order <= 0) { unsafe_position<FactCursor>(left_cursor) = advance(a.records,x,false); }
        if (order >= 0) { unsafe_position<FactCursor>(right_cursor) = advance(b.records,y,false); }
        if (order == 0 ? operation != 2 : order < 0 ? operation != 1 : operation == 0) {
            return order <= 0 ? first : second;
        }
    }
}

extern "C" i32 fact_tree_compare(address left, address right) on cpu {
    if (left == right) { return i32(0); }
    FactTree a = unsafe_position<FactTree>(left);
    FactTree b = unsafe_position<FactTree>(right);
    u64 x = extreme(a.records,a.root,false);
    u64 y = extreme(b.records,b.root,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = order_fact(payload(a.records,x),payload(b.records,y));
        if (order != 0) { return order; }
        x = neighbor(a.records,x,false);
        y = neighbor(b.records,y,false);
    }
    return x != 0u64 ? i32(1) : y != 0u64 ? -i32(1) : i32(0);
}

extern "C" u32 fact_tree_relation(address left, address right, u32 subset) on cpu {
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
    u64 x = extreme(a.records,a.root,false);
    // A sparse operand can probe the other root without walking its full order.
    if (a.count <= b.count / max(1u64,height(b.records,b.root))) {
        while (x != 0u64) {
            bool found = locate(b.records,b.root,payload(a.records,x)).exact != 0u64;
            if (found != (subset != 0)) { return 0; }
            x = neighbor(a.records,x,false);
        }
        return 1;
    }
    u64 y = extreme(b.records,b.root,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = order_fact(payload(a.records,x),payload(b.records,y));
        if (order == 0 && subset == 0 || order < 0 && subset != 0) { return 0; }
        if (order <= 0) { x = neighbor(a.records,x,false); }
        if (order >= 0) { y = neighbor(b.records,y,false); }
    }
    return u32(subset == 0 || x == 0u64);
}
