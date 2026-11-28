module bootstrap.output_bindings;
import bootstrap.fact_key;
import bootstrap.proof_facts;
import bootstrap.proof_origins;

struct OutputBindingCell { OrderedNode links; IdentityKey source; IdentityKey target; };
struct OutputBindingPosition { address match_cell; u64 parent; i32 order; };

address output_binding_cell(OrderedTree tree,u64 index) on cpu {
    return unsafe_address(address_bits(tree.records)+(index-1u64)*size_of<OutputBindingCell>());
}

OutputBindingPosition output_binding_position(OrderedTree tree,address identity) on cpu {
    u64 parent=0u64;
    i32 order=i32(0);
    u64 index=tree.root;
    while(index!=0u64){
        address cell=output_binding_cell(tree,index);
        address key=unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).source)));
        order=order_identity(identity,key);
        if(order==i32(0)){return OutputBindingPosition(cell,parent,order);}
        parent=index;
        OrderedNode links=unsafe_position<OutputBindingCell>(cell).links;
        index=order<i32(0) ? links.left : links.right;
    }
    return OutputBindingPosition(address(),parent,order);
}

void store_output_binding(address owner,IdentityKey source,IdentityKey target,OutputBindingPosition position) on cpu {
    address cell=ordered_tree_prepare(owner,size_of<OutputBindingCell>());
    unsafe_position<OutputBindingCell>(cell).source=source;
    unsafe_position<OutputBindingCell>(cell).target=target;
    ordered_tree_publish(owner,size_of<OutputBindingCell>(),position.parent,position.order);
}

extern "C" void write_output_binding(address owner,address source,address target) on cpu {
    OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
    IdentityKey value=make_key_identity(capture_key_identity(target));
    if(position.match_cell==address()){
        store_output_binding(owner,make_key_identity(capture_key_identity(source)),value,position);
    }else{
        unsafe_position<OutputBindingCell>(position.match_cell).target=value;
    }
}

void collect_output_row(address owner,address origins,usize count,usize result,usize entry) on cpu {
    IdentityKey target=make_key_identity(NumericIdentity(IdentityKind::Register,result,0u64,0u64));
    usize index=0u64;
    while(index<count){
        address source=unsafe_address(address_bits(origins)+index*size_of<IdentityKey>());
        NumericIdentity key=capture_key_identity(source);
        if(key.kind==IdentityKind::TaskCompletion||
            (key.kind==IdentityKind::Register&&key.first>=entry&&key.first!=result)){
            OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
            if(position.match_cell==address()){store_output_binding(owner,make_key_identity(key),target,position);}
        }
        index+=1u64;
    }
}

extern "C" void collect_output_bindings(address owner,address rows,address registers,usize count,usize entry) on cpu {
    usize width=min(unsafe_position<OrderedTree>(rows).count,count);
    usize component=0u64;
    while(component<width){
        OrderedTree row=unsafe_position<OrderedTree>(origin_row(rows,component));
        address reg=unsafe_address(address_bits(registers)+component*size_of<usize>());
        collect_output_row(owner,row.records,row.count,unsafe_position<usize>(reg),entry);
        component+=1u64;
    }
}

extern "C" address find_output_binding(address owner,address source) on cpu {
    OutputBindingPosition position=output_binding_position(unsafe_position<OrderedTree>(owner),source);
    if(position.match_cell==address()){return address();}
    return unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(position.match_cell).target)));
}

extern "C" OutputBindingEntry next_output_binding(address owner,address cursor) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 previous=unsafe_position<u64>(cursor);
    u64 index=previous==0u64 ? ordered_tree_first(tree,size_of<OutputBindingCell>())
        : ordered_tree_next(tree,size_of<OutputBindingCell>(),previous);
    unsafe_position<u64>(cursor)=index;
    if(index==0u64){return OutputBindingEntry(address(),address());}
    address cell=output_binding_cell(tree,index);
    return OutputBindingEntry(unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).source))),
        unsafe_address(address_bits(address_of(unsafe_position<OutputBindingCell>(cell).target))));
}

extern "C" void copy_output_bindings(address original,address output) on cpu {
    ordered_tree_copy(original,output,size_of<OutputBindingCell>());
}

