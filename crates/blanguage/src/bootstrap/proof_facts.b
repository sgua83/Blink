module bootstrap.proof_facts;
import bootstrap.fact_key;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap, u32 flags, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap, u32 flags, address data, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap, u32 flags, address data) on cpu from "KERNEL32.dll";
extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

struct FactCell { OrderedNode node; FactKey key; };
struct AliasFactCell { OrderedNode node; FactKey key; u64 supports; };
struct DomainCell { OrderedNode node; IndexDomainKey key; };

u64 tree_stride<const DOMAIN:u32 in {0,1,2}>() on cpu {
    return DOMAIN==0 ? size_of<FactCell>() : DOMAIN==1 ? size_of<DomainCell>() : size_of<AliasFactCell>();
}

u64 tree_key_bytes<const DOMAIN:u32 in {0,1,2}>() on cpu {
    return DOMAIN==0 ? size_of<FactKey>() : DOMAIN==1 ? size_of<IndexDomainKey>() : size_of<FactKey>()+size_of<u64>();
}

i32 order_tree_key<const VIEW:u32,const DOMAIN:u32 in {0,1,2}>(address left,address right) on cpu {
    if(DOMAIN==1){return order_domain(left,right);}
    return order_fact<VIEW>(left,right);
}

// One-based node indices survive growth. The actual key occupies the
// aligned tail of each cell and is compared in place.
address offset(address base, u64 bytes) on cpu {
    return unsafe_address(address_bits(base) + bytes);
}

address node<const DOMAIN:u32 in {0,1,2}>(address records, u64 index) on cpu {
    return offset(records, (index - 1u64) * tree_stride<DOMAIN>());
}

