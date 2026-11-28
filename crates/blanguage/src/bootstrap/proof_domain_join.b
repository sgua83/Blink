module bootstrap.proof_domain_join;
import bootstrap.proof_facts;
import bootstrap.fact_key;
import bootstrap.scalar_fold;
import codec.scalar_ranges;

struct DomainCursor { address domains; FactCursor position; };
struct UpperGroup { InternalUpper limit; InternalUpper following; };

InternalUpper next_upper(address input) on cpu {
    address domains=unsafe_position<DomainCursor>(input).domains;
    address cursor=address_of(unsafe_position<DomainCursor>(input).position);
    while(true){
        address key=domain_tree_next(domains,cursor,0);
        if(key==address()){return InternalUpper(address(),0,0);}
        InternalUpper limit=inspect_fact_upper(key);
        if(limit.present!=0){return limit;}
    }
}

UpperGroup upper_group(address input,InternalUpper first) on cpu {
    InternalUpper limit=first;
    InternalUpper following=next_upper(input);
    while(following.present!=0&&order_identity(first.identity,following.identity)==i32(0)){
        limit.distance=following.distance;
        following=next_upper(input);
    }
    return UpperGroup(limit,following);
}

bool has_origin(ProofDomainOperand operand,address limit) on cpu {
    usize index=0u64;
    while(index<operand.origin_count){
        address origin=unsafe_address(address_bits(operand.origins)+index*size_of<IdentityKey>());
        if(order_identity(origin,limit)==i32(0)){return true;}
        index+=1u64;
    }
    return false;
}

void add_upper(address output,address origin,u32 distance) on cpu {
    IndexDomainKey key;
    address raw=unsafe_address(address_bits(address_of(key)));
    write_domain_upper(raw,origin,distance);
    domain_tree_insert(output,raw);
}

void join_upper_domains(address output,ProofDomainOperand left,ProofDomainOperand right) on cpu {
    DomainCursor a=DomainCursor(left.domains,domain_tree_cursor(left.domains));
    DomainCursor b=DomainCursor(right.domains,domain_tree_cursor(right.domains));
    InternalUpper x=next_upper(address_of(a));
    InternalUpper y=next_upper(address_of(b));
    while(x.present!=0||y.present!=0){
        i32 order=x.present==0 ? i32(1) : y.present==0 ? -i32(1) : order_identity(x.identity,y.identity);
        if(order<0){
            UpperGroup row=upper_group(address_of(a),x);
            if(has_origin(right,row.limit.identity)){add_upper(output,row.limit.identity,0);}
            x=row.following;
        }else if(order>0){
            UpperGroup row=upper_group(address_of(b),y);
            if(has_origin(left,row.limit.identity)){add_upper(output,row.limit.identity,0);}
            y=row.following;
        }else{
            UpperGroup first=upper_group(address_of(a),x);
            UpperGroup second=upper_group(address_of(b),y);
            add_upper(output,first.limit.identity,min(first.limit.distance,second.limit.distance));
            x=first.following;
            y=second.following;
        }
    }
}

address first_join_scalar(address left,address right) on cpu {
    FactCursor a=domain_tree_cursor(left);
    FactCursor b=domain_tree_cursor(right);
    while(true){
        address row=domain_tree_merge(left,right,address_of(a),address_of(b),0);
        if(row==address()){return address();}
        DomainCastView bits=inspect_domain_cast(row);
        if(bits.kind==DomainKind::KnownBits){return bits.low;}
    }
}

extern "C" KnownDomain next_join_known_domain(address opaque) on cpu {
    address domains=unsafe_position<DomainCursor>(opaque).domains;
    address cursor=address_of(unsafe_position<DomainCursor>(opaque).position);
    while(true){
        address row=domain_tree_next(domains,cursor,0);
        if(row==address()){return KnownDomain(0u64,0u64,BitPattern(0u64,0u64),0);}
        DomainCastView bits=inspect_domain_cast(row);
        if(bits.kind==DomainKind::KnownBits){
            FactScalarView source=inspect_fact_scalar(bits.low);
            return KnownDomain(source.mask,source.bias,BitPattern(bits.mask,bits.value),1);
        }
    }
}

BitPattern proof_join_bits(ProofDomainOperand operand,FactScalarView scalar) on cpu {
    DomainCursor domains=DomainCursor(operand.domains,domain_tree_cursor(operand.domains));
    return known_proof(scalar.mask,scalar.bias,operand.ranges,operand.range_count,
        c_function(proof_range_record),address_of(domains),c_function(next_join_known_domain));
}

extern "C" u32 join_proof_domain_sets(address output,ProofDomainOperand left,ProofDomainOperand right) on cpu {
    FactCursor a=domain_tree_cursor(left.domains);
    FactCursor b=domain_tree_cursor(right.domains);
    while(true){
        address domain=domain_tree_merge(left.domains,right.domains,address_of(a),address_of(b),1);
        if(domain==address()){break;}
        if(unsafe_position<DomainKind>(domain)!=DomainKind::KnownBits){domain_tree_insert(output,domain);}
    }
    join_upper_domains(output,left,right);

    address scalar=first_join_scalar(left.domains,right.domains);
    if(scalar==address()){return 0;}
    FactScalarView encoding=inspect_fact_scalar(scalar);
    BitPattern first=proof_join_bits(left,encoding);
    if(first.mask==0u64){return 0;}
    BitPattern second=proof_join_bits(right,encoding);
    if(second.mask==0u64){return 0;}
    BitPattern common=known_common(encoding.mask,first,encoding.mask,encoding.bias,second);
    if(common.mask==0u64){return 0;}
    IndexDomainKey key;
    address raw=unsafe_address(address_bits(address_of(key)));
    unsafe_position<DomainKind>(raw)=DomainKind::KnownBits;
    write_known_bits_cast(raw,unsafe_position<u8>(scalar),common.mask,common.value);
    domain_tree_insert(output,raw);
    return u32(common.value!=0u64);
}
