module bootstrap.clause_storage;
import bootstrap.fact_key;

extern "C" handle GetProcessHeap() on cpu from "KERNEL32.dll";
extern "C" address HeapAlloc(handle heap,u32 flags,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" address HeapReAlloc(handle heap,u32 flags,address data,u64 bytes) on cpu from "KERNEL32.dll";
extern "C" i32 HeapFree(handle heap,u32 flags,address data) on cpu from "KERNEL32.dll";
extern "C" never abort() on cpu from "ucrtbase.dll";

struct ClauseHeader { u64 strong; u64 identities; };

address header_at(address payload) on cpu {
    return unsafe_address(address_bits(payload) - size_of<ClauseHeader>());
}

struct ClausePair { address first; address second; };
struct ClauseChildren { address first; address second; address third; };

address payload_fields(address payload) on cpu {
    return unsafe_address(address_bits(payload) + size_of<StoredClause>());
}

SourceNode clause_source(address clause) on cpu {
    address payload = unsafe_position<address>(clause);
    StoredClause node = unsafe_position<StoredClause>(payload);
    address fields = payload_fields(payload);
    if (node.kind == StoredClauseKind::Impossible) {
        return SourceNode(0,address(),address(),address(),0,0,address());
    }
    if (node.kind == StoredClauseKind::Facts) {
        u32 empty = u32(unsafe_position<OrderedTree>(fields).count == 0u64);
        return SourceNode(1,fields,address(),address(),0,empty,address());
    }
    address left = address_of(unsafe_position<ClausePair>(fields).first);
    address right = address_of(unsafe_position<ClausePair>(fields).second);
    if (node.kind == StoredClauseKind::Branch) {
        address cases = payload_fields(unsafe_position<address>(right));
        return SourceNode(4,left,address_of(unsafe_position<ClausePair>(cases).second),
                          address_of(unsafe_position<ClausePair>(cases).first),node.truth,0,right);
    }
    return SourceNode(u32(node.kind),left,right,address(),0,0,address());
}

address allocate_clause(StoredClauseKind kind,u32 truth,u64 bytes) on cpu {
    address header = HeapAlloc(GetProcessHeap(),0,size_of<ClauseHeader>() + size_of<StoredClause>() + bytes);
    if (header == address()) { abort(); }
    // The strong group holds one identity reference until its payload is destroyed.
    unsafe_position<ClauseHeader>(header) = ClauseHeader(1u64,1u64);
    address payload = unsafe_address(address_bits(header) + size_of<ClauseHeader>());
    unsafe_position<StoredClause>(payload) = StoredClause(kind,truth);
    return payload;
}

extern "C" address clause_impossible() on cpu {
    return allocate_clause(StoredClauseKind::Impossible,0,0u64);
}

extern "C" address clause_facts(address facts) on cpu {
    address payload = allocate_clause(StoredClauseKind::Facts,0,size_of<OrderedTree>());
    unsafe_position<OrderedTree>(payload_fields(payload)) = unsafe_position<OrderedTree>(facts);
    return payload;
}

extern "C" address clause_pair(StoredClauseKind kind,u32 truth,address first,address second) on cpu {
    address payload = allocate_clause(kind,truth,size_of<ClausePair>());
    unsafe_position<ClausePair>(payload_fields(payload)) = ClausePair(first,second);
    return payload;
}

void retain_count(address counter) on cpu {
    u64 previous = atomic_add(unsafe_position<atomic<u64>>(counter),1u64,relaxed);
    if (previous > 9223372036854775807u64) { abort(); }
}

address strong_at(address payload) on cpu {
    return address_of(unsafe_position<ClauseHeader>(header_at(payload)).strong);
}

address identities_at(address payload) on cpu {
    return address_of(unsafe_position<ClauseHeader>(header_at(payload)).identities);
}

extern "C" void clause_retain(address payload) on cpu {
    retain_count(strong_at(payload));
}

extern "C" void clause_retain_identity(address payload) on cpu {
    retain_count(identities_at(payload));
}

extern "C" void clause_release_identity(address payload) on cpu {
    if (atomic_sub(unsafe_position<atomic<u64>>(identities_at(payload)),1u64,acq_rel) == 1u64) {
        HeapFree(GetProcessHeap(),0,header_at(payload));
    }
}

struct PendingClauses { address records; u64 length; u64 capacity; };

address pending_slot(address records,u64 index) on cpu {
    return unsafe_address(address_bits(records) + index * size_of<address>());
}

void push_clause(inout PendingClauses pending,address payload) on cpu {
    if (payload == address()) { return; }
    if (pending.length == pending.capacity) {
        u64 next = pending.capacity == 0u64 ? 8u64 : pending.capacity * 2u64;
        address records = pending.records == address()
            ? HeapAlloc(GetProcessHeap(),0,next * size_of<address>())
            : HeapReAlloc(GetProcessHeap(),0,pending.records,next * size_of<address>());
        if (records == address()) { abort(); }
        pending.records = records;
        pending.capacity = next;
    }
    unsafe_position<address>(pending_slot(pending.records,pending.length)) = payload;
    pending.length += 1u64;
}

address pop_clause(inout PendingClauses pending) on cpu {
    if (pending.length == 0u64) { return address(); }
    pending.length -= 1u64;
    return unsafe_position<address>(pending_slot(pending.records,pending.length));
}

bool release_strong(address payload) on cpu {
    return atomic_sub(unsafe_position<atomic<u64>>(strong_at(payload)),1u64,acq_rel) == 1u64;
}

ClauseChildren consume_fields(address payload) on cpu {
    StoredClause node = unsafe_position<StoredClause>(payload);
    if (node.kind == StoredClauseKind::Impossible) {
        return ClauseChildren(address(),address(),address());
    }
    address fields = payload_fields(payload);
    if (node.kind == StoredClauseKind::Facts) {
        fact_tree_clear(fields);
        return ClauseChildren(address(),address(),address());
    }
    ClausePair pair = unsafe_position<ClausePair>(fields);
    if (node.kind != StoredClauseKind::Branch) {
        return ClauseChildren(pair.first,pair.second,address());
    }
    // Release the shared condition before following the branch's parent.
    if (release_strong(pair.second)) {
        ClausePair condition = unsafe_position<ClausePair>(payload_fields(pair.second));
        clause_release_identity(pair.second);
        return ClauseChildren(pair.first,condition.first,condition.second);
    }
    return ClauseChildren(pair.first,address(),address());
}

extern "C" void clause_release(address payload) on cpu {
    PendingClauses pending = PendingClauses(address(),0u64,0u64);
    address current = payload;
    while (current != address()) {
        if (release_strong(current)) {
            ClauseChildren children = consume_fields(current);
            clause_release_identity(current);
            push_clause(pending,children.second);
            push_clause(pending,children.third);
            current = children.first;
        } else {
            current = address();
        }
        if (current == address()) { current = pop_clause(pending); }
    }
    if (pending.records != address()) { HeapFree(GetProcessHeap(),0,pending.records); }
}

address retain_payload(address payload) on cpu {
    clause_retain(payload);
    return payload;
}

address pair_of(StoredClauseKind kind,u32 truth,address left,address right) on cpu {
    return clause_pair(kind,truth,retain_payload(left),retain_payload(right));
}

StoredClause clause_header(address payload) on cpu {
    return unsafe_position<StoredClause>(payload);
}

ClausePair clause_edges(address payload) on cpu {
    return unsafe_position<ClausePair>(payload_fields(payload));
}

address selected_case(address condition,u32 truth) on cpu {
    ClausePair cases=clause_edges(condition);
    return truth==0 ? cases.first : cases.second;
}

bool contains_conjunct(StoredClause source,address payload,address other) on cpu {
    if(source.kind!=StoredClauseKind::Both&&source.kind!=StoredClauseKind::Branch){return false;}
    ClausePair pair=clause_edges(payload);
    address right=source.kind==StoredClauseKind::Branch ? selected_case(pair.second,source.truth) : pair.second;
    return pair.first==other||right==other;
}

address complementary_parent(address left,address right) on cpu {
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Branch&&b.kind==StoredClauseKind::Branch&&a.truth!=b.truth){
        ClausePair x=clause_edges(left);
        ClausePair y=clause_edges(right);
        if(x.first==y.first&&x.second==y.second){return x.first;}
    }
    return address();
}

