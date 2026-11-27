module bootstrap.proof_facts;
import bootstrap.fact_key;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct FactCell { OrderedNode node; FactKey key; };

// One-based node indices survive growth. The actual key occupies the
// aligned tail of each cell and is compared in place.
address offset(address base, u64 bytes) on cpu {
    return unsafe_address(address_bits(base) + bytes);
}

address node(address records, u64 index) on cpu {
    return offset(records, (index - 1u64) * size_of<FactCell>());
}

address payload(address records, u64 index) on cpu {
    return offset(node(records,index),size_of<OrderedNode>());
}

// The owning tree and row stride are fixed for the whole rotation.
struct TreeRows { address owner; address records; u64 stride; };

address row(address records,u64 stride,u64 index) on cpu {
    return offset(records,(index-1u64)*stride);
}

u64 height(TreeRows tree,u64 index) on cpu {
    if(index==0u64){return 0u64;}
    return unsafe_position<OrderedNode>(row(tree.records,tree.stride,index)).height;
}

void update_height(TreeRows tree,u64 index) on cpu {
    address slot=row(tree.records,tree.stride,index);
    OrderedNode value=unsafe_position<OrderedNode>(slot);
    unsafe_position<OrderedNode>(slot).height=1u64+max(height(tree,value.left),height(tree,value.right));
}

void set_parent(TreeRows tree,u64 index,u64 parent) on cpu {
    if(index!=0u64){unsafe_position<OrderedNode>(row(tree.records,tree.stride,index)).parent=parent;}
}

void replace_child(TreeRows tree,u64 parent,u64 old,u64 fresh) on cpu {
    if(parent==0u64){
        unsafe_position<OrderedTree>(tree.owner).root=fresh;
    }else{
        address slot=row(tree.records,tree.stride,parent);
        if(unsafe_position<OrderedNode>(slot).left==old){unsafe_position<OrderedNode>(slot).left=fresh;}
        else{unsafe_position<OrderedNode>(slot).right=fresh;}
    }
    set_parent(tree,fresh,parent);
}

u64 rotate(TreeRows tree,u64 low,bool left) on cpu {
    address low_slot=row(tree.records,tree.stride,low);
    OrderedNode old=unsafe_position<OrderedNode>(low_slot);
    u64 high=left?old.right:old.left;
    address high_slot=row(tree.records,tree.stride,high);
    OrderedNode elevated=unsafe_position<OrderedNode>(high_slot);
    u64 middle=left?elevated.left:elevated.right;
    replace_child(tree,old.parent,low,high);
    if(left){
        unsafe_position<OrderedNode>(low_slot).right=middle;
        unsafe_position<OrderedNode>(high_slot).left=low;
    }else{
        unsafe_position<OrderedNode>(low_slot).left=middle;
        unsafe_position<OrderedNode>(high_slot).right=low;
    }
    set_parent(tree,middle,low);
    set_parent(tree,low,high);
    update_height(tree,low);
    update_height(tree,high);
    return high;
}

void rebalance(TreeRows tree,u64 start) on cpu {
    u64 current=start;
    while(current!=0u64){
        address slot=row(tree.records,tree.stride,current);
        OrderedNode value=unsafe_position<OrderedNode>(slot);
        u64 left=height(tree,value.left);
        u64 right=height(tree,value.right);
        u64 top=current;
        if(left>right+1u64){
            OrderedNode child=unsafe_position<OrderedNode>(row(tree.records,tree.stride,value.left));
            if(height(tree,child.right)>height(tree,child.left)){rotate(tree,value.left,true);}
            top=rotate(tree,current,false);
        }else if(right>left+1u64){
            OrderedNode child=unsafe_position<OrderedNode>(row(tree.records,tree.stride,value.right));
            if(height(tree,child.left)>height(tree,child.right)){rotate(tree,value.right,false);}
            top=rotate(tree,current,true);
        }else{
            unsafe_position<OrderedNode>(slot).height=1u64+max(left,right);
        }
        current=unsafe_position<OrderedNode>(row(tree.records,tree.stride,top)).parent;
    }
}