address payload<const DOMAIN:u32 in {0,1,2}>(address records, u64 index) on cpu {
    return offset(node<DOMAIN>(records,index),size_of<OrderedNode>());
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

u64 tree_extreme(address records, u64 stride, u64 start, bool reverse) on cpu {
    u64 current = start;
    if (current != 0u64) {
        while (true) {
            OrderedNode value = unsafe_position<OrderedNode>(row(records,stride,current));
            u64 child = reverse ? value.right : value.left;
            if (child == 0u64) { break; }
            current = child;
        }
    }
    return current;
}

u64 tree_neighbor(address records, u64 stride, u64 start, bool reverse) on cpu {
    u64 current = start;
    OrderedNode value = unsafe_position<OrderedNode>(row(records,stride,current));
    u64 child = reverse ? value.left : value.right;
    if (child != 0u64) { return tree_extreme(records,stride,child,reverse); }
    u64 parent = value.parent;
    while (parent != 0u64) {
        OrderedNode higher = unsafe_position<OrderedNode>(row(records,stride,parent));
        if ((reverse ? higher.right : higher.left) == current) { break; }
        current = parent;
        parent = higher.parent;
    }
    return parent;
}

u64 extreme<const DOMAIN:u32 in {0,1,2}>(address records,u64 start,bool reverse) on cpu {
    return tree_extreme(records,tree_stride<DOMAIN>(),start,reverse);
}

u64 neighbor<const DOMAIN:u32 in {0,1,2}>(address records,u64 start,bool reverse) on cpu {
    return tree_neighbor(records,tree_stride<DOMAIN>(),start,reverse);
}

u64 ordered_tree_first(OrderedTree tree,u64 stride) on cpu {
    return tree_extreme(tree.records,stride,tree.root,false);
}

u64 ordered_tree_next(OrderedTree tree,u64 stride,u64 current) on cpu {
    return tree_neighbor(tree.records,stride,current,false);
}

struct Search { u64 exact; u64 parent; i32 order; };

Search locate<const VIEW:u32,const DOMAIN:u32 in {0,1,2}>(address records, u64 root, address key) on cpu {
    Search result = Search(0u64,0u64,i32(0));
    u64 current = root;
    while (current != 0u64) {
        result.order = order_tree_key<VIEW,DOMAIN>(key,payload<DOMAIN>(records,current));
        if (result.order == 0) {
            result.exact = current;
            break;
        }
        result.parent = current;
        OrderedNode value = unsafe_position<OrderedNode>(node<DOMAIN>(records,current));
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

OrderedTree clone_storage(OrderedTree tree,u64 stride) on cpu {
    let records = HeapAlloc(GetProcessHeap(),0,tree.count * stride);
    if (records == address()) { abort(); }
    return OrderedTree(records,tree.root,tree.count,tree.count);
}

void ordered_tree_copy(address original,address output,u64 stride) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(original);
    if (tree.count == 0u64) { return; }
    OrderedTree copy = clone_storage(tree,stride);
    RtlMoveMemory(copy.records,tree.records,tree.count * stride);
    unsafe_position<OrderedTree>(output) = copy;
}

extern "C" void fact_tree_copy(address original, address output) on cpu {
    ordered_tree_copy(original,output,size_of<FactCell>());
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

u32 insert_key<const DOMAIN:u32 in {0,1}>(address owner,address key) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    Search place=locate<0,DOMAIN>(tree.records,tree.root,key);
    if(place.exact!=0u64){return 0;}
    address destination=offset(ordered_tree_prepare(owner,tree_stride<DOMAIN>()),size_of<OrderedNode>());
    if(destination!=key){RtlMoveMemory(destination,key,tree_key_bytes<DOMAIN>());}
    ordered_tree_publish(owner,tree_stride<DOMAIN>(),place.parent,place.order);
    return 1;
}

extern "C" u32 fact_tree_insert(address owner,address key) on cpu {
    return insert_key<0>(owner,key);
}

// Erase the current key and return its successor's new index for retain.
// Keeping live nodes packed avoids a separate free list and copies only the
// live allocation when a query first needs to own borrowed facts.
u64 erase_ordered(TreeRows tree, u64 count, u64 erased) on cpu {
    u64 current = erased;
    u64 next = tree_neighbor(tree.records,tree.stride,current,false);
    OrderedNode value = unsafe_position<OrderedNode>(row(tree.records,tree.stride,current));
    if (value.left != 0u64 && value.right != 0u64) {
        RtlMoveMemory(offset(row(tree.records,tree.stride,current),size_of<OrderedNode>()),
            offset(row(tree.records,tree.stride,next),size_of<OrderedNode>()),tree.stride-size_of<OrderedNode>());
        u64 successor = next;
        next = current;
        current = successor;
        value = unsafe_position<OrderedNode>(row(tree.records,tree.stride,current));
    }
    u64 child = value.left == 0u64 ? value.right : value.left;
    replace_child(tree,value.parent,current,child);
    rebalance(tree,value.parent);
    if (current != count) {
        OrderedNode moved = unsafe_position<OrderedNode>(row(tree.records,tree.stride,count));
        RtlMoveMemory(row(tree.records,tree.stride,current),row(tree.records,tree.stride,count),tree.stride);
        replace_child(tree,moved.parent,count,current);
        set_parent(tree,moved.left,current);
        set_parent(tree,moved.right,current);
        if (next == count) { next = current; }
    }
    unsafe_position<OrderedTree>(tree.owner).count = count - 1u64;
    return next;
}

u64 erase<const DOMAIN:u32 in {0,1,2}>(address owner,address records,u64 count,u64 erased) on cpu {
    return erase_ordered(TreeRows(owner,records,tree_stride<DOMAIN>()),count,erased);
}

OrderedPlace find_ordered_place(address records,u64 stride,address key,
    u64 start,u64 parent,i32 side,cfn<i32,address,address> compare) on cpu requires nonzero(compare) {
    OrderedPlace place = OrderedPlace(address(),parent,side);
    u64 current = start;
    while (current != 0u64) {
        let cell = row(records,stride,current);
        let entry = offset(cell,size_of<OrderedNode>());
        place.order = compare(key,entry);
        if (place.order == 0) {
            place.entry = entry;
            return place;
        }
        place.parent = current;
        let node = unsafe_position<OrderedNode>(cell);
        current = place.order < 0 ? node.left : node.right;
    }
    return place;
}

u64 tree_entry_index(OrderedTree tree,u64 stride,address entry) on cpu requires nonzero(stride) {
    return (address_bits(entry)-address_bits(tree.records)-size_of<OrderedNode>())/stride+1u64;
}

extern "C" OrderedPlace ordered_tree_seek(address owner,u64 stride,address key,cfn<i32,address,address> compare) on cpu
    requires nonzero(compare) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    return find_ordered_place(tree.records,stride,key,tree.root,0u64,i32(0),compare);
}

// Each incoming key is greater than the previous key, which the caller has
// already merged. Rotations and growth leave its row index valid.
OrderedPlace ordered_tree_seek_next(address owner,u64 stride,address key,
    inout u64 position,cfn<i32,address,address> compare) on cpu
    requires nonzero(stride) requires nonzero(compare) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 start = tree.root;
    u64 lower = position;
    i32 side = i32(0);
    if (lower != 0u64) {
        u64 child = lower;
        u64 above = unsafe_position<OrderedNode>(row(tree.records,stride,child)).parent;
        while (above != 0u64) {
            address cell = row(tree.records,stride,above);
            OrderedNode node = unsafe_position<OrderedNode>(cell);
            if (node.left == child) {
                address entry = offset(cell,size_of<OrderedNode>());
                i32 order = compare(key,entry);
                if (order < 0) { break; }
                if (order == 0) {
                    position = above;
                    return OrderedPlace(entry,node.parent,i32(0));
                }
                lower = above;
            }
            child = above;
            above = node.parent;
        }
        start = unsafe_position<OrderedNode>(row(tree.records,stride,lower)).right;
        side = i32(1);
    }
    OrderedPlace place = find_ordered_place(tree.records,stride,key,start,lower,side,compare);
    position = place.entry == address() ? tree.count+1u64 : tree_entry_index(tree,stride,place.entry);
    return place;
}

