module bootstrap.common_facts;
import codec.scalar_ranges;
import bootstrap.scalar_fold;
import bootstrap.fact_key;
import bootstrap.proof_facts;
import bootstrap.output_bindings;

address physical_key(address key) on cpu {
    return unsafe_address(address_bits(key));
}

bool contains_fact(address owner,FactView fact) on cpu {
    FactKey key;
    address raw=physical_key(address_of(key));
    materialize_fact(fact,raw);
    return fact_tree_contains(owner,raw)!=0;
}

void insert_fact(address owner,FactView fact) on cpu {
    FactKey key;
    address raw=physical_key(address_of(key));
    materialize_fact(fact,raw);
    fact_tree_insert(owner,raw);
}

FactCursor bounded_facts(address owner,FactView lower,FactView upper) on cpu {
    FactKey first;
    FactKey last;
    address low=physical_key(address_of(first));
    address high=physical_key(address_of(last));
    materialize_fact(lower,low);
    materialize_fact(upper,high);
    return fact_tree_range(owner,FactBound(low,0),FactBound(high,0));
}

FactView fact_at(address key) on cpu {
    if(key!=address()){return inspect_fact(key);}
    return FactView(0,FactKind::Equal,address(),address(),address(),address(),0u64,0u64,0,0);
}

FactView indexed_bits(address key,address identity,address scalar,u64 mask,u64 value) on cpu {
    unsafe_position<DomainKind>(key)=DomainKind::KnownBits;
    write_known_bits_cast(key,u8(u32(fact_scalar_tag(scalar))),mask,value);
    return FactView(1,FactKind::Index,identity,address(),key,address(),0u64,0u64,0,0);
}

struct KnownFactInput { address sets; u64 count; u64 next_set; address active; FactCursor cursor; address identity; address scalar; };

address fact_set(address sets,u64 index) on cpu {
    address slot=unsafe_address(address_bits(sets)+index*size_of<address>());
    return unsafe_position<address>(slot);
}

BitRange fact_limits(address owner,address identity,address scalar,u64 mask) on cpu {
    FactCursor minima=bounded_facts(owner,fact_bound(identity,scalar,0u64,1),fact_bound(identity,scalar,mask,1));
    FactCursor maxima=bounded_facts(owner,fact_bound(identity,scalar,0u64,0),fact_bound(identity,scalar,mask,0));
    FactView lower=fact_at(fact_tree_next(owner,address_of(minima),1));
    FactView upper=fact_at(fact_tree_next(owner,address_of(maxima),0));
    return BitRange(lower.present==0 ? 0u64 : lower.value,upper.present==0 ? mask : upper.value);
}

extern "C" KnownBounds read_fact_bounds(address context,u64 index) on cpu {
    KnownFactInput input=unsafe_position<KnownFactInput>(context);
    address owner=fact_set(input.sets,index);
    BitRange limits=fact_limits(owner,input.identity,input.scalar,inspect_fact_scalar(input.scalar).mask);
    FactView zero=FactView(1,FactKind::Zero,input.identity,address(),address(),address(),0u64,0u64,0,0);
    return KnownBounds(limits,u32(contains_fact(owner,zero)));
}

extern "C" KnownDomain next_fact_domain(address context) on cpu {
    while(true){
        KnownFactInput input=unsafe_position<KnownFactInput>(context);
        if(input.active!=address()){
            address cursor=address_of(unsafe_position<KnownFactInput>(context).cursor);
            address key=fact_tree_next(input.active,cursor,0);
            if(key!=address()){
                FactView fact=read_fact_fields(key);
                DomainCastView domain=inspect_domain_cast(fact.payload);
                if(domain.kind!=DomainKind::KnownBits){continue;}
                FactScalarView source=inspect_fact_scalar(domain.low);
                return KnownDomain(source.mask,source.bias,BitPattern(domain.mask,domain.value),1);
            }
        }
        if(input.next_set==input.count){return KnownDomain(0u64,0u64,BitPattern(0u64,0u64),0);}
        address owner=fact_set(input.sets,input.next_set);
        unsafe_position<KnownFactInput>(context).active=owner;
        unsafe_position<KnownFactInput>(context).next_set+=1u64;
        IndexDomainKey start;
        IndexDomainKey end;
        FactView lower=indexed_bits(physical_key(address_of(start)),input.identity,input.scalar,0u64,0u64);
        u64 maximum=18446744073709551615u64;
        FactView upper=indexed_bits(physical_key(address_of(end)),input.identity,input.scalar,maximum,maximum);
        unsafe_position<KnownFactInput>(context).cursor=bounded_facts(owner,lower,upper);
    }
}