extern "C" u32 same_output_bindings(address left,address right) on cpu {
    OrderedTree a=unsafe_position<OrderedTree>(left);
    OrderedTree b=unsafe_position<OrderedTree>(right);
    if(a.count!=b.count){return 0;}
    u64 i=0u64;
    u64 j=0u64;
    while(true){
        OutputBindingEntry x=next_output_binding(left,address_of(i));
        if(x.source==address()){return 1;}
        OutputBindingEntry y=next_output_binding(right,address_of(j));
        if(order_identity(x.source,y.source)!=0||order_identity(x.target,y.target)!=0){return 0;}
    }
}

struct RootResult { address root; bool present; };

RootResult root_of(address roots,address identity,bool linked) on cpu {
    address first=find_output_binding(roots,identity);
    if(first==address()){return RootResult(identity,false);}
    address parent=first;
    if(linked){
        while(true){
            address next=find_output_binding(roots,parent);
            if(next==address()||order_identity(next,parent)==0){break;}
            parent=next;
        }
    }
    return RootResult(parent,true);
}

void flatten_equality_roots(address roots) on cpu {
    u64 cursor=0u64;
    while(true){
        OutputBindingEntry entry=next_output_binding(roots,address_of(cursor));
        if(entry.source==address()){return;}
        // Each parent precedes its members, so the parent's entry is already final.
        address parent=find_output_binding(roots,entry.target);
        if(parent!=address()&&order_identity(parent,entry.target)!=0){
            unsafe_position<IdentityKey>(entry.target)=make_key_identity(capture_key_identity(parent));
        }
    }
}

struct EqualityChanges { bool merged; u32 changed; };

EqualityChanges add_equality_root(address roots,address key,EqualityChanges prior) on cpu {
    let equality = read_fact_fields(key);
    let left = root_of(roots, equality.first, prior.merged);
    let right = root_of(roots, equality.second, prior.merged);
    let order = order_identity(left.root, right.root);
    if (order == i32(0)) {
        if (!left.present) { write_output_binding(roots, equality.first, left.root); }
        return prior;
    }
    let child = order < i32(0) ? right : left;
    // A write may move the root tree; keep the selected representative in a local slot.
    IdentityKey parent_key = make_key_identity(capture_key_identity(order < i32(0) ? left.root : right.root));
    let parent = unsafe_address(address_bits(address_of(parent_key)));
    if (child.present) { write_output_binding(roots, child.root, parent); }
    if (!left.present) { write_output_binding(roots, equality.first, parent); }
    if (!right.present) { write_output_binding(roots, equality.second, parent); }
    return EqualityChanges(prior.merged || child.present, 1);
}

extern "C" u32 extend_equality_roots(address roots,address context,cfn<address,address> next) on cpu
    requires nonzero(next) {
    EqualityChanges changes = EqualityChanges(false, 0);
    while (true) {
        let key = next(context);
        if (key == address() || unsafe_position<FactKind>(key) != FactKind::Equal) { break; }
        changes = add_equality_root(roots, key, changes);
    }
    if (changes.merged) { flatten_equality_roots(roots); }
    return changes.changed;
}

void apply_computed_equalities(address roots,address equalities) on cpu {
    FactCursor cursor = fact_tree_cursor(equalities);
    EqualityChanges changes = EqualityChanges(false, 0);
    while (true) {
        let key = fact_tree_next(equalities, address_of(cursor), 0);
        if (key == address()) { break; }
        changes = add_equality_root(roots, key, changes);
    }
    if (changes.merged) { flatten_equality_roots(roots); }
}

extern "C" void RtlMoveMemory(address output,address input,u64 bytes) on cpu from "KERNEL32.dll";

struct ComputedRoot { IndexDomainKey expression; IdentityKey result; };
struct ComputedBinaryKey {
    DomainKind domain;
    u32 reserved;
    u32 expression;
    u8 operation;
    u8 scalar;
    OperandKey first;
    OperandKey second;
    u64 tail;
};

address computed_field(address domain,u64 offset) on cpu {
    return unsafe_address(address_bits(domain) + offset);
}