extern "C" address ordered_tree_insert_slot(address owner,u64 stride,u64 parent,i32 order) on cpu
    requires nonzero(stride) {
    let cell = ordered_tree_prepare(owner,stride);
    ordered_tree_publish(owner,stride,parent,order);
    return offset(cell,size_of<OrderedNode>());
}

extern "C" void ordered_tree_erase_slot(address owner,u64 stride,address entry) on cpu
    requires nonzero(stride) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = tree_entry_index(tree,stride,entry);
    erase_ordered(TreeRows(owner,tree.records,stride),tree.count,index);
}

extern "C" void ordered_tree_retain_owned(address owner,u64 stride,address context,cfn<u32,address,address> retain) on cpu
    requires nonzero(retain) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = tree_extreme(tree.records,stride,tree.root,false);
    while (index != 0u64) {
        let entry = offset(row(tree.records,stride,index),size_of<OrderedNode>());
        if (retain(context,entry) == 0) {
            index = erase_ordered(TreeRows(owner,tree.records,stride),tree.count,index);
            tree.count -= 1u64;
        } else {
            index = tree_neighbor(tree.records,stride,index,false);
        }
    }
}

extern "C" void ordered_tree_clone_owned(address original,address output,u64 stride,cfn<void,address,address> clone) on cpu
    requires nonzero(clone) {
    OrderedTree source = unsafe_position<OrderedTree>(original);
    if (source.count == 0u64) { return; }
    OrderedTree destination = clone_storage(source,stride);
    u64 index = 1u64;
    while (index <= source.count) {
        let from = row(source.records,stride,index);
        let to = row(destination.records,stride,index);
        unsafe_position<OrderedNode>(to) = unsafe_position<OrderedNode>(from);
        clone(offset(from,size_of<OrderedNode>()),offset(to,size_of<OrderedNode>()));
        index += 1u64;
    }
    unsafe_position<OrderedTree>(output) = destination;
}

extern "C" FactCursor ordered_tree_cursor(address owner,u64 stride) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    return FactCursor(tree_extreme(tree.records,stride,tree.root,false),tree_extreme(tree.records,stride,tree.root,true));
}

extern "C" address ordered_tree_step(address owner,u64 stride,address cursor,u32 reverse) on cpu {
    FactCursor state = unsafe_position<FactCursor>(cursor);
    if (state.front == 0u64) { return address(); }
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 at = reverse == 0 ? state.front : state.back;
    if (state.front == state.back) {
        state = FactCursor(0u64,0u64);
    } else if (reverse == 0) {
        state.front = tree_neighbor(tree.records,stride,state.front,false);
    } else {
        state.back = tree_neighbor(tree.records,stride,state.back,true);
    }
    unsafe_position<FactCursor>(cursor) = state;
    return offset(row(tree.records,stride,at),size_of<OrderedNode>());
}

extern "C" u32 fact_tree_remove(address owner, address key) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = locate<0,0>(tree.records,tree.root,key).exact;
    if (index == 0u64) { return 0; }
    erase<0>(owner,tree.records,tree.count,index);
    return 1;
}

extern "C" u32 fact_tree_contains(address owner, address key) on cpu {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    return u32(locate<0,0>(tree.records,tree.root,key).exact != 0u64);
}

u32 fact_tree_contains_view(address owner,address key) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    return u32(locate<1,0>(tree.records,tree.root,key).exact!=0u64);
}