extern "C" KnownProjection known_fact_values(address sets,u64 count,address identity,address scalar) on cpu {
    KnownFactInput input=KnownFactInput(sets,count,0u64,address(),FactCursor(0u64,0u64),identity,scalar);
    FactScalarView layout=inspect_fact_scalar(scalar);
    return known_facts(layout.mask,layout.bias,physical_key(address_of(input)),count,
                       c_function(read_fact_bounds),c_function(next_fact_domain));
}

struct RootGroup { OrderedNode links; IdentityKey left; IdentityKey right; IdentityKey representative; };

address root_group_at(OrderedTree tree,u64 index) on cpu {
    return unsafe_address(address_bits(tree.records)+(index-1u64)*size_of<RootGroup>());
}

address group_representative(address owner,address left,address right,address identity) on cpu {
    OrderedTree tree=unsafe_position<OrderedTree>(owner);
    u64 index=tree.root;
    u64 parent=0u64;
    i32 order=i32(0);
    while(index!=0u64){
        address slot=root_group_at(tree,index);
        order=order_identity(left,physical_key(address_of(unsafe_position<RootGroup>(slot).left)));
        if(order==0){order=order_identity(right,physical_key(address_of(unsafe_position<RootGroup>(slot).right)));}
        if(order==0){return physical_key(address_of(unsafe_position<RootGroup>(slot).representative));}
        parent=index;
        OrderedNode links=unsafe_position<RootGroup>(slot).links;
        index=order<0 ? links.left : links.right;
    }
    address slot=ordered_tree_prepare(owner,size_of<RootGroup>());
    unsafe_position<RootGroup>(slot).left=make_key_identity(capture_key_identity(left));
    unsafe_position<RootGroup>(slot).right=make_key_identity(capture_key_identity(right));
    unsafe_position<RootGroup>(slot).representative=make_key_identity(capture_key_identity(identity));
    ordered_tree_publish(owner,size_of<RootGroup>(),parent,order);
    return address();
}

void common_equalities(CommonFactInputs input,address output) on cpu {
    OrderedTree groups=OrderedTree(address(),0u64,0u64,0u64);
    address owner=address_of(groups);
    u64 i=0u64;
    u64 j=0u64;
    address left_roots=input.roots[0];
    address right_roots=input.roots[1];
    OutputBindingEntry left=next_output_binding(left_roots,address_of(i));
    OutputBindingEntry right=next_output_binding(right_roots,address_of(j));
    while(left.source!=address()&&right.source!=address()){
        i32 order=order_identity(left.source,right.source);
        if(order<0){left=next_output_binding(left_roots,address_of(i));continue;}
        if(order>0){right=next_output_binding(right_roots,address_of(j));continue;}
        address identity=left.source;
        address representative=left.target;
        if(order_identity(left.target,right.target)==0){
            if(order_identity(identity,representative)==0){
                left=next_output_binding(left_roots,address_of(i));
                right=next_output_binding(right_roots,address_of(j));
                continue;
            }
        }else{
            representative=group_representative(owner,left.target,right.target,identity);
        }
        left=next_output_binding(left_roots,address_of(i));
        right=next_output_binding(right_roots,address_of(j));
        if(representative==address()){continue;}
        bool ordered=order_identity(representative,identity)<=0;
        FactView equality=FactView(1,FactKind::Equal,ordered ? representative : identity,
            ordered ? identity : representative,address(),address(),0u64,0u64,0,0);
        if(!contains_fact(input.environment,equality)){insert_fact(output,equality);}
    }
    fact_tree_clear(owner);
}