u64 extreme(address records, u64 start, bool reverse) on cpu {
    u64 current = start;
    if (current != 0u64) {
        while (true) {
            OrderedNode value = unsafe_position<OrderedNode>(node(records,current));
            u64 child = reverse ? value.right : value.left;
            if (child == 0u64) { break; }
            current = child;
        }
    }
    return current;
}

u64 neighbor(address records, u64 start, bool reverse) on cpu {
    u64 current = start;
    OrderedNode value = unsafe_position<OrderedNode>(node(records,current));
    u64 child = reverse ? value.left : value.right;
    if (child != 0u64) { return extreme(records,child,reverse); }
    u64 parent = value.parent;
    while (parent != 0u64) {
        OrderedNode higher = unsafe_position<OrderedNode>(node(records,parent));
        if ((reverse ? higher.right : higher.left) == current) { break; }
        current = parent;
        parent = higher.parent;
    }
    return parent;
}

struct Search { u64 exact; u64 parent; i32 order; };

Search locate<const VIEW:u32>(address records, u64 root, address key) on cpu {
    Search result = Search(0u64,0u64,i32(0));
    u64 current = root;
    while (current != 0u64) {
        result.order = order_fact<VIEW>(key,payload(records,current));
        if (result.order == 0) {
            result.exact = current;
            break;
        }
        result.parent = current;
        OrderedNode value = unsafe_position<OrderedNode>(node(records,current));
        current = result.order < 0 ? value.left : value.right;
    }
    return result;
}

void grow(address owner, OrderedTree tree, u64 bytes) on cpu requires nonzero(bytes) {
    u64 limit = 9223372036854775807u64 / bytes;
    if (tree.count == limit) { abort(); return; }
    u64 capacity = tree.capacity == 0u64 ? 4u64 : tree.count + min(tree.count,limit - tree.count);
    address records = tree.capacity == 0u64
        ? HeapAlloc(GetProcessHeap(),0,capacity * bytes)
        : HeapReAlloc(GetProcessHeap(),0,tree.records,capacity * bytes);
    if (records == address()) { abort(); return; }
    unsafe_position<OrderedTree>(owner).records = records;
    unsafe_position<OrderedTree>(owner).capacity = capacity;
}

extern "C" void fact_tree_clear(address owner) on cpu {
    address records = unsafe_position<OrderedTree>(owner).records;
    if (records != address()) { HeapFree(GetProcessHeap(),0,records); }
    unsafe_position<OrderedTree>(owner) = OrderedTree(address(),0u64,0u64,0u64);
}

extern "C" void fact_tree_copy(address original, address output) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(original);
    if (tree.count != 0u64) {
        u64 bytes = tree.count * size_of<FactCell>();
        address records = HeapAlloc(GetProcessHeap(),0,bytes);
        if (records == address()) { abort(); return; }
        RtlMoveMemory(records,tree.records,bytes);
        unsafe_position<OrderedTree>(output) = OrderedTree(records,tree.root,tree.count,tree.count);
    }
}

address ordered_tree_prepare(address owner,u64 stride) on cpu requires nonzero(stride) {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    if(tree.count==tree.capacity){
        grow(owner,tree,stride);
        tree=unsafe_position<OrderedTree>(owner);
    }
    return row(tree.records,stride,tree.count+1u64);
}

void ordered_tree_publish(address owner,u64 stride,u64 parent,i32 order) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 fresh=tree.count+1u64;
    unsafe_position<OrderedNode>(row(tree.records,stride,fresh))=OrderedNode(0u64,0u64,parent,1u64);
    if(parent==0u64){
        unsafe_position<OrderedTree>(owner).root=fresh;
    }else{
        address slot=row(tree.records,stride,parent);
        if(order<0){unsafe_position<OrderedNode>(slot).left=fresh;}
        else{unsafe_position<OrderedNode>(slot).right=fresh;}
        rebalance(TreeRows(owner,tree.records,stride),parent);
    }
    unsafe_position<OrderedTree>(owner).count=fresh;
}

// The wire reader writes into the next real cell and publishes it by insert.
address fact_tree_prepare_key(address owner) on cpu {
    return offset(ordered_tree_prepare(owner,size_of<FactCell>()),size_of<OrderedNode>());
}

