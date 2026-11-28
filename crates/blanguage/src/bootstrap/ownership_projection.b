module bootstrap.ownership_projection;

address at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
ExprNode expression(Source source,ExprRef reference) on cpu {return unsafe_position<ExprNode>(at(source.nodes,reference.offset));}
Bytes text(Source source,Text name) on cpu {return Bytes(at(name.kind==0 ? source.source : source.stored,name.range.start),name.range.end-name.range.start);}
extern "C" PlaceRoot place_root(Source source) on cpu {
    Bytes absent=Bytes(address(),0u64);
    ExprRef current=source.expression;
    while(true){
        ExprNode node=expression(source,current);
        match(node.kind){
            ExprKind::Name(name){return PlaceRoot(text(source,name),PlaceRootKind::Named);}
            ExprKind::Member(value){current=value.base;}
            ExprKind::Index(value){current=value.base;}
            ExprKind::Specialize(value){
                PlaceRootKind kind=unsafe_position_name(text(source,value.name)) ? PlaceRootKind::Address : PlaceRootKind::Temporary;
                return PlaceRoot(absent,kind);
            }
            ExprKind::Call(unused){return PlaceRoot(absent,PlaceRootKind::Temporary);}
            ExprKind::Invoke(unused){return PlaceRoot(absent,PlaceRootKind::Temporary);}
            ExprKind::Construct(unused){return PlaceRoot(absent,PlaceRootKind::Temporary);}
            ExprKind::Target(unused){return PlaceRoot(absent,PlaceRootKind::Temporary);}
            ExprKind::Select(unused){return PlaceRoot(absent,PlaceRootKind::Temporary);}
            ExprKind::Number(unused){break;}
            ExprKind::StaticU32(unused){break;}
            ExprKind::Bool(unused){break;}
            ExprKind::FunctionRef(unused){break;}
            ExprKind::Cooperate(unused){break;}
            ExprKind::Unary(unused){break;}
            ExprKind::Binary(unused){break;}
        }
    }
    return PlaceRoot(absent,PlaceRootKind::Other);
}
u8 byte(Bytes value,u64 index) on cpu {return unsafe_position<u8>(at(value.data,index));}
bool same(Bytes left,Bytes right) on cpu {
    if(left.count!=right.count){return false;}
    u64 i=0u64;
    while(i<left.count){if(byte(left,i)!=byte(right,i)){return false;}i+=1u64;}
    return true;
}
Projection absent() on cpu {return Projection(unsafe_address(0u64),Text(0,Span(0u64,0u64)),0);}
KnownIndex decimal(Bytes value) on cpu {
    u64 i=0u64;
    if(value.count!=0u64&&byte(value,0u64)==u8(43)){i=1u64;}
    if(i==value.count){return KnownIndex(0,0);}
    u32 result=0;
    while(i<value.count){
        u32 digit=u32(byte(value,i));
        if(digit<48||digit>57){return KnownIndex(0,0);}
        digit-=48;
        if(result>429496729||(result==429496729&&digit>5)){return KnownIndex(0,0);}
        result=result*10+digit;i+=1u64;
    }
    return KnownIndex(1,result);
}
KnownIndex index(Source source,ExprRef reference,address env,Host host) on cpu requires nonzero(host.index) {
    ExprNode node=expression(source,reference);
    match(node.kind){
        ExprKind::StaticU32(value){return KnownIndex(1,value);}
        ExprKind::Number(name){return decimal(text(source,name));}
        ExprKind::Name(name){return host.index(env,text(source,name));}
        ExprKind::Bool(unused){return KnownIndex(0,0);}
        ExprKind::FunctionRef(unused){return KnownIndex(0,0);}
        ExprKind::Call(unused){return KnownIndex(0,0);}
        ExprKind::Invoke(unused){return KnownIndex(0,0);}
        ExprKind::Specialize(unused){return KnownIndex(0,0);}
        ExprKind::Cooperate(unused){return KnownIndex(0,0);}
        ExprKind::Construct(unused){return KnownIndex(0,0);}
        ExprKind::Target(unused){return KnownIndex(0,0);}
        ExprKind::Member(unused){return KnownIndex(0,0);}
        ExprKind::Unary(unused){return KnownIndex(0,0);}
        ExprKind::Index(unused){return KnownIndex(0,0);}
        ExprKind::Binary(unused){return KnownIndex(0,0);}
        ExprKind::Select(unused){return KnownIndex(0,0);}
    }
}
enum ProjectionQuery:u32 { TypeOnly=0; Static=1; Loop=2; };
struct ProjectionWalk { Projection value; address logical; LoopPlace place; };
ProjectionWalk unlocated(address logical) on cpu {
    return ProjectionWalk(absent(),logical,LoopPlace::Missing);
}