// The caller transfers this tree. A contradictory tree is released here.
extern "C" address checked_clause_facts(address facts,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    if(conflict(facts)!=0){
        fact_tree_clear(facts);
        return clause_impossible();
    }
    return clause_facts(facts);
}

address unite_facts(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    array<FactCursor,2> cursors=array<FactCursor,2>(fact_tree_cursor(left),fact_tree_cursor(right));
    OrderedTree facts=OrderedTree(address(),0u64,0u64,0u64);
    fact_tree_collect_merge(left,right,address_of(cursors[0]),0,address_of(facts));
    return checked_clause_facts(address_of(facts),conflict);
}

extern "C" address combine_clause_both(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Impossible||b.kind==StoredClauseKind::Impossible){return clause_impossible();}
    if(a.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(left)).count==0u64){
        return retain_payload(right);
    }
    if(b.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(right)).count==0u64){
        return retain_payload(left);
    }
    if(a.kind==StoredClauseKind::Facts&&b.kind==StoredClauseKind::Facts){
        return unite_facts(payload_fields(left),payload_fields(right),conflict);
    }
    if(left==right||contains_conjunct(a,left,right)){return retain_payload(left);}
    if(contains_conjunct(b,right,left)){return retain_payload(right);}
    return pair_of(StoredClauseKind::Both,0,left,right);
}