extern "C" u32 fact_tree_insert(address owner,address key) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    Search place=locate<0>(tree.records,tree.root,key);
    if(place.exact!=0u64){return 0;}
    address destination=fact_tree_prepare_key(owner);
    if(destination!=key){RtlMoveMemory(destination,key,size_of<FactKey>());}
    ordered_tree_publish(owner,size_of<FactCell>(),place.parent,place.order);
    return 1;
}

// Erase the current key and return its successor's new index for retain.
// Keeping live nodes packed avoids a separate free list and copies only the
// live allocation when a query first needs to own borrowed facts.
u64 erase(address owner, address records, u64 count, u64 erased) on cpu {
    TreeRows tree=TreeRows(owner,records,size_of<FactCell>());
    u64 current = erased;
    u64 next = neighbor(records,current,false);
    OrderedNode value = unsafe_position<OrderedNode>(node(records,current));
    if (value.left != 0u64 && value.right != 0u64) {
        RtlMoveMemory(payload(records,current),payload(records,next),size_of<FactKey>());
        u64 successor = next;
        next = current;
        current = successor;
        value = unsafe_position<OrderedNode>(node(records,current));
    }
    u64 child = value.left == 0u64 ? value.right : value.left;
    replace_child(tree,value.parent,current,child);
    rebalance(tree,value.parent);
    if (current != count) {
        OrderedNode moved = unsafe_position<OrderedNode>(node(records,count));
        RtlMoveMemory(node(records,current),node(records,count),size_of<FactCell>());
        replace_child(tree,moved.parent,count,current);
        set_parent(tree,moved.left,current);
        set_parent(tree,moved.right,current);
        if (next == count) { next = current; }
    }
    unsafe_position<OrderedTree>(owner).count = count - 1u64;
    return next;
}

extern "C" u32 fact_tree_remove(address owner, address key) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = locate<0>(tree.records,tree.root,key).exact;
    if (index == 0u64) { return 0; }
    erase(owner,tree.records,tree.count,index);
    return 1;
}

extern "C" u32 fact_tree_contains(address owner, address key) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    return u32(locate<0>(tree.records,tree.root,key).exact != 0u64);
}

u32 fact_tree_contains_view(address owner,address key) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    return u32(locate<1>(tree.records,tree.root,key).exact!=0u64);
}

extern "C" void fact_tree_retain(address owner, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = extreme(tree.records,tree.root,false);
    while (index != 0u64) {
        if (keep(context,payload(tree.records,index)) == 0) {
            index = erase(owner,tree.records,tree.count,index);
            tree = unsafe_position<OrderedTree>(owner);
        } else {
            index = neighbor(tree.records,index,false);
        }
    }
}

extern "C" FactCursor fact_tree_cursor(address owner) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    return FactCursor(extreme(tree.records,tree.root,false),extreme(tree.records,tree.root,true));
}

u64 bound<const VIEW:u32>(address records, u64 root, FactBound limit, bool reverse) on cpu {
    if (limit.key == address()) { return extreme(records,root,reverse); }
    u64 result = 0u64;
    u64 current = root;
    while (current != 0u64) {
        i32 order = order_fact<VIEW>(limit.key,payload(records,current));
        if (order == 0 && limit.exclusive == 0) { return current; }
        OrderedNode value = unsafe_position<OrderedNode>(node(records,current));
        if (reverse ? order > 0 : order < 0) {
            result = current;
            current = reverse ? value.right : value.left;
        } else {
            current = reverse ? value.left : value.right;
        }
    }
    return result;
}

extern "C" FactCursor fact_tree_range(address owner, FactBound start, FactBound end) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 first = bound<0>(tree.records,tree.root,start,false);
    u64 last = bound<0>(tree.records,tree.root,end,true);
    if (first == 0u64 || last == 0u64 || order_fact<0>(payload(tree.records,first),payload(tree.records,last)) > 0) {
        return FactCursor(0u64,0u64);
    }
    return FactCursor(first,last);
}

bool fact_tree_range_contains_view(address owner,FactBound start,FactBound end) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 first=bound<1>(tree.records,tree.root,start,false);
    if(first==0u64){return false;}
    i32 order=order_fact<1>(end.key,payload(tree.records,first));
    return order>0 || (order==0 && end.exclusive==0);
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
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    address item = payload(tree.records,reverse == 0 ? state.front : state.back);
    unsafe_position<FactCursor>(cursor) = advance(tree.records,state,reverse != 0);
    return item;
}