void common_known_bits(CommonFactInputs input,address output) on cpu {
    address left=input.sides[0];
    address right=input.sides[1];
    array<FactCursor,2> cursors=array<FactCursor,2>(fact_tree_cursor(left),fact_tree_cursor(right));
    address previous=address();
    address previous_scalar=address();
    array<address,2> left_sets=array<address,2>(input.environment,left);
    array<address,2> right_sets=array<address,2>(input.environment,right);
    while(true){
        address key=fact_tree_merge(left,right,physical_key(address_of(cursors)),0);
        if(key==address()){break;}
        FactView fact=read_fact_fields(key);
        if(fact.kind!=FactKind::Index){continue;}
        DomainCastView domain=inspect_domain_cast(fact.payload);
        if(domain.kind!=DomainKind::KnownBits){continue;}
        address scalar=domain.low;
        if(previous!=address()&&order_identity(previous,fact.first)==0&&fact_scalar_tag(previous_scalar)==fact_scalar_tag(scalar)){continue;}
        previous=fact.first;
        previous_scalar=scalar;
        KnownProjection a=known_fact_values(physical_key(address_of(left_sets)),2u64,fact.first,scalar);
        if(a.present==0){continue;}
        KnownProjection b=known_fact_values(physical_key(address_of(right_sets)),2u64,fact.first,scalar);
        if(b.present==0){continue;}
        FactScalarView layout=inspect_fact_scalar(scalar);
        BitPattern bits=known_common(layout.mask,a.pattern,layout.mask,layout.bias,b.pattern);
        if(bits.mask!=0u64){
            IndexDomainKey result;
            FactView value=indexed_bits(physical_key(address_of(result)),fact.first,scalar,bits.mask,bits.value);
            if(!contains_fact(input.environment,value)){insert_fact(output,value);}
        }
    }
}

i32 limit_order(FactView left,FactView right) on cpu {
    if(left.kind!=right.kind){return left.kind==FactKind::Minimum ? -i32(1) : i32(1);}
    i32 order=order_identity(left.first,right.first);
    return order!=0 ? order : i32(u32(fact_scalar_tag(left.scalar)))-i32(u32(fact_scalar_tag(right.scalar)));
}

FactView next_bound(address owner,address cursor) on cpu {
    while(true){
        FactView value=fact_at(fact_tree_next(owner,cursor,0));
        if(value.present==0||value.kind==FactKind::Minimum||value.kind==FactKind::Maximum){return value;}
    }
}

struct Limit { FactView value; FactView next; };

Limit collect_limit(address owner,address cursor,FactView carry) on cpu {
    if(carry.present==0){return Limit(carry,carry);}
    FactView value=carry;
    FactView next=next_bound(owner,cursor);
    while(next.present!=0&&limit_order(value,next)==0){
        value.value=value.kind==FactKind::Minimum ? max(value.value,next.value) : min(value.value,next.value);
        next=next_bound(owner,cursor);
    }
    return Limit(value,next);
}

void common_limits(CommonFactInputs input,address output) on cpu {
    address left=input.sides[0];
    address right=input.sides[1];
    FactCursor i=fact_tree_cursor(left);
    FactCursor j=fact_tree_cursor(right);
    address left_cursor=address_of(i);
    address right_cursor=address_of(j);
    Limit a=collect_limit(left,left_cursor,next_bound(left,left_cursor));
    if(a.value.present==0){return;}
    Limit b=collect_limit(right,right_cursor,next_bound(right,right_cursor));
    while(a.value.present!=0&&b.value.present!=0){
        i32 order=limit_order(a.value,b.value);
        if(order<0){a=collect_limit(left,left_cursor,a.next);}
        else if(order>0){b=collect_limit(right,right_cursor,b.next);}
        else{
            FactView fact=a.value;
            FactView lower=fact;
            FactView upper=fact;
            if(fact.kind==FactKind::Maximum){
                fact.value=max(fact.value,b.value.value);
                lower.value=0u64;
                upper.value=fact.value;
            }else{
                fact.value=min(fact.value,b.value.value);
                lower.value=fact.value;
                upper.value=fact.mask;
            }
            FactCursor previous=bounded_facts(input.environment,lower,upper);
            if(previous.front==0u64){insert_fact(output,fact);}
            if(a.next.present==0||b.next.present==0){return;}
            a=collect_limit(left,left_cursor,a.next);
            b=collect_limit(right,right_cursor,b.next);
        }
    }
}

extern "C" void common_facts(CommonFactInputs input,address output) on cpu {
    address left=input.sides[0];
    address right=input.sides[1];
    array<FactCursor,2> cursors=array<FactCursor,2>(fact_tree_cursor(left),fact_tree_cursor(right));
    fact_tree_collect_merge(left,right,physical_key(address_of(cursors)),1,output);
    common_equalities(input,output);
    common_known_bits(input,output);
    common_limits(input,output);
}