void retain_rows<const DOMAIN:u32 in {0,1}>(address owner, address context, cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    OrderedTree tree = unsafe_position<OrderedTree>(owner);
    u64 index = extreme<DOMAIN>(tree.records,tree.root,false);
    while (index != 0u64) {
        if (keep(context,payload<DOMAIN>(tree.records,index)) == 0) {
            index = erase<DOMAIN>(owner,tree.records,tree.count,index);
            tree = unsafe_position<OrderedTree>(owner);
        } else {
            index = neighbor<DOMAIN>(tree.records,index,false);
        }
    }
}

extern "C" void fact_tree_retain(address owner,address context,cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    retain_rows<0>(owner,context,keep);
}

FactCursor row_cursor<const DOMAIN:u32 in {0,1,2}>(address owner) on cpu {
    return ordered_tree_cursor(owner,tree_stride<DOMAIN>());
}

extern "C" FactCursor fact_tree_cursor(address owner) on cpu {
    return row_cursor<0>(owner);
}

u64 bound<const VIEW:u32,const DOMAIN:u32 in {0,1,2}>(address records, u64 root, FactBound limit, bool reverse) on cpu {
    if (limit.key == address()) { return extreme<DOMAIN>(records,root,reverse); }
    u64 result = 0u64;
    u64 current = root;
    while (current != 0u64) {
        i32 order = order_tree_key<VIEW,DOMAIN>(limit.key,payload<DOMAIN>(records,current));
        if (order == 0 && limit.exclusive == 0) { return current; }
        OrderedNode value = unsafe_position<OrderedNode>(node<DOMAIN>(records,current));
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
    u64 first = bound<0,0>(tree.records,tree.root,start,false);
    u64 last = bound<0,0>(tree.records,tree.root,end,true);
    if (first == 0u64 || last == 0u64 || order_fact<0>(payload<0>(tree.records,first),payload<0>(tree.records,last)) > 0) {
        return FactCursor(0u64,0u64);
    }
    return FactCursor(first,last);
}

bool fact_tree_range_contains_view(address owner,FactBound start,FactBound end) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 first=bound<1,0>(tree.records,tree.root,start,false);
    if(first==0u64){return false;}
    i32 order=order_fact<1>(end.key,payload<0>(tree.records,first));
    return order>0 || (order==0 && end.exclusive==0);
}

address head<const DOMAIN:u32 in {0,1}>(address records, FactCursor cursor) on cpu {
    if (cursor.front == 0u64) { return address(); }
    return payload<DOMAIN>(records,cursor.front);
}

FactCursor advance<const DOMAIN:u32 in {0,1,2}>(address records, FactCursor previous, bool reverse) on cpu {
    FactCursor state = previous;
    if (state.front == state.back) { return FactCursor(0u64,0u64); }
    if (reverse) {
        state.back = neighbor<DOMAIN>(records,state.back,true);
    } else {
        state.front = neighbor<DOMAIN>(records,state.front,false);
    }
    return state;
}

address next_key<const DOMAIN:u32 in {0,1,2}>(address owner, address cursor, u32 reverse) on cpu {
    return ordered_tree_step(owner,tree_stride<DOMAIN>(),cursor,reverse);
}

extern "C" address fact_tree_next(address owner,address cursor,u32 reverse) on cpu {
    return next_key<0>(owner,cursor,reverse);
}

u32 store_alias_fact(address owner,address key,bool seed) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    Search place=locate<0,2>(tree.records,tree.root,key);
    if(place.exact!=0u64){
        if(seed){unsafe_position<AliasFactCell>(node<2>(tree.records,place.exact)).supports+=1u64;}
        return 0;
    }
    address cell=ordered_tree_prepare(owner,size_of<AliasFactCell>());
    RtlMoveMemory(address_of(unsafe_position<AliasFactCell>(cell).key),key,size_of<FactKey>());
    unsafe_position<AliasFactCell>(cell).supports=1u64;
    ordered_tree_publish(owner,size_of<AliasFactCell>(),place.parent,place.order);
    return 1;
}

extern "C" void alias_facts_build(address owner,address facts,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup) {
    FactCursor cursor=fact_tree_cursor(facts);
    FactKey key;
    address output=unsafe_address(address_bits(address_of(key)));
    while(true){
        address fact=fact_tree_next(facts,address_of(cursor),0);
        if(fact==address()){break;}
        if(unsafe_position<FactKind>(fact)==FactKind::Equal){continue;}
        rename_fact_key(fact,output,roots,lookup);
        store_alias_fact(owner,output,true);
    }
}

