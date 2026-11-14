module bootstrap.view_liveness;

address at(address data,u64 offset) on cpu {return unsafe_address(address_bits(data)+offset);}
T node<T>(StorageView storage,u64 offset) on cpu {return unsafe_position<T>(at(storage.nodes,offset));}
u64 item(StorageView storage,Slice values,u64 index) on cpu {return node<u64>(storage,values.offset+index*8u64);}
Bytes text(StorageView storage,Text value) on cpu {
    return Bytes(at(value.kind==0 ? storage.source : storage.text,value.range.start),value.range.end-value.range.start);
}
bool same(Bytes a,Bytes b) on cpu {
    if(a.length!=b.length){return false;}
    u64 i=0u64;
    while(i<a.length){if(unsafe_position<u8>(at(a.data,i))!=unsafe_position<u8>(at(b.data,i))){return false;}i+=1u64;}
    return true;
}
bool word<const N:u32 in {1,2,3,5,7}>(Bytes name,read array<u8,N> expected) on cpu {
    if(name.length!=u64(N)){return false;}
    for(i in 0..N){if(unsafe_position<u8>(at(name.data,u64(i)))!=expected[i]){return false;}}
    return true;
}
bool prefix(PathView a,PathView b,cfn<PathStep,address,u64> step) on cpu requires nonzero(step) {
    if(a.count>b.count){return false;}
    u64 i=0u64;
    while(i<a.count){
        PathStep left=step(a.path,i);PathStep right=step(b.path,i);
        if(left.kind!=right.kind){return false;}
        if(left.kind==0){if(!same(left.field,right.field)){return false;}}
        else {if(left.known!=right.known || (left.known!=0 && left.index!=right.index)){return false;}}
        i+=1u64;
    }
    return true;
}
extern "C" u32 retain_action(LocationView location,LocationView written,u32 name_only,
    cfn<PathStep,address,u64> step) on cpu requires nonzero(step) {
    if(!same(location.name,written.name)){return 1;}
    if(name_only!=0 || prefix(written.path,location.path,step)){return 0;}
    return prefix(location.path,written.path,step) ? 2 : 1;
}
void unite(Host host,address live,address other) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    address rows=host.iterate(other,0);
    Row r=host.next(rows);
    while(r.present!=0){
        address current=host.entry(live,r.key,r.value);
        if(address_bits(current)!=0u64){
            address result=host.make_paths();
            address left=host.iterate(current,1);Row a=host.next(left);
            while(a.present!=0){
                address right=host.iterate(r.value,1);Row b=host.next(right);
                while(b.present!=0){
                    PathView pa=PathView(a.key,a.count);
                    PathView pb=PathView(b.key,b.count);
                    if(prefix(pa,pb,host.step)){host.insert_path(result,b.key);}
                    else {if(prefix(pb,pa,host.step)){host.insert_path(result,a.key);}}
                    b=host.next(right);
                }
                host.drop_iterator(right);a=host.next(left);
            }
            host.drop_iterator(left);host.replace_paths(current,result);
        }
        r=host.next(rows);
    }
    host.drop_iterator(rows);
}
void kill(Host host,address live,address written) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    LocationView location=host.location(written);u64 i=0u64;
    while(i<location.path.count){PathStep part=host.step(location.path.path,i);if(part.kind==1 && part.known==0){return;}i+=1u64;}
    host.retain(live,written,0);
}
void kill_name(Host host,address live,Bytes name) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    address location=host.make_location(name);host.retain(live,location,1);host.drop_location(location);
}
address location(Host host,address context,StorageView storage,ExprRef expr) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    ExprNode value=node<ExprNode>(storage,expr.offset);
    match(value.kind){
        ExprKind::Name(name) {return host.make_location(text(storage,name));}
        ExprKind::Member(member) {address base=location(host,context,storage,member.base);
            if(address_bits(base)!=0u64){host.field(base,text(storage,member.field));}return base;}
        ExprKind::Index(index) {address base=location(host,context,storage,index.base);
            if(address_bits(base)!=0u64){host.index(base,host.literal(context,index.index));}return base;}
        ExprKind::Number(ignored) {return address();}
        ExprKind::StaticU32(ignored) {return address();}
        ExprKind::Bool(ignored) {return address();}
        ExprKind::Call(ignored) {return address();}
        ExprKind::Invoke(ignored) {return address();}
        ExprKind::Construct(ignored) {return address();}
        ExprKind::Specialize(ignored) {return address();}
        ExprKind::FunctionRef(ignored) {return address();}
        ExprKind::Cooperate(ignored) {return address();}
        ExprKind::Target(ignored) {return address();}
        ExprKind::Unary(ignored) {return address();}
        ExprKind::Binary(ignored) {return address();}
        ExprKind::Select(ignored) {return address();}
    }
}
void point(Host host,address context,Span span,address before,address after) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    address invocation=host.point_invocation(context,span);
    host.put_point(context,span,host.copy(before),after,invocation);
}
address address_use(Host host,address context,StorageView storage,ExprRef expr,address after,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    ExprNode value=node<ExprNode>(storage,expr.offset);address before;
    match(value.kind){
        ExprKind::Name(ignored) {before=host.copy(after);}
        ExprKind::Member(member) {before=address_use(host,context,storage,member.base,host.copy(after),outputs,broken,continued);}
        ExprKind::Index(index) {address live=expression(host,context,storage,index.index,host.copy(after),outputs,broken,continued);
            before=address_use(host,context,storage,index.base,live,outputs,broken,continued);}
        ExprKind::Number(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::StaticU32(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Bool(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Call(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Invoke(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Construct(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Specialize(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::FunctionRef(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Cooperate(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Target(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Unary(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Binary(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
        ExprKind::Select(ignored) {return expression(host,context,storage,expr,after,outputs,broken,continued);}
    }
    point(host,context,value.span,before,after);return before;
}
bool never(StorageView storage,FunctionNode declaration) on cpu {
    TypeNode result=node<TypeNode>(storage,declaration.result.offset);
    return word(text(storage,result.name),"never") && result.arguments.count==0u64 && result.dataset==0 && result.tile==0 && result.view==0 && result.count.present==0 && result.contracts.present==0;
}
address call(Host host,address context,StorageView storage,Text name,Slice args,Span span,address after,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    Bytes called=text(storage,name);Optional<SignatureNode> function=host.resolve(context,called);
    Slice params=Slice(0u64,0u64);StorageView formal_storage=storage;bool terminator=false;
    if(function.present!=0){
        formal_storage=function.value.storage;FunctionNode declaration=node<FunctionNode>(formal_storage,function.value.offset);
        params=declaration.params;terminator=never(formal_storage,declaration);
    }
    address invocation=terminator ? host.fresh() : host.copy(after);
    u64 count=params.count<args.count ? params.count : args.count;
    u64 i=0u64;
    while(i<count){
        ParamNode param=node<ParamNode>(formal_storage,item(formal_storage,params,i));
        TypeNode ty=node<TypeNode>(formal_storage,param.ty.offset);
        if(ty.dataset==0 && param.access==2){
            address place=location(host,context,storage,ExprRef(item(storage,args,i)));
            if(address_bits(place)!=0u64){kill(host,invocation,place);host.drop_location(place);}
        }
        i+=1u64;
    }
    i=0u64;
    while(i<count){
        ParamNode param=node<ParamNode>(formal_storage,item(formal_storage,params,i));
        TypeNode ty=node<TypeNode>(formal_storage,param.ty.offset);
        if(ty.dataset==0 && param.access==3){
            address place=location(host,context,storage,ExprRef(item(storage,args,i)));
            if(address_bits(place)!=0u64){host.read(invocation,place);}
        }
        i+=1u64;
    }
    if(word(called,"try")){unite(host,invocation,outputs);}
    host.put_point(context,span,host.fresh(),host.copy(after),host.copy(invocation));
    address live=invocation;i=args.count;
    while(i!=0u64){
        i-=1u64;bool place_only=word(called,"ordinal");
        if(i<params.count){ParamNode param=node<ParamNode>(formal_storage,item(formal_storage,params,i));TypeNode ty=node<TypeNode>(formal_storage,param.ty.offset);place_only=place_only || (ty.dataset==0 && param.access==2);}
        ExprRef arg=ExprRef(item(storage,args,i));
        if(place_only){live=address_use(host,context,storage,arg,live,outputs,broken,continued);}
        else {live=expression(host,context,storage,arg,live,outputs,broken,continued);}
    }
    if(function.present==0){host.read(live,host.make_location(called));}
    return live;
}
address arguments(Host host,address context,StorageView storage,Slice args,address incoming,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    address live=incoming;
    u64 i=args.count;
    while(i!=0u64){i-=1u64;live=expression(host,context,storage,ExprRef(item(storage,args,i)),live,outputs,broken,continued);}
    return live;
}
address expression(Host host,address context,StorageView storage,ExprRef expr,address after,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    ExprNode value=node<ExprNode>(storage,expr.offset);address place=location(host,context,storage,expr);address before;
    if(address_bits(place)!=0u64){
        address live=host.copy(after);host.read(live,place);before=address_use(host,context,storage,expr,live,outputs,broken,continued);
    } else {
        match(value.kind){
        ExprKind::Call(c) {before=call(host,context,storage,c.name,c.arguments,value.span,after,outputs,broken,continued);}
        ExprKind::Specialize(c) {before=call(host,context,storage,c.name,c.arguments,value.span,after,outputs,broken,continued);}
        ExprKind::Cooperate(c) {before=call(host,context,storage,c.name,c.arguments,value.span,after,outputs,broken,continued);}
        ExprKind::Invoke(c) {host.put_point(context,value.span,host.fresh(),host.copy(after),host.copy(after));
                address live=arguments(host,context,storage,c.arguments,host.copy(after),outputs,broken,continued);
                before=expression(host,context,storage,c.callee,live,outputs,broken,continued);}
        ExprKind::Construct(c) {before=arguments(host,context,storage,c.arguments,host.copy(after),outputs,broken,continued);}
        ExprKind::Target(c) {before=expression(host,context,storage,c.value,host.copy(after),outputs,broken,continued);}
        ExprKind::Unary(c) {before=expression(host,context,storage,c.value,host.copy(after),outputs,broken,continued);}
        ExprKind::Member(c) {before=expression(host,context,storage,c.base,host.copy(after),outputs,broken,continued);}
        ExprKind::Index(c) {address live=expression(host,context,storage,c.index,host.copy(after),outputs,broken,continued);
                before=expression(host,context,storage,c.base,live,outputs,broken,continued);}
        ExprKind::Binary(c) {address live=expression(host,context,storage,c.right,host.copy(after),outputs,broken,continued);
                Bytes op=text(storage,c.operation);if(word(op,"&&") || word(op,"||")){unite(host,live,after);}
                before=expression(host,context,storage,c.left,live,outputs,broken,continued);}
        ExprKind::Select(c) {address yes=expression(host,context,storage,c.yes,host.copy(after),outputs,broken,continued);
                address no=expression(host,context,storage,c.no,host.copy(after),outputs,broken,continued);
                unite(host,yes,no);host.release(no);before=expression(host,context,storage,c.condition,yes,outputs,broken,continued);}
        ExprKind::Number(ignored) {before=host.copy(after);}
        ExprKind::StaticU32(ignored) {before=host.copy(after);}
        ExprKind::Bool(ignored) {before=host.copy(after);}
        ExprKind::Name(ignored) {before=host.copy(after);}
        ExprKind::FunctionRef(ignored) {before=host.copy(after);}
    }
    }
    point(host,context,value.span,before,after);return before;
}
address edge(Host host,address live) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {return address_bits(live)==0u64 ? host.fresh() : host.copy(live);}
u32 boolean(StorageView storage,ExprRef expr) on cpu {
    ExprNode value=node<ExprNode>(storage,expr.offset);match(value.kind){
        ExprKind::Bool(v) {return v==0 ? 1 : 2;}
        ExprKind::Number(ignored) {return 0;}
        ExprKind::StaticU32(ignored) {return 0;}
        ExprKind::Name(ignored) {return 0;}
        ExprKind::Member(ignored) {return 0;}
        ExprKind::Index(ignored) {return 0;}
        ExprKind::Call(ignored) {return 0;}
        ExprKind::Invoke(ignored) {return 0;}
        ExprKind::Construct(ignored) {return 0;}
        ExprKind::Specialize(ignored) {return 0;}
        ExprKind::FunctionRef(ignored) {return 0;}
        ExprKind::Cooperate(ignored) {return 0;}
        ExprKind::Target(ignored) {return 0;}
        ExprKind::Unary(ignored) {return 0;}
        ExprKind::Binary(ignored) {return 0;}
        ExprKind::Select(ignored) {return 0;}
    }
}
address block(Host host,address context,StorageView storage,Slice body,address incoming,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    address live=incoming;
    u64 i=body.count;
    while(i!=0u64){i-=1u64;live=statement(host,context,storage,StmtRef(item(storage,body,i)),live,outputs,broken,continued);}
    return live;
}
address statement(Host host,address context,StorageView storage,StmtRef stmt,address after,address outputs,address broken,address continued) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    StmtNode value=node<StmtNode>(storage,stmt.offset);address before;
    match(value.kind){
        StmtKind::Declare(c) {before=host.copy(after);kill_name(host,before,text(storage,c.name));}
        StmtKind::Let(c) {address live=host.copy(after);kill_name(host,live,text(storage,c.name));before=expression(host,context,storage,c.value,live,outputs,broken,continued);}
        StmtKind::Assign(c) {address live=host.copy(after);address place=location(host,context,storage,c.target);
            if(address_bits(place)!=0u64){kill(host,live,place);host.drop_location(place);}
            live=expression(host,context,storage,c.value,live,outputs,broken,continued);
            if(!word(text(storage,c.operation),"=")){place=location(host,context,storage,c.target);if(address_bits(place)!=0u64){host.read(live,place);}}
            before=address_use(host,context,storage,c.target,live,outputs,broken,continued);}
        StmtKind::Expr(c) {before=expression(host,context,storage,c,host.copy(after),outputs,broken,continued);}
        StmtKind::Return(c) {before=host.copy(outputs);if(c.present!=0){before=expression(host,context,storage,c.value,before,outputs,broken,continued);}}
        StmtKind::Break() {before=edge(host,broken);}
        StmtKind::Continue() {before=edge(host,continued);}
        StmtKind::Seek(c) {before=expression(host,context,storage,c.value,edge(host,continued),outputs,broken,continued);}
        StmtKind::If(c) {address yes=block(host,context,storage,c.yes,host.copy(after),outputs,broken,continued);
            address no=block(host,context,storage,c.no,host.copy(after),outputs,broken,continued);
            u32 known=boolean(storage,c.condition);
            if(known==2){host.release(no);before=yes;}
            else {if(known==1){host.release(yes);before=no;}else{unite(host,yes,no);host.release(no);before=yes;}}
            before=expression(host,context,storage,c.condition,before,outputs,broken,continued);}
        StmtKind::Match(c) {address live=host.fresh();u64 i=0u64;
            while(i<c.arms.count){
                ArmNode arm=node<ArmNode>(storage,item(storage,c.arms,i));address branch=block(host,context,storage,arm.body,host.copy(after),outputs,broken,continued);
                if(arm.binding.present!=0){kill_name(host,branch,text(storage,arm.binding.value));}
                unite(host,live,branch);host.release(branch);i+=1u64;
            }
            before=expression(host,context,storage,c.value,live,outputs,broken,continued);}
        StmtKind::While(c) {u32 known=boolean(storage,c.condition);
            if(known==1){before=expression(host,context,storage,c.condition,host.copy(after),outputs,broken,continued);}
            else {
                address head=host.copy(after);
                while(true){
                    address live=block(host,context,storage,c.body,host.copy(head),outputs,after,head);
                    if(known!=2){unite(host,live,after);}
                    live=expression(host,context,storage,c.condition,live,outputs,broken,continued);
                    bool done=host.equal(live,head);host.release(head);head=live;if(done){break;}
                }
                before=head;
            }}
        StmtKind::For(c) {bool empty=false;
            if(c.end.present!=0){Optional<u32> begin=host.literal(context,c.start);Optional<u32> end=host.literal(context,c.end.value);empty=begin.present!=0 && end.present!=0 && begin.value>=end.value;}
            if(empty){address live=expression(host,context,storage,c.end.value,host.copy(after),outputs,broken,continued);before=expression(host,context,storage,c.start,live,outputs,broken,continued);}
            else {
                address head=host.copy(after);
                while(true){
                    address live=block(host,context,storage,c.body,host.copy(head),outputs,after,head);unite(host,live,after);
                    if(host.equal(live,head)){host.release(live);break;}host.release(head);head=live;
                }
                kill_name(host,head,text(storage,c.name));if(c.end.present!=0){head=expression(host,context,storage,c.end.value,head,outputs,broken,continued);}
                before=expression(host,context,storage,c.start,head,outputs,broken,continued);
            }}
        StmtKind::Parallel(c) {address live=block(host,context,storage,c.body,host.copy(after),outputs,address(),address());kill_name(host,live,text(storage,c.name));
            ParallelNode domain=node<ParallelNode>(storage,c.domain.offset);
            if(domain.selection.present!=0){live=expression(host,context,storage,domain.selection.value,live,outputs,broken,continued);}
            if(domain.count.present!=0){live=expression(host,context,storage,domain.count.value,live,outputs,broken,continued);}
            host.read(live,host.make_location(text(storage,domain.dataset)));before=live;}
        StmtKind::Phase() {before=host.copy(after);}
    }
    point(host,context,value.span,before,after);return before;
}
extern "C" void run(SignatureNode function,Host host,address context) on cpu 
    requires nonzero(host.fresh)
    requires nonzero(host.copy)
    requires nonzero(host.release)
    requires nonzero(host.equal)
    requires nonzero(host.make_location)
    requires nonzero(host.field)
    requires nonzero(host.index)
    requires nonzero(host.drop_location)
    requires nonzero(host.read)
    requires nonzero(host.retain)
    requires nonzero(host.entry)
    requires nonzero(host.iterate)
    requires nonzero(host.next)
    requires nonzero(host.drop_iterator)
    requires nonzero(host.make_paths)
    requires nonzero(host.insert_path)
    requires nonzero(host.replace_paths)
    requires nonzero(host.location)
    requires nonzero(host.step)
    requires nonzero(host.point_invocation)
    requires nonzero(host.put_point)
    requires nonzero(host.resolve)
    requires nonzero(host.literal)
    requires nonzero(host.finish) {
    StorageView storage=function.storage;FunctionNode declaration=node<FunctionNode>(storage,function.offset);
    address outputs=host.fresh();u64 i=0u64;
    while(i<declaration.params.count){ParamNode param=node<ParamNode>(storage,item(storage,declaration.params,i));if(param.access==2 || param.access==3){host.read(outputs,host.make_location(text(storage,param.name)));}i+=1u64;}
    address current=block(host,context,storage,declaration.body.value,host.copy(outputs),outputs,address(),address());
    host.finish(context,current,outputs);
}