void remember_computed(address produced,address equalities,address domain,address result) on cpu {
    let stride = size_of<OrderedNode>() + size_of<ComputedRoot>();
    let found = ordered_tree_seek(produced, stride, domain, c_function(order_domain));
    if (found.entry == address()) {
        let entry = ordered_tree_insert_slot(produced, stride, found.parent, found.order);
        RtlMoveMemory(entry, domain, size_of<IndexDomainKey>());
        unsafe_position<ComputedRoot>(entry).result = make_key_identity(capture_key_identity(result));
        return;
    }
    let first = unsafe_address(address_bits(address_of(unsafe_position<ComputedRoot>(found.entry).result)));
    let order = order_identity(first, result);
    if (order == i32(0)) { return; }
    FactKey fact = FactKey(array<u64,18>(0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64));
    let left = order < i32(0) ? first : result;
    let right = order < i32(0) ? result : first;
    let fact_slot = unsafe_address(address_bits(address_of(fact)));
    materialize_fact(FactView(1, FactKind::Equal, left, right, address(), address(), 0u64, 0u64, 0, 0), fact_slot);
    fact_tree_insert(equalities, fact_slot);
}

void remember_inverse(address produced,address equalities,address domain,address result) on cpu {
    // The binary operands follow the Computed tag and the expression header.
    if (unsafe_position<u32>(computed_field(domain, 8u64)) != 0) { return; }
    let op = unsafe_position<u8>(computed_field(domain, 12u64));
    if (op != u8(0) && op != u8(1)) { return; }
    let left = computed_field(domain, 16u64);
    let right = computed_field(left, size_of<OperandKey>());
    let swap = op == u8(0) && unsafe_position<OperandKind>(left) == OperandKind::Constant;
    let first = swap ? right : left;
    let second = swap ? left : right;
    if (unsafe_position<OperandKind>(first) != OperandKind::Value ||
        unsafe_position<OperandKind>(second) != OperandKind::Constant) { return; }

    IdentityKey input = make_key_identity(capture_key_identity(computed_field(first, 8u64)));
    let known = capture_key_operand(second);
    let constant = make_key_operand(known);
    let resolved = make_key_operand(NumericOperand(OperandKind::Value, capture_key_identity(result), known.scalar, 0u64));
    ComputedBinaryKey inverse = ComputedBinaryKey(DomainKind::Computed, 0, 0, op == u8(0) ? u8(1) : u8(0),
        unsafe_position<u8>(computed_field(domain, 13u64)), resolved, constant, 0u64);
    remember_computed(produced, equalities, unsafe_address(address_bits(address_of(inverse))), unsafe_address(address_bits(address_of(input))));
}

void remember_expression(address roots,address produced,address equalities,address source) on cpu {
    IndexDomainKey normalized = IndexDomainKey(array<u64,13>(0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64));
    let fact = read_fact_fields(source);
    let scratch = unsafe_address(address_bits(address_of(normalized)));
    let domain = rename_index_domain(fact.payload, scratch, roots, c_function(find_output_binding));
    let mapped = find_output_binding(roots, fact.first);
    remember_computed(produced, equalities, domain, mapped == address() ? fact.first : mapped);
    if (mapped != address()) { remember_inverse(produced, equalities, domain, mapped); }
}

extern "C" void resolve_computed_roots(address roots,address sources,usize count) on cpu {
    OrderedTree produced = OrderedTree(address(), 0u64, 0u64, 0u64);
    OrderedTree equalities = OrderedTree(address(), 0u64, 0u64, 0u64);
    while (true) {
        usize index = 0u64;
        while (index < count) {
            let slot = computed_field(sources, index * size_of<address>());
            remember_expression(roots, address_of(produced), address_of(equalities), unsafe_position<address>(slot));
            index += 1u64;
        }
        if (equalities.count == 0u64) { break; }
        apply_computed_equalities(roots, address_of(equalities));
        produced.root = 0u64;
        produced.count = 0u64;
        equalities.root = 0u64;
        equalities.count = 0u64;
    }
    fact_tree_clear(address_of(equalities));
    fact_tree_clear(address_of(produced));
}


extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address records) on cpu from "KERNEL32.dll";
extern "C" void abort() on cpu from "ucrtbase.dll";

address member_slot(EqualityMembers members,usize index) on cpu {
    return unsafe_address(address_bits(members.records) + index * size_of<OutputBindingEntry>());
}

OutputBindingEntry read_member(EqualityMembers members,usize index) on cpu {
    return unsafe_position<OutputBindingEntry>(member_slot(members, index));
}

void write_member(EqualityMembers members,usize index,OutputBindingEntry entry) on cpu {
    unsafe_position<OutputBindingEntry>(member_slot(members, index)) = entry;
}