extern "C" address combine_clause_either(address left,address right,cfn<u32,address> conflict) on cpu
 requires nonzero(conflict) {
    address parent=complementary_parent(left,right);
    if(parent!=address()){return retain_payload(parent);}
    StoredClause a=clause_header(left);
    StoredClause b=clause_header(right);
    if(a.kind==StoredClauseKind::Either){
        ClausePair pair=clause_edges(left);
        parent=complementary_parent(pair.second,right);
        if(parent!=address()){return combine_clause_either(pair.first,parent,conflict);}
        parent=complementary_parent(pair.first,right);
        if(parent!=address()){return combine_clause_either(parent,pair.second,conflict);}
    }
    if(b.kind==StoredClauseKind::Either){
        ClausePair pair=clause_edges(right);
        parent=complementary_parent(left,pair.first);
        if(parent!=address()){return combine_clause_either(parent,pair.second,conflict);}
        parent=complementary_parent(left,pair.second);
        if(parent!=address()){return combine_clause_either(pair.first,parent,conflict);}
    }
    if(a.kind==StoredClauseKind::Impossible){return retain_payload(right);}
    if(b.kind==StoredClauseKind::Impossible){return retain_payload(left);}
    if(a.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(left)).count==0u64){
        return retain_payload(left);
    }
    if(b.kind==StoredClauseKind::Facts&&unsafe_position<OrderedTree>(payload_fields(right)).count==0u64){
        return retain_payload(right);
    }
    if(a.kind==StoredClauseKind::Facts&&b.kind==StoredClauseKind::Facts){
        address x=payload_fields(left);
        address y=payload_fields(right);
        if(fact_tree_relation(x,y,1)!=0){return retain_payload(left);}
        if(fact_tree_relation(y,x,1)!=0){return retain_payload(right);}
    }
    if(left==right){return retain_payload(left);}
    if(contains_conjunct(a,left,right)){return retain_payload(right);}
    if(contains_conjunct(b,right,left)){return retain_payload(left);}
    if(a.kind==StoredClauseKind::Both&&b.kind==StoredClauseKind::Both){
        ClausePair x=clause_edges(left);
        ClausePair y=clause_edges(right);
        address common=address();
        address first=address();
        address second=address();
        if(x.first==y.first){
            common=x.first;first=x.second;second=y.second;
        }else if(x.first==y.second){
            common=x.first;first=x.second;second=y.first;
        }else if(x.second==y.first){
            common=x.second;first=x.first;second=y.second;
        }else if(x.second==y.second){
            common=x.second;first=x.first;second=y.first;
        }
        if(common!=address()){
            address nested=combine_clause_either(first,second,conflict);
            address result=combine_clause_both(common,nested,conflict);
            clause_release(nested);
            return result;
        }
    }
    return pair_of(StoredClauseKind::Either,0,left,right);
}

extern "C" u32 clause_possible(address value,cfn<u32,address> query) on cpu
    requires nonzero(query) {
    let node = clause_header(unsafe_position<address>(value));
    if (node.kind == StoredClauseKind::Impossible) { return 0; }
    if (node.kind == StoredClauseKind::Facts) { return 1; }
    return query(value);
}