// Modes are union (0), intersection (1) and left difference (2).
extern "C" address fact_tree_merge(address left, address right, address cursors, u32 operation) on cpu {
    OrderedTree a = unsafe_position<OrderedTree>(left);
    OrderedTree b = unsafe_position<OrderedTree>(right);
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
        i32 order = order_fact<0>(first,second);
        if (order <= 0) { unsafe_position<FactCursor>(left_cursor) = advance(a.records,x,false); }
        if (order >= 0) { unsafe_position<FactCursor>(right_cursor) = advance(b.records,y,false); }
        if (order == 0 ? operation != 2 : order < 0 ? operation != 1 : operation == 0) {
            return order <= 0 ? first : second;
        }
    }
}

struct LinkedTree { u64 root; u64 height; };

LinkedTree link_ordered(address records,u64 first,u64 count,u64 parent) on cpu {
    if(count==0u64){return LinkedTree(0u64,0u64);}
    u64 left_count=count/2u64;
    u64 index=first+left_count;
    LinkedTree left=link_ordered(records,first,left_count,index);
    LinkedTree right=link_ordered(records,index+1u64,count-left_count-1u64,index);
    u64 height=1u64+max(left.height,right.height);
    unsafe_position<OrderedNode>(node(records,index))=OrderedNode(left.root,right.root,parent,height);
    return LinkedTree(index,height);
}

extern "C" void fact_tree_collect_merge(address left,address right,address cursors,u32 operation,address output) on cpu {
    address key=fact_tree_merge(left,right,cursors,operation);
    while(key!=address()){
        address cell=ordered_tree_prepare(output,size_of<FactCell>());
        RtlMoveMemory(offset(cell,size_of<OrderedNode>()),key,size_of<FactKey>());
        unsafe_position<OrderedTree>(output).count+=1u64;
        key=fact_tree_merge(left,right,cursors,operation);
    }
    OrderedTree tree=unsafe_position<OrderedTree>(output);
    LinkedTree linked=link_ordered(tree.records,1u64,tree.count,0u64);
    unsafe_position<OrderedTree>(output).root=linked.root;
}

extern "C" i32 fact_tree_compare(address left, address right) on cpu {
    if (left == right) { return i32(0); }
    OrderedTree a = unsafe_position<OrderedTree>(left);
    OrderedTree b = unsafe_position<OrderedTree>(right);
    u64 x = extreme(a.records,a.root,false);
    u64 y = extreme(b.records,b.root,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = order_fact<0>(payload(a.records,x),payload(b.records,y));
        if (order != 0) { return order; }
        x = neighbor(a.records,x,false);
        y = neighbor(b.records,y,false);
    }
    return x != 0u64 ? i32(1) : y != 0u64 ? -i32(1) : i32(0);
}

extern "C" u32 fact_tree_relation(address left, address right, u32 subset) on cpu {
    OrderedTree a = unsafe_position<OrderedTree>(left);
    OrderedTree b = unsafe_position<OrderedTree>(right);
    if (subset != 0 && a.count > b.count) { return 0; }
    if (subset == 0 && a.count > b.count) {
        OrderedTree temporary = a;
        a = b;
        b = temporary;
    }
    if (a.count == 0u64) { return 1; }
    if (b.count == 0u64) { return u32(subset == 0); }
    u64 x = extreme(a.records,a.root,false);
    // A sparse operand can probe the other root without walking its full order.
    if (a.count <= b.count / max(1u64,unsafe_position<OrderedNode>(node(b.records,b.root)).height)) {
        while (x != 0u64) {
            bool found = locate<0>(b.records,b.root,payload(a.records,x)).exact != 0u64;
            if (found != (subset != 0)) { return 0; }
            x = neighbor(a.records,x,false);
        }
        return 1;
    }
    u64 y = extreme(b.records,b.root,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = order_fact<0>(payload(a.records,x),payload(b.records,y));
        if (order == 0 && subset == 0 || order < 0 && subset != 0) { return 0; }
        if (order <= 0) { x = neighbor(a.records,x,false); }
        if (order >= 0) { y = neighbor(b.records,y,false); }
    }
    return u32(subset == 0 || x == 0u64);
}