void sift_members(EqualityMembers members,usize start,usize end) on cpu {
    usize root = start;
    let entry = read_member(members, root);
    while (root < end / 2u64) {
        usize child = root * 2u64 + 1u64;
        let left = read_member(members, child);
        if (child + 1u64 < end && order_identity(left.target, read_member(members, child + 1u64).target) < 0) {
            child += 1u64;
        }
        let next = read_member(members, child);
        if (order_identity(entry.target, next.target) >= 0) { break; }
        write_member(members, root, next);
        root = child;
    }
    write_member(members, root, entry);
}

void order_members(EqualityMembers members) on cpu {
    usize root = members.count / 2u64;
    while (root > 0u64) {
        root -= 1u64;
        sift_members(members, root, members.count);
    }
    usize end = members.count;
    while (end > 1u64) {
        end -= 1u64;
        let first = read_member(members, 0u64);
        write_member(members, 0u64, read_member(members, end));
        write_member(members, end, first);
        sift_members(members, 0u64, end);
    }
}

extern "C" EqualityMembers build_equality_members(address roots) on cpu {
    let count = unsafe_position<OrderedTree>(roots).count;
    if (count == 0u64) { return EqualityMembers(address(), 0u64); }
    let records = HeapAlloc(GetProcessHeap(), 0, count * size_of<OutputBindingEntry>());
    if (records == address()) { abort(); }
    let members = EqualityMembers(records, count);
    u64 cursor = 0u64;
    usize index = 0u64;
    while (index < count) {
        write_member(members, index, next_output_binding(roots, address_of(cursor)));
        index += 1u64;
    }
    order_members(members);
    return members;
}

extern "C" void release_equality_members(address records) on cpu {
    if (records != address()) { HeapFree(GetProcessHeap(), 0, records); }
}

usize first_equality_member(EqualityMembers members,address root) on cpu {
    usize begin = 0u64;
    usize end = members.count;
    while (begin < end) {
        let middle = begin + (end - begin) / 2u64;
        if (order_identity(read_member(members, middle).target, root) < 0) { begin = middle + 1u64; }
        else { end = middle; }
    }
    return begin;
}

struct AliasProjection {
    address environment;
    address facts;
    address roots;
    EqualityMembers members;
    address previous;
    address output;
};

void append_alias_fact(AliasProjection query,FactView original,address member) on cpu {
    FactView fact = original;
    fact.first = member;
    FactKey key = FactKey(array<u64,18>(0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64));
    let slot = unsafe_address(address_bits(address_of(key)));
    materialize_fact(fact, slot);
    if (fact_tree_contains(query.environment, slot) == 0 && fact_tree_contains(query.facts, slot) == 0) {
        fact_tree_insert(query.output, slot);
    }
}

void project_alias_members(AliasProjection query,FactView fact,address root,address previous) on cpu {
    usize index = first_equality_member(query.members, root);
    while (index < query.members.count) {
        let entry = read_member(query.members, index);
        if (order_identity(entry.target, root) != 0) { break; }
        index += 1u64;
        if (order_identity(entry.source, fact.first) == 0) { continue; }
        if (previous != address()) {
            let old = find_output_binding(query.previous, entry.source);
            if (old != address() && order_identity(old, previous) == 0) { continue; }
        }
        append_alias_fact(query, fact, entry.source);
    }
}

void project_alias_source(AliasProjection query,address source,bool inherited) on cpu {
    FactCursor cursor = fact_tree_cursor(source);
    while (true) {
        let key = fact_tree_next(source, address_of(cursor), 0);
        if (key == address()) { break; }
        let fact = read_fact_fields(key);
        if (fact.kind == FactKind::Equal || fact.kind == FactKind::NotEqual ||
            fact.kind == FactKind::Below || fact.kind == FactKind::Predicate) { continue; }
        let root = find_output_binding(query.roots, fact.first);
        if (root == address()) { continue; }
        let previous = inherited ? find_output_binding(query.previous, fact.first) : address();
        if (previous != address() && order_identity(previous, fact.first) != 0) { continue; }
        project_alias_members(query, fact, root, previous);
    }
}

extern "C" void project_alias_facts(address environment,address facts,address roots,EqualityMembers members,
    address previous,address output) on cpu {
    if (members.count == 0u64) { return; }
    let query = AliasProjection(environment, facts, roots, members, previous, output);
    project_alias_source(query, facts, false);
    if (previous != address()) { project_alias_source(query, environment, true); }
}
