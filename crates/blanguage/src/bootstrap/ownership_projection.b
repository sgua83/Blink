module bootstrap.ownership_projection;

address at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
ExprNode expression(Source source,ExprRef reference) on cpu {return unsafe_position<ExprNode>(at(source.nodes,reference.offset));}
Bytes text(Source source,Text name) on cpu {return Bytes(at(name.kind==0 ? source.source : source.stored,name.range.start),name.range.end-name.range.start);}
extern "C" Bytes root_name(Source source) on cpu {
    ExprRef current=source.expression;
    while(true){
        ExprNode node=expression(source,current);
        match(node.kind){
            ExprKind::Name(name){return text(source,name);}
            ExprKind::Member(value){current=value.base;}
            ExprKind::Index(value){current=value.base;}
            ExprKind::Number(unused){break;}
            ExprKind::StaticU32(unused){break;}
            ExprKind::Bool(unused){break;}
            ExprKind::FunctionRef(unused){break;}
            ExprKind::Call(unused){break;}
            ExprKind::Invoke(unused){break;}
            ExprKind::Specialize(unused){break;}
            ExprKind::Cooperate(unused){break;}
            ExprKind::Construct(unused){break;}
            ExprKind::Target(unused){break;}
            ExprKind::Unary(unused){break;}
            ExprKind::Binary(unused){break;}
            ExprKind::Select(unused){break;}
        }
    }
    return Bytes(address(),0u64);
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
Projection project(Source source,ExprRef reference,address env,address path,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty)
    requires nonzero(host.field) requires nonzero(host.element) requires nonzero(host.push) {
    ExprNode node=expression(source,reference);
    match(node.kind){
        ExprKind::Name(name){return Projection(host.root(env,text(source,name)),name,0);}
        ExprKind::Index(value){
            Projection selected=project(source,value.base,env,path,host);
            if(address_bits(selected.ty)==0u64){return absent();}
            KnownIndex chosen=index(source,value.index,env,host);
            if(chosen.present==0){return absent();}
            TypeInfo ty=host.ty(selected.ty);
            if(address_bits(ty.array)==0u64||u64(chosen.value)>=ty.length){return absent();}
            Part part=host.element(selected.ty,u64(chosen.value));
            selected.ty=part.ty;selected.offset+=part.offset;host.push(path,u64(chosen.value));
            return selected;
        }
        ExprKind::Member(value){
            Projection selected=project(source,value.base,env,path,host);
            if(address_bits(selected.ty)==0u64){return absent();}
            TypeInfo ty=host.ty(selected.ty);Bytes name=text(source,value.field);u64 i=0u64;
            while(i<ty.fields){
                Field field=host.field(selected.ty,i);
                if(same(name,field.name)){selected.ty=field.ty;selected.offset+=field.offset;host.push(path,i);return selected;}
                i+=1u64;
            }
            return absent();
        }
        ExprKind::Number(unused){return absent();}
        ExprKind::StaticU32(unused){return absent();}
        ExprKind::Bool(unused){return absent();}
        ExprKind::FunctionRef(unused){return absent();}
        ExprKind::Call(unused){return absent();}
        ExprKind::Invoke(unused){return absent();}
        ExprKind::Specialize(unused){return absent();}
        ExprKind::Cooperate(unused){return absent();}
        ExprKind::Construct(unused){return absent();}
        ExprKind::Target(unused){return absent();}
        ExprKind::Unary(unused){return absent();}
        ExprKind::Binary(unused){return absent();}
        ExprKind::Select(unused){return absent();}
    }
}
address type_of(Source source,ExprRef reference,address env,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.selected_field) {
    ExprNode node=expression(source,reference);
    match(node.kind){
        ExprKind::Name(name){return host.root(env,text(source,name));}
        ExprKind::Member(value){
            address parent=type_of(source,value.base,env,host);
            if(address_bits(parent)==0u64){return parent;}
            return host.selected_field(parent,text(source,value.field));
        }
        ExprKind::Index(value){
            address parent=type_of(source,value.base,env,host);
            if(address_bits(parent)==0u64){return parent;}
            return host.ty(parent).array;
        }
        ExprKind::Number(unused){return unsafe_address(0u64);}
        ExprKind::StaticU32(unused){return unsafe_address(0u64);}
        ExprKind::Bool(unused){return unsafe_address(0u64);}
        ExprKind::FunctionRef(unused){return unsafe_address(0u64);}
        ExprKind::Call(unused){return unsafe_address(0u64);}
        ExprKind::Invoke(unused){return unsafe_address(0u64);}
        ExprKind::Specialize(unused){return unsafe_address(0u64);}
        ExprKind::Cooperate(unused){return unsafe_address(0u64);}
        ExprKind::Construct(unused){return unsafe_address(0u64);}
        ExprKind::Target(unused){return unsafe_address(0u64);}
        ExprKind::Unary(unused){return unsafe_address(0u64);}
        ExprKind::Binary(unused){return unsafe_address(0u64);}
        ExprKind::Select(unused){return unsafe_address(0u64);}
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
bool temporary_root(Source source,ExprRef reference) on cpu {
    ExprNode node=expression(source,reference);
    match(node.kind){
        ExprKind::Index(value){return temporary_root(source,value.base);}
        ExprKind::Member(value){return temporary_root(source,value.base);}
        ExprKind::Specialize(value){return !unsafe_position_name(text(source,value.name));}
        ExprKind::Call(unused){return true;}
        ExprKind::Invoke(unused){return true;}
        ExprKind::Construct(unused){return true;}
        ExprKind::Select(unused){return true;}
        ExprKind::Target(unused){return true;}
        ExprKind::Number(unused){return false;}
        ExprKind::StaticU32(unused){return false;}
        ExprKind::Bool(unused){return false;}
        ExprKind::Name(unused){return false;}
        ExprKind::FunctionRef(unused){return false;}
        ExprKind::Cooperate(unused){return false;}
        ExprKind::Unary(unused){return false;}
        ExprKind::Binary(unused){return false;}
    }
}
extern "C" Projection owned(Source source,address env,address path,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.index) requires nonzero(host.ty)
    requires nonzero(host.field) requires nonzero(host.element) requires nonzero(host.push) {
    return project(source,source.expression,env,path,host);
}
extern "C" address owned_type(Source source,address env,Host host) on cpu
    requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.selected_field) {
    return type_of(source,source.expression,env,host);
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
extern "C" bool temporary(Source source) on cpu {return temporary_root(source,source.expression);}