ProjectionWalk project(Source source,ExprRef reference,address env,address path,Host host,ProjectionQuery query) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty) requires nonzero(host.field)
    requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push) {
    ExprNode node=expression(source,reference);
    match(node.kind){
        ExprKind::Name(name){
            address ty=host.root(env,text(source,name));
            address logical=query==ProjectionQuery::Static ? address() : ty;
            if(query==ProjectionQuery::TypeOnly){return unlocated(logical);}
            return ProjectionWalk(Projection(ty,name,0),logical,LoopPlace::Root);
        }
        ExprKind::Index(value){
            ProjectionWalk selected=project(source,value.base,env,path,host,query);
            address logical=selected.logical;
            if(address_bits(logical)!=0u64){logical=host.ty(logical).array;}
            address parent=selected.value.ty;
            if(address_bits(parent)==0u64){return unlocated(logical);}
            KnownIndex chosen=index(source,value.index,env,host);
            if(chosen.present==0){return unlocated(logical);}
            TypeInfo ty=host.ty(parent);
            if(address_bits(ty.array)==0u64 || u64(chosen.value)>=ty.length){return unlocated(logical);}
            Part part=host.element(parent,u64(chosen.value));
            selected.value.ty=part.ty;
            selected.value.offset+=part.offset;
            selected.logical=logical;
            selected.place=LoopPlace::Child;
            host.push(path,u64(chosen.value));
            return selected;
        }
        ExprKind::Member(value){
            ProjectionWalk selected=project(source,value.base,env,path,host,query);
            Bytes name=text(source,value.field);
            if(address_bits(selected.logical)!=0u64){
                selected.logical=host.selected_field(selected.logical,name);
            }
            if(address_bits(selected.value.ty)==0u64){return unlocated(selected.logical);}
            TypeInfo ty=host.ty(selected.value.ty);
            u64 i=0u64;
            while(i<ty.fields){
                Field field=host.field(selected.value.ty,i);
                if(same(name,field.name)){
                    selected.value.ty=field.ty;
                    selected.value.offset+=field.offset;
                    selected.place=LoopPlace::Child;
                    host.push(path,i);
                    return selected;
                }
                i+=1u64;
            }
            return unlocated(selected.logical);
        }
        ExprKind::Number(unused){return unlocated(address());}
        ExprKind::StaticU32(unused){return unlocated(address());}
        ExprKind::Bool(unused){return unlocated(address());}
        ExprKind::FunctionRef(unused){return unlocated(address());}
        ExprKind::Call(unused){return unlocated(address());}
        ExprKind::Invoke(unused){return unlocated(address());}
        ExprKind::Specialize(unused){return unlocated(address());}
        ExprKind::Cooperate(unused){return unlocated(address());}
        ExprKind::Construct(unused){return unlocated(address());}
        ExprKind::Target(unused){return unlocated(address());}
        ExprKind::Unary(unused){return unlocated(address());}
        ExprKind::Binary(unused){return unlocated(address());}
        ExprKind::Select(unused){return unlocated(address());}
    }
}
bool unsafe_position_name(Bytes value) on cpu {
    if(value.count!=15u64){return false;}
    return byte(value,0u64)==u8(117)&&byte(value,1u64)==u8(110)&&byte(value,2u64)==u8(115)
        &&byte(value,3u64)==u8(97)&&byte(value,4u64)==u8(102)&&byte(value,5u64)==u8(101)
        &&byte(value,6u64)==u8(95)&&byte(value,7u64)==u8(112)&&byte(value,8u64)==u8(111)
        &&byte(value,9u64)==u8(115)&&byte(value,10u64)==u8(105)&&byte(value,11u64)==u8(116)
        &&byte(value,12u64)==u8(105)&&byte(value,13u64)==u8(111)&&byte(value,14u64)==u8(110);
}
extern "C" Projection owned(Source source,address env,address path,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty) requires nonzero(host.field)
    requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push) {
    return project(source,source.expression,env,path,host,ProjectionQuery::Static).value;
}
extern "C" address owned_type(Source source,address env,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty) requires nonzero(host.field)
    requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push) {
    return project(source,source.expression,env,address(),host,ProjectionQuery::TypeOnly).logical;
}
extern "C" LoopSelection loop_projection(Source source,address context,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty) requires nonzero(host.field)
    requires nonzero(host.element) requires nonzero(host.selected_field) requires nonzero(host.push) {
    ProjectionWalk selected=project(source,source.expression,context,context,host,ProjectionQuery::Loop);
    return LoopSelection(selected.logical,selected.place);
}
extern "C" Part field_layout(address root,address fields,u64 count,Host host) on cpu
    requires nonzero(host.element) {
    Part selected=Part(0,root);
    u64 i=0u64;
    while(i<count){
        u64 index=unsafe_position<u64>(at(fields,i*8u64));
        Part child=host.element(selected.ty,index);
        selected.ty=child.ty;
        selected.offset+=child.offset;
        i+=1u64;
    }
    return selected;
}
extern "C" void components(address root,address selected,u32 offset,address output,Host host) on cpu
    requires nonzero(host.ty) requires nonzero(host.word) requires nonzero(host.push) {
    u64 count=host.ty(selected).components;
    if(count==0u64){return;}
    u32 first=offset+host.word(selected,0u64);
    // The value layout appends fields, array elements and variant words in byte order.
    u64 found=0u64;
    u64 end=host.ty(root).components;
    while(found<end){
        u64 middle=found+(end-found)/2u64;
        if(host.word(root,middle)<first){found=middle+1u64;}
        else{end=middle;}
    }
    u64 i=0u64;
    while(i<count){
        u32 word=offset+host.word(selected,i);
        while(host.word(root,found)!=word){found+=1u64;}
        host.push(output,found);i+=1u64;
    }
}