extern "C" u32 alias_facts_insert(address owner,address fact,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup) {
    if(unsafe_position<FactKind>(fact)==FactKind::Equal){return 0;}
    FactKey key;
    address output=unsafe_address(address_bits(address_of(key)));
    rename_fact_key(fact,output,roots,lookup);
    return store_alias_fact(owner,output,false);
}

extern "C" void alias_facts_remove(address owner,address fact,address roots,cfn<address,address,address> lookup) on cpu
    requires nonzero(lookup) {
    if(unsafe_position<FactKind>(fact)==FactKind::Equal){return;}
    FactKey key;
    address output=unsafe_address(address_bits(address_of(key)));
    rename_fact_key(fact,output,roots,lookup);
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 index=locate<0,2>(tree.records,tree.root,output).exact;
    if(index==0u64){return;}
    address cell=node<2>(tree.records,index);
    if(unsafe_position<AliasFactCell>(cell).supports>1u64){
        unsafe_position<AliasFactCell>(cell).supports-=1u64;
    }else{
        erase<2>(owner,tree.records,tree.count,index);
    }
}

FactCursor conflict_fact_cursor(address owner,u32 aliases) on cpu {
    return aliases==0 ? row_cursor<0>(owner) : row_cursor<2>(owner);
}

address conflict_fact_next(address owner,address cursor,u32 aliases) on cpu {
    return aliases==0 ? next_key<0>(owner,cursor,0) : next_key<2>(owner,cursor,0);
}

address conflict_fact_select(address owner,address key,u32 preceding,u32 aliases) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    FactBound limit=FactBound(key,0);
    if(aliases==0){
        u64 index=bound<0,0>(tree.records,tree.root,limit,preceding!=0);
        return index==0u64 ? address() : payload<0>(tree.records,index);
    }
    u64 index=bound<0,2>(tree.records,tree.root,limit,preceding!=0);
    return index==0u64 ? address() : payload<2>(tree.records,index);
}

bool conflict_fact_contains(address owner,address key,u32 aliases) on cpu {
    if(aliases==0){return fact_tree_contains(owner,key)!=0;}
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    return locate<0,2>(tree.records,tree.root,key).exact!=0u64;
}

// Modes are union (0), intersection (1) and left difference (2).
address merge_keys<const DOMAIN:u32 in {0,1}>(address left,address right,address left_cursor,address right_cursor,u32 operation) on cpu {
    OrderedTree a = unsafe_position<OrderedTree>(left);
    OrderedTree b = unsafe_position<OrderedTree>(right);
    while (true) {
        FactCursor x = unsafe_position<FactCursor>(left_cursor);
        FactCursor y = unsafe_position<FactCursor>(right_cursor);
        address first = head<DOMAIN>(a.records,x);
        address second = head<DOMAIN>(b.records,y);
        if (first == address()) {
            if (operation != 0 || second == address()) { return address(); }
            unsafe_position<FactCursor>(right_cursor) = advance<DOMAIN>(b.records,y,false);
            return second;
        }
        if (second == address()) {
            if (operation == 1) { return address(); }
            unsafe_position<FactCursor>(left_cursor) = advance<DOMAIN>(a.records,x,false);
            return first;
        }
        i32 order = order_tree_key<0,DOMAIN>(first,second);
        if (order <= 0) { unsafe_position<FactCursor>(left_cursor) = advance<DOMAIN>(a.records,x,false); }
        if (order >= 0) { unsafe_position<FactCursor>(right_cursor) = advance<DOMAIN>(b.records,y,false); }
        if (order == 0 ? operation != 2 : order < 0 ? operation != 1 : operation == 0) {
            return order <= 0 ? first : second;
        }
    }
}

extern "C" address fact_tree_merge(address left,address right,address cursors,u32 operation) on cpu {
    return merge_keys<0>(left,right,cursors,offset(cursors,size_of<FactCursor>()),operation);
}

struct LinkedTree { u64 root; u64 height; };