bool case_possible_on_path(address path,address condition,cfn<u32,address> conflict,cfn<u32,address> query) on cpu
    requires nonzero(conflict)
    requires nonzero(query) {
    let joined = combine_clause_both(path, condition, conflict);
    let possible = clause_possible(address_of(joined), query) != 0;
    clause_release(joined);
    return possible;
}

extern "C" PathTruth condition_truth_in(address value,address path,cfn<u32,address> conflict,cfn<u32,address> query) on cpu
    requires nonzero(conflict)
    requires nonzero(query) {
    let cases = unsafe_position<ClausePair>(value);
    let premise = unsafe_position<address>(path);
    if (!case_possible_on_path(premise, cases.first, conflict, query)) { return PathTruth::True; }
    if (!case_possible_on_path(premise, cases.second, conflict, query)) { return PathTruth::False; }
    return PathTruth::Unknown;
}

extern "C" address extend_required_environment(address environment,address output,address additions) on cpu {
    if (unsafe_position<OrderedTree>(additions).count == 0u64) { return environment; }
    if (environment != output) { fact_tree_copy(environment, output); }
    FactCursor cursor = fact_tree_cursor(additions);
    while (true) {
        let key = fact_tree_next(additions, address_of(cursor), 0);
        if (key == address()) { return output; }
        fact_tree_insert(output, key);
    }
}

bool required_has_computed(address facts) on cpu {
    FactCursor cursor = fact_tree_cursor(facts);
    while (true) {
        let fact = fact_tree_next(facts, address_of(cursor), 0);
        if (fact == address()) { return false; }
        if (unsafe_position<FactKind>(fact) == FactKind::Index) {
            let domain = read_fact_fields(fact).payload;
            if (unsafe_position<DomainKind>(domain) == DomainKind::Computed) { return true; }
        }
    }
}

extern "C" u32 required_fact_closure(address context,address input,address output,RequiredClosureHost host) on cpu
    requires nonzero(host.evaluate)
    requires nonzero(host.extend_roots)
    requires nonzero(host.adopt)
    requires nonzero(host.discard)
    requires nonzero(host.close_computed)
    requires nonzero(host.numeric)
    requires nonzero(host.conflict) {
    address environment = input;
    while (true) {
        let round = host.evaluate(context, environment, output);
        if (round.additions == address()) { return 0; }
        environment = round.environment;
        let added = unsafe_position<OrderedTree>(round.additions).count != 0u64;
        if (added) {
            let close_roots = host.extend_roots(context) != 0 || required_has_computed(round.additions);
            if (unsafe_position<OrderedTree>(environment).count == 0u64) {
                environment = host.adopt(context, output);
            } else {
                environment = extend_required_environment(environment, output, round.additions);
                host.discard(context);
            }
            if (close_roots) { host.close_computed(context, environment); }
        } else { host.discard(context); }

        OrderedTree numeric = OrderedTree(address(), 0u64, 0u64, 0u64);
        let numeric_slot = address_of(numeric);
        if (host.numeric(context, environment, numeric_slot) == 0) { return 0; }
        let numeric_added = numeric.count != 0u64;
        environment = extend_required_environment(environment, output, numeric_slot);
        fact_tree_clear(numeric_slot);
        if (!added && !numeric_added) {
            if (environment != output) { fact_tree_copy(environment, output); }
            return 1;
        }
        if (host.conflict(context, environment) != 0) { return 0; }
    }
}

extern "C" address combine_clause_branch(address parent,address condition,u32 truth) on cpu {
    if(clause_header(parent).kind==StoredClauseKind::Impossible||
       clause_header(selected_case(condition,truth)).kind==StoredClauseKind::Impossible){
        return clause_impossible();
    }
    return pair_of(StoredClauseKind::Branch,truth,parent,condition);
}

struct ClauseWalk { OrderedTree pending; OrderedTree visited; };

void push_walk_payload(address pending,address payload) on cpu {
    address slot=ordered_tree_prepare(pending,size_of<address>());
    unsafe_position<address>(slot)=payload;
    unsafe_position<OrderedTree>(pending).count+=1u64;
}

