module bootstrap.numeric_bounds;
import codec.scalar_ranges;
import bootstrap.fact_key;
import bootstrap.proof_facts;
import bootstrap.output_bindings;

address find_numeric_bound(address owner,FactView bound) on cpu {
    bool lower=bound.kind==FactKind::Minimum;
    FactView limit=bound;
    limit.value=lower ? 18446744073709551615u64 : 0u64;
    FactKey key;
    address raw=unsafe_address(address_bits(address_of(key)));
    materialize_fact(limit,raw);
    address found=conflict_fact_select(owner,raw,u32(lower),0);
    if(found==address()){return address();}
    FactView previous=read_fact_fields(found);
    if(previous.kind!=bound.kind||order_identity(previous.first,bound.first)!=0
        ||fact_scalar_tag(previous.scalar)!=fact_scalar_tag(bound.scalar)){return address();}
    return found;
}

extern "C" void numeric_bounds(address source,address roots,address output) on cpu {
    FactCursor cursor=fact_tree_cursor(source);
    while(true){
        address row=fact_tree_next(source,address_of(cursor),0);
        if(row==address()){return;}
        FactKind kind=unsafe_position<FactKind>(row);
        if(kind!=FactKind::Minimum&&kind!=FactKind::Maximum){continue;}
        FactView bound=read_fact_fields(row);
        if(roots!=address()){
            address root=find_output_binding(roots,bound.first);
            if(root!=address()){bound.first=root;}
        }
        address current=find_numeric_bound(output,bound);
        if(current!=address()){
            FactView previous=read_fact_fields(current);
            bool stronger=kind==FactKind::Minimum ? bound.value>previous.value : bound.value<previous.value;
            if(!stronger){continue;}
            fact_tree_remove(output,current);
        }
        FactKey key;
        address raw=unsafe_address(address_bits(address_of(key)));
        materialize_fact(bound,raw);
        fact_tree_insert(output,raw);
    }
}

extern "C" BitRange numeric_limits(address owner,address identity,address scalar) on cpu {
    address lower=find_numeric_bound(owner,fact_bound(identity,scalar,0u64,1));
    address upper=find_numeric_bound(owner,fact_bound(identity,scalar,0u64,0));
    return BitRange(lower==address() ? 0u64 : read_fact_fields(lower).value,
          upper==address() ? inspect_fact_scalar(scalar).mask : read_fact_fields(upper).value);
}