LinkedTree link_ordered(address records,u64 first,u64 count,u64 parent) on cpu {
    if(count==0u64){return LinkedTree(0u64,0u64);}
    u64 left_count=count/2u64;
    u64 index=first+left_count;
    LinkedTree left=link_ordered(records,first,left_count,index);
    LinkedTree right=link_ordered(records,index+1u64,count-left_count-1u64,index);
    u64 height=1u64+max(left.height,right.height);
    unsafe_position<OrderedNode>(node<0>(records,index))=OrderedNode(left.root,right.root,parent,height);
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

extern "C" i32 ordered_tree_compare(address left,address right,u64 stride,
    cfn<i32,address,address> compare) on cpu requires nonzero(compare) {
    if (left == right) { return i32(0); }
    OrderedTree a = unsafe_position<OrderedTree>(left);
    OrderedTree b = unsafe_position<OrderedTree>(right);
    u64 x = ordered_tree_first(a,stride);
    u64 y = ordered_tree_first(b,stride);
    while (x != 0u64 && y != 0u64) {
        address first = offset(row(a.records,stride,x),size_of<OrderedNode>());
        address second = offset(row(b.records,stride,y),size_of<OrderedNode>());
        i32 order = compare(first,second);
        if (order != 0) { return order; }
        x = ordered_tree_next(a,stride,x);
        y = ordered_tree_next(b,stride,y);
    }
    return x != 0u64 ? i32(1) : y != 0u64 ? -i32(1) : i32(0);
}

extern "C" i32 fact_tree_compare(address left,address right) on cpu {
    return ordered_tree_compare(left,right,tree_stride<0>(),c_function(order_fact<0>));
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
    u64 x = extreme<0>(a.records,a.root,false);
    // A sparse operand can probe the other root without walking its full order.
    if (a.count <= b.count / max(1u64,unsafe_position<OrderedNode>(node<0>(b.records,b.root)).height)) {
        while (x != 0u64) {
            bool found = locate<0,0>(b.records,b.root,payload<0>(a.records,x)).exact != 0u64;
            if (found != (subset != 0)) { return 0; }
            x = neighbor<0>(a.records,x,false);
        }
        return 1;
    }
    u64 y = extreme<0>(b.records,b.root,false);
    while (x != 0u64 && y != 0u64) {
        i32 order = order_fact<0>(payload<0>(a.records,x),payload<0>(b.records,y));
        if (order == 0 && subset == 0 || order < 0 && subset != 0) { return 0; }
        if (order <= 0) { x = neighbor<0>(a.records,x,false); }
        if (order >= 0) { y = neighbor<0>(b.records,y,false); }
    }
    return u32(subset == 0 || x == 0u64);
}

extern "C" void domain_tree_copy(address original,address output) on cpu {
    ordered_tree_copy(original,output,tree_stride<1>());
}

extern "C" u32 domain_tree_insert(address owner,address key) on cpu {
    return insert_key<1>(owner,key);
}

extern "C" u32 domain_tree_contains(address owner,address key) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    return u32(locate<0,1>(tree.records,tree.root,key).exact!=0u64);
}

extern "C" u32 domain_tree_contains_range(address owner,address start,address end) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 first=bound<0,1>(tree.records,tree.root,FactBound(start,0),false);
    return u32(first!=0u64&&order_domain(payload<1>(tree.records,first),end)<=i32(0));
}

extern "C" void domain_tree_append(address owner,address incoming) on cpu {
    OrderedTree source=unsafe_position<OrderedTree>(incoming);
    if(source.count==0u64){return;}
    if(unsafe_position<OrderedTree>(owner).count==0u64){
        fact_tree_clear(owner);
        unsafe_position<OrderedTree>(owner)=source;
        unsafe_position<OrderedTree>(incoming)=OrderedTree(address(),0u64,0u64,0u64);
        return;
    }
    FactCursor cursor=row_cursor<1>(incoming);
    while(true){
        address key=next_key<1>(incoming,address_of(cursor),0);
        if(key==address()){break;}
        insert_key<1>(owner,key);
    }
    fact_tree_clear(incoming);
}

extern "C" void domain_tree_retain(address owner,address context,cfn<u32,address,address> keep) on cpu
    requires nonzero(keep) {
    retain_rows<1>(owner,context,keep);
}

extern "C" FactCursor domain_tree_cursor(address owner) on cpu {
    return row_cursor<1>(owner);
}

extern "C" address domain_tree_next(address owner,address cursor,u32 reverse) on cpu {
    return next_key<1>(owner,cursor,reverse);
}

extern "C" address domain_tree_merge(address left,address right,address left_cursor,address right_cursor,u32 operation) on cpu {
    return merge_keys<1>(left,right,left_cursor,right_cursor,operation);
}

extern "C" i32 domain_tree_compare(address left,address right) on cpu {
    return ordered_tree_compare(left,right,tree_stride<1>(),c_function(order_domain));
}