address pop_walk_payload(address pending) on cpu {
    OrderedTree stack=unsafe_position<OrderedTree>(pending);
    u64 slot=stack.count-1u64;
    address row=unsafe_address(address_bits(stack.records)+slot*size_of<address>());
    unsafe_position<OrderedTree>(pending).count=slot;
    return unsafe_position<address>(row);
}

struct ClauseVisitKey { OrderedNode node; address payload; };
struct ClauseVisitSearch { u64 index; u64 parent; i32 order; };

address visit_key(OrderedTree tree,u64 index) on cpu {
    return unsafe_address(address_bits(tree.records)+(index-1u64)*size_of<ClauseVisitKey>());
}

ClauseVisitSearch find_clause(OrderedTree visited,address payload) on cpu {
    u64 index=visited.root;
    ClauseVisitSearch result=ClauseVisitSearch(0u64,0u64,i32(0));
    while(index!=0u64){
        ClauseVisitKey row=unsafe_position<ClauseVisitKey>(visit_key(visited,index));
        if(row.payload==payload){result.index=index;return result;}
        bool before=address_bits(payload)<address_bits(row.payload);
        result.parent=index;
        result.order=before ? -i32(1) : i32(1);
        index=before ? row.node.left : row.node.right;
    }
    return result;
}

void visit_clause(address visited,address payload,ClauseVisitSearch found) on cpu {
    address entry=ordered_tree_prepare(visited,size_of<ClauseVisitKey>());
    unsafe_position<ClauseVisitKey>(entry).payload=payload;
    ordered_tree_publish(visited,size_of<ClauseVisitKey>(),found.parent,found.order);
}

extern "C" void clause_walk_drop(address owner) on cpu {
    fact_tree_clear(address_of(unsafe_position<ClauseWalk>(owner).pending));
    fact_tree_clear(address_of(unsafe_position<ClauseWalk>(owner).visited));
}

extern "C" void clause_fact_walk_start(address owner,address context,cfn<address,address> next) on cpu
    requires nonzero(next) {
    address pending=address_of(unsafe_position<ClauseWalk>(owner).pending);
    while(true){
        address root=next(context);
        if(root==address()){break;}
        push_walk_payload(pending,root);
    }
    OrderedTree stack=unsafe_position<OrderedTree>(pending);
    u64 first=0u64;
    u64 limit=stack.count;
    while(first<limit){
        limit-=1u64;
        if(first==limit){break;}
        address left=unsafe_address(address_bits(stack.records)+first*size_of<address>());
        address right=unsafe_address(address_bits(stack.records)+limit*size_of<address>());
        address payload=unsafe_position<address>(left);
        unsafe_position<address>(left)=unsafe_position<address>(right);
        unsafe_position<address>(right)=payload;
        first+=1u64;
    }
}

extern "C" address clause_fact_walk_next(address owner) on cpu {
    address pending=address_of(unsafe_position<ClauseWalk>(owner).pending);
    address visited=address_of(unsafe_position<ClauseWalk>(owner).visited);
    while(unsafe_position<OrderedTree>(pending).count!=0u64){
        address payload=pop_walk_payload(pending);
        ClauseVisitSearch found=find_clause(unsafe_position<OrderedTree>(visited),payload);
        if(found.index!=0u64){continue;}
        visit_clause(visited,payload,found);
        StoredClause node=clause_header(payload);
        if(node.kind==StoredClauseKind::Facts){return payload_fields(payload);}
        if(node.kind==StoredClauseKind::Impossible){continue;}
        ClausePair edges=clause_edges(payload);
        address right=node.kind==StoredClauseKind::Branch ? selected_case(edges.second,node.truth) : edges.second;
        push_walk_payload(pending,right);
        push_walk_payload(pending,edges.first);
    }
    return address();
}

extern "C" u32 clause_input_identities(address owner,address context,u32 nested_domains,cfn<u32,address,address> accept) on cpu
    requires nonzero(accept) {
    while(true){
        address facts=clause_fact_walk_next(owner);
        if(facts==address()){return 0;}
        FactCursor cursor=fact_tree_cursor(facts);
        while(true){
            address fact=fact_tree_next(facts,address_of(cursor),0);
            if(fact==address()){break;}
            if(visit_fact_identities(fact,context,nested_domains,accept)!=0){return 1;}
        }
    }
}
