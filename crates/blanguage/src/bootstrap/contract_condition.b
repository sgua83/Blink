module bootstrap.contract_condition;

address nil() on cpu {return unsafe_address(0u64);}
bool empty(address value) on cpu {return address_bits(value)==0u64;}
u64 bit_at(address bits,u64 index) on cpu {return unsafe_position<u64>(unsafe_address(address_bits(bits)+8u64*index));}
bool text_is(Bytes text,u32 first,u32 second,u64 count) on cpu {
    if(text.count!=count){return false;}
    if(u32(unsafe_position<u8>(text.data))!=first){return false;}
    return count==1u64||u32(unsafe_position<u8>(unsafe_address(address_bits(text.data)+1u64)))==second;
}
bool returned_name(Bytes text) on cpu {
    if(text.count!=8u64){return false;}
    return unsafe_position<u64>(text.data)==7234309814316459378u64;
}
bool length_name(Bytes text) on cpu {
    if(text.count!=6u64){return false;}
    return unsafe_position<u32>(text.data)==1735288172&&unsafe_position<u16>(unsafe_address(address_bits(text.data)+4u64))==26740;
}
bool logical(Bytes op) on cpu {return text_is(op,38,38,2u64)||text_is(op,124,124,2u64);}
bool comparison(Bytes op) on cpu {
    return text_is(op,61,61,2u64)||text_is(op,33,61,2u64)||text_is(op,60,0,1u64)
        ||text_is(op,62,0,1u64)||text_is(op,60,61,2u64)||text_is(op,62,61,2u64);
}
u32 scalar_width(u32 scalar) on cpu {
    if(scalar==4||scalar==5||scalar==6){return 64;}
    if(scalar==7||scalar==8){return 8;}if(scalar==9||scalar==10){return 16;}return 32;
}
ScalarEncoding encoding(u32 scalar) on cpu {
    ScalarKind kind=ScalarKind::Unsigned;
    if(scalar==0||scalar==6){kind=ScalarKind::Float;}
    else if(scalar==2||scalar==5||scalar==8||scalar==10){kind=ScalarKind::Signed;}
    else if(scalar==3){kind=ScalarKind::Boolean;}
    return ScalarEncoding(kind,scalar_width(scalar));
}
u32 literal_scalar(LiteralKind kind) on cpu {
    if(kind==LiteralKind::F32){return 0;}if(kind==LiteralKind::F64){return 6;}
    if(kind==LiteralKind::U32){return 1;}if(kind==LiteralKind::U64){return 4;}
    if(kind==LiteralKind::I32){return 2;}return 5;
}
Term failed() on cpu {return Term(nil(),nil(),nil(),0,0);}
TypeResult failed_type() on cpu {return TypeResult(nil(),0,0);}
void drop_term(Term term,Host host) on cpu
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
{
    host.drop_node(term.value);if(!empty(term.ty)){host.drop_type(term.ty);}
}
Term constant_term(address original,address ty,address bits,u32 literal,Host host) on cpu
    requires nonzero(host.copy_type)
    requires nonzero(host.make_constant)
{
    address value=host.make_constant(host.copy_type(ty),bits,nil());
    return Term(value,original,ty,literal,1);
}
Term scalar_term(address original,u32 scalar,u64 bits,u32 literal,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.copy_type)
    requires nonzero(host.make_constant)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
{
    address values=host.bits(1u64);host.push_bit(values,bits);
    return constant_term(original,host.scalar_type(scalar,1u64),values,literal,host);
}
Literal number_value(address context,Bytes text,bool negative,Host host) on cpu
    requires nonzero(host.fail)
{
    Literal decoded=decode(text.data,text.count,negative);
    u32 scalar=literal_scalar(decoded.kind);
    if(decoded.error!=LiteralError::None){
        host.fail(context,100+u32(decoded.error)*16+scalar,text);
    }
    return decoded;
}
TypeResult number_type(address context,Bytes text,bool negative,Host host) on cpu
    requires nonzero(host.fail)
    requires nonzero(host.scalar_type)
{
    Literal decoded=number_value(context,text,negative,host);if(decoded.error!=LiteralError::None){return failed_type();}
    u32 scalar=literal_scalar(decoded.kind);return TypeResult(host.scalar_type(scalar,1u64),scalar_width(scalar)==32 ? 1 : 0,1);
}
Term number_term(address context,address original,Bytes text,bool negative,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.copy_type)
    requires nonzero(host.fail)
    requires nonzero(host.make_constant)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
{
    Literal decoded=number_value(context,text,negative,host);if(decoded.error!=LiteralError::None){return failed();}
    u32 scalar=literal_scalar(decoded.kind);
    return scalar_term(original,scalar,decoded.bits,scalar_width(scalar)==32 ? 1 : 0,host);
}
Term cast_term(Term term,u32 scalar,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.constant)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_bits)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.make_constant)
    requires nonzero(host.node)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
    requires nonzero(host.type_info)
{
    Node source_node=host.node(term.value);if(source_node.kind!=4){return term;}
    Constant value=host.constant(source_node.left);TypeInfo source=host.type_info(value.ty);
    address bits=host.bits(value.count);bool present=true;
    u64 i=0u64;while(i<value.count){
        u64 current=bit_at(value.bits,i);
        if(source.scalar==scalar){
            u32 width=scalar_width(scalar);if(width<64){current=current&((1u64<<u64(width))-1u64);}host.push_bit(bits,current);
        }
        else {
            FoldedCast casted=cast(encoding(source.scalar),encoding(scalar),current);
            if(casted.present==0){present=false;break;}host.push_bit(bits,casted.bits);
        }
        i+=1u64;
    }
    address ty=host.scalar_type(scalar,value.count);address result=nil();
    if(present){result=host.make_constant(host.copy_type(ty),bits,source_node.left);}
    else {host.drop_bits(bits);result=host.copy_node(term.original);}
    host.drop_node(term.value);if(!empty(term.ty)){host.drop_type(term.ty);}
    return Term(result,term.original,ty,term.literal,1);
}
u64 unary_bits(u32 operation,u32 width,u64 bits) on cpu {
    if(operation==0){return bits;}
    UnaryOperation op=UnaryOperation::Negate;
    if(operation==1){op=UnaryOperation::FloatNegate;}
    if(operation==4){op=UnaryOperation::LogicalNot;}
    if(operation==5){op=UnaryOperation::BitNot;}
    return unary(op,width,bits);
}
BinaryOperation binary_operation(u32 op) on cpu {
    if(op==1||op==5){return BinaryOperation::Add;}
    if(op==2||op==6){return BinaryOperation::Subtract;}
    if(op==3||op==7){return BinaryOperation::Multiply;}
    if(op==4){return BinaryOperation::Divide;}
    if(op==8){return BinaryOperation::Equal;}if(op==9){return BinaryOperation::NotEqual;}
    if(op==10){return BinaryOperation::Less;}if(op==11){return BinaryOperation::Greater;}
    if(op==12){return BinaryOperation::LessEqual;}if(op==13){return BinaryOperation::GreaterEqual;}
    if(op==14){return BinaryOperation::And;}if(op==15){return BinaryOperation::Or;}
    if(op==16){return BinaryOperation::Xor;}if(op==17){return BinaryOperation::ShiftLeft;}
    if(op==18){return BinaryOperation::ShiftRight;}return BinaryOperation::ShiftRightSigned;
}
Result bind_member(address context,address base,Bytes field,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.constant)
    requires nonzero(host.drop_indices)
    requires nonzero(host.drop_node)
    requires nonzero(host.fail)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_member)
    requires nonzero(host.member)
    requires nonzero(host.node)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
    requires nonzero(host.type_info)
    requires nonzero(host.variant)
{
        Node bound=host.node(base);if(bound.kind!=4){return Result(host.make_member(base,field),1);}
        Constant constant=host.constant(bound.left);TypeInfo info=host.type_info(constant.ty);
        address ty=nil();address bits=nil();
        if(info.is_variant!=0){
            Variant variant=host.variant(constant.ty,field);
            if(variant.found==0){host.drop_node(base);host.fail(context,3,field);return Result(nil(),0);}
            ty=host.scalar_type(3,1u64);bits=host.bits(1u64);
            host.push_bit(bits,bit_at(constant.bits,0u64)==u64(variant.discriminant) ? 1u64 : 0u64);
        }
        else if(info.is_array!=0&&length_name(field)){
            ty=host.scalar_type(1,1u64);bits=host.bits(1u64);host.push_bit(bits,info.length);
        }
        else {
            Member member=host.member(context,constant.ty,field);
            if(member.ok==0){host.drop_node(base);return Result(nil(),0);}
            ty=member.ty;bits=host.bits(member.count);
            u64 i=0u64;while(i<member.count){host.push_bit(bits,bit_at(constant.bits,bit_at(member.indices,i)));i+=1u64;}
            host.drop_indices(member.owner);
        }
        address selected=host.make_constant(ty,bits,bound.left);host.drop_node(base);return Result(selected,1);
}
Term unary_identity(address context,address value,Node node,Term operand_input,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.constant)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_unary)
    requires nonzero(host.node)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
    requires nonzero(host.type_info)
    requires nonzero(host.unary_type)
{
    Term operand=operand_input;
        if(operand.literal!=0){
          Node folded=host.node(operand.value);if(folded.kind==4){
            address typed=operand.ty;
            UnaryRule rule=host.unary_type(context,node.text,typed);
            if(rule.ok==0){drop_term(operand,host);return failed();}
            Constant constant=host.constant(folded.left);TypeInfo info=host.type_info(typed);address bits=host.bits(constant.count);
            u64 i=0u64;while(i<constant.count){host.push_bit(bits,unary_bits(rule.operation,info.width,bit_at(constant.bits,i)));i+=1u64;}
            operand.ty=nil();drop_term(operand,host);return constant_term(value,typed,bits,1,host);
          }
        }
        if(!empty(operand.ty)){host.drop_type(operand.ty);}
        address ty=nil();if(text_is(node.text,33,0,1u64)){ty=host.scalar_type(3,1u64);}
        return Term(host.make_unary(node.text,operand.value),value,ty,0,1);
}
Term binary_identity(address context,address value,Node node,Term left_input,Term right_input,address a,address b,Host host) on cpu
    requires nonzero(host.binary_type)
    requires nonzero(host.bits)
    requires nonzero(host.broadcast_bit)
    requires nonzero(host.constant)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.divide)
    requires nonzero(host.drop_bits)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.fail)
    requires nonzero(host.make_binary)
    requires nonzero(host.make_constant)
    requires nonzero(host.node)
    requires nonzero(host.push_bit)
    requires nonzero(host.scalar_type)
    requires nonzero(host.type_info)
{
    Term left=left_input;Term right=right_input;
    address ty=nil();u32 literal=0;
    bool typed=!logical(node.text)&&(left.literal!=0||right.literal!=0);
    if(!typed&&(comparison(node.text)||logical(node.text))){ty=host.scalar_type(3,1u64);}
    if(typed){
        BinaryRule rule=host.binary_type(context,node.text,a,left.literal,b,right.literal);
        if(rule.ok==0){drop_term(left,host);drop_term(right,host);return failed();}
        ty=rule.ty;
        if(rule.kind==0){
            literal=left.literal!=0&&right.literal!=0&&rule.scalar!=0&&rule.scalar!=6&&rule.comparison==0 ? 1 : 0;
            if(rule.cast_left!=0){left=cast_term(left,rule.scalar,host);}
            if(rule.cast_right!=0){right=cast_term(right,rule.scalar,host);}
            if(literal!=0){
              Node a_value=host.node(left.value);Node b_value=host.node(right.value);
              if(a_value.kind==4&&b_value.kind==4){
                TypeInfo info=host.type_info(ty);
                address bits=host.bits(info.components);
                u64 i=0u64;while(i<info.components){
                    u64 x=host.broadcast_bit(a_value.left,i);u64 y=host.broadcast_bit(b_value.left,i);u64 folded=0u64;
                    if(rule.operation!=0){
                        folded=binary(binary_operation(rule.operation),scalar_width(rule.scalar),encoding(rule.scalar).kind==ScalarKind::Signed ? 1 : 0,x,y);
                    }
                    else {
                        if(y==0u64){host.fail(context,4,node.text);host.drop_bits(bits);host.drop_type(ty);drop_term(left,host);drop_term(right,host);return failed();}
                        folded=host.divide(rule.scalar,x,y,text_is(node.text,37,0,1u64) ? 1 : 0);
                    }
                    host.push_bit(bits,folded);i+=1u64;
                }
                drop_term(left,host);drop_term(right,host);return constant_term(value,ty,bits,1,host);
              }
            }
        }
        else if(rule.kind==5&&rule.cast_right!=0){right=cast_term(right,0,host);}
        else if(rule.kind==6&&rule.cast_left!=0){left=cast_term(left,0,host);}
    }
    if(!empty(left.ty)){host.drop_type(left.ty);}if(!empty(right.ty)){host.drop_type(right.ty);}
    return Term(host.make_binary(node.text,left.value,right.value),value,ty,literal,1);
}
TypeResult binary_condition(address context,Node node,TypeResult a,TypeResult b,Host host) on cpu
    requires nonzero(host.binary_type)
    requires nonzero(host.drop_type)
    requires nonzero(host.same_type)
    requires nonzero(host.scalar_type)
{
    if(logical(node.text)){
        address boolean=host.scalar_type(3,1u64);u32 ok=host.same_type(context,boolean,a.ty);
        if(ok!=0){ok=host.same_type(context,boolean,b.ty);}
        if(ok==0){host.drop_type(boolean);return failed_type();}return TypeResult(boolean,0,1);
    }
    BinaryRule rule=host.binary_type(context,node.text,a.ty,a.literal,b.ty,b.literal);
    if(rule.ok==0){return failed_type();}
    u32 literal=rule.kind==0&&rule.comparison==0&&a.literal!=0&&b.literal!=0&&rule.scalar!=0&&rule.scalar!=6 ? 1 : 0;
    return TypeResult(rule.ty,literal,1);
}

extern "C" BindingCursor bind_start(address value,u32 is_public) on cpu {
    return BindingCursor(value,nil(),0,is_public);
}
QueryCursor query_start(address value) on cpu {return QueryCursor(value,nil(),0,0);}
extern "C" QueryCursor identity_start(address value) on cpu {return query_start(value);}
extern "C" QueryCursor type_start(address value) on cpu {return query_start(value);}
BindingNeed give_binding(address state,BindingCursor cursor,BindingNeed need) on cpu {
    unsafe_position<BindingCursor>(state)=cursor;return need;
}
IdentityNeed give_identity(address state,QueryCursor cursor,IdentityNeed need) on cpu {
    unsafe_position<QueryCursor>(state)=cursor;return need;
}
TypeNeed give_type(address state,QueryCursor cursor,TypeNeed need) on cpu {
    unsafe_position<QueryCursor>(state)=cursor;return need;
}
extern "C" BindingNeed bind_step(address state,address context,Host host) on cpu
    requires nonzero(host.bits)
    requires nonzero(host.constant)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_indices)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.fail)
    requires nonzero(host.lookup)
    requires nonzero(host.make_binary)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_member)
    requires nonzero(host.make_unary)
    requires nonzero(host.member)
    requires nonzero(host.node)
    requires nonzero(host.parameter)
    requires nonzero(host.push_bit)
    requires nonzero(host.put_bound)
    requires nonzero(host.scalar_type)
    requires nonzero(host.take_bound)
    requires nonzero(host.type_info)
    requires nonzero(host.variant)
{
    BindingCursor cursor=unsafe_position<BindingCursor>(state);
    Node node=host.node(cursor.source);
    if(cursor.stage==0){
        if(node.kind==0&&!returned_name(node.text)&&host.parameter(context,node.text)==0){
            Lookup found=host.lookup(context,node.text);
            if(found.found==0||found.constant==0){host.fail(context,1,node.text);return give_binding(state,cursor,BindingNeed::Done);}
            if(cursor.is_public!=0&&found.is_public==0){host.fail(context,2,node.text);return give_binding(state,cursor,BindingNeed::Done);}
            cursor.request=found.key;cursor.stage=3;
            return give_binding(state,cursor,BindingNeed::Constant);
        }
        if(node.kind==3){
            Term term=scalar_term(cursor.source,1,u64(node.flag),0,host);
            host.drop_type(term.ty);host.put_bound(context,0,term.value);
            return give_binding(state,cursor,BindingNeed::Done);
        }
        if(node.kind==5||node.kind==6||node.kind==7){
            cursor.request=node.left;cursor.stage=1;
            return give_binding(state,cursor,BindingNeed::Child);
        }
        host.put_bound(context,0,host.copy_node(cursor.source));
        return give_binding(state,cursor,BindingNeed::Done);
    }
    if(cursor.stage==1){
        if(node.kind==5){
            Result result=bind_member(context,host.take_bound(context,0),node.text,host);
            if(result.ok!=0){host.put_bound(context,0,result.value);}
            return give_binding(state,cursor,BindingNeed::Done);
        }
        if(node.kind==6){
            host.put_bound(context,0,host.make_unary(node.text,host.take_bound(context,0)));
            return give_binding(state,cursor,BindingNeed::Done);
        }
        host.put_bound(context,1,host.take_bound(context,0));
        cursor.request=node.right;cursor.stage=2;
        return give_binding(state,cursor,BindingNeed::Child);
    }
    if(cursor.stage==2){
        host.put_bound(context,0,host.make_binary(node.text,host.take_bound(context,1),host.take_bound(context,0)));
    }
    return give_binding(state,cursor,BindingNeed::Done);
}
void put_identity(address context,Term value,Host host) on cpu
    requires nonzero(host.put_term)
{
    if(value.ok!=0){host.put_term(context,0,value);}
}
extern "C" IdentityNeed identity_step(address state,address context,Host host) on cpu
    requires nonzero(host.binary_type)
    requires nonzero(host.bits)
    requires nonzero(host.broadcast_bit)
    requires nonzero(host.constant)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.divide)
    requires nonzero(host.drop_bits)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.fail)
    requires nonzero(host.make_binary)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_unary)
    requires nonzero(host.node)
    requires nonzero(host.push_bit)
    requires nonzero(host.put_term)
    requires nonzero(host.scalar_type)
    requires nonzero(host.take_term)
    requires nonzero(host.term)
    requires nonzero(host.term_type)
    requires nonzero(host.type_info)
    requires nonzero(host.unary_type)
{
    QueryCursor cursor=unsafe_position<QueryCursor>(state);
    Node node=host.node(cursor.source);
    if(cursor.stage==0){
        if(node.kind==1){put_identity(context,number_term(context,cursor.source,node.text,false,host),host);return give_identity(state,cursor,IdentityNeed::Done);}
        if(node.kind==2||node.kind==3){
            put_identity(context,scalar_term(cursor.source,node.kind==2 ? 3 : 1,u64(node.flag),0,host),host);
            return give_identity(state,cursor,IdentityNeed::Done);
        }
        if(node.kind==4){
            host.put_term(context,0,Term(host.copy_node(cursor.source),cursor.source,host.copy_type(host.constant(node.left).ty),0,1));
            return give_identity(state,cursor,IdentityNeed::Done);
        }
        if(node.kind==0||node.kind==5){
            host.put_term(context,0,Term(host.copy_node(cursor.source),cursor.source,nil(),0,1));
            return give_identity(state,cursor,IdentityNeed::Done);
        }
        if(node.kind==6){
            Node operand=host.node(node.left);
            if(text_is(node.text,45,0,1u64)&&operand.kind==1){
                put_identity(context,number_term(context,cursor.source,operand.text,true,host),host);
                return give_identity(state,cursor,IdentityNeed::Done);
            }
        }
        cursor.request=node.left;cursor.slot=0;cursor.stage=node.kind==6 ? 1 : 2;
        return give_identity(state,cursor,IdentityNeed::Child);
    }
    if(cursor.stage==1){
        put_identity(context,unary_identity(context,cursor.source,node,host.take_term(context,0),host),host);
        return give_identity(state,cursor,IdentityNeed::Done);
    }
    if(cursor.stage==2){
        cursor.request=node.right;cursor.slot=1;cursor.stage=3;
        return give_identity(state,cursor,IdentityNeed::Child);
    }
    if(cursor.stage==3){
        Term left=host.term(context,0);Term right=host.term(context,1);
        if(logical(node.text)||(left.literal==0&&right.literal==0)){
            put_identity(context,binary_identity(context,cursor.source,node,host.take_term(context,0),host.take_term(context,1),nil(),nil(),host),host);
            return give_identity(state,cursor,IdentityNeed::Done);
        }
        cursor.stage=4;
        if(empty(left.ty)){
            cursor.request=left.value;cursor.slot=0;
            return give_identity(state,cursor,IdentityNeed::Type);
        }
    }
    if(cursor.stage==4){
        cursor.stage=5;Term right=host.term(context,1);
        if(empty(right.ty)){
            cursor.request=right.value;cursor.slot=1;
            return give_identity(state,cursor,IdentityNeed::Type);
        }
    }
    address a=host.term_type(context,0);address b=host.term_type(context,1);
    put_identity(context,binary_identity(context,cursor.source,node,host.take_term(context,0),host.take_term(context,1),a,b,host),host);
    return give_identity(state,cursor,IdentityNeed::Done);
}
void put_condition(address context,TypeResult value,Host host) on cpu
    requires nonzero(host.put_type)
{
    if(value.ok!=0){host.put_type(context,0,value);}
}
extern "C" TypeNeed type_step(address state,address context,Host host) on cpu
    requires nonzero(host.binary_type)
    requires nonzero(host.clear_type)
    requires nonzero(host.constant)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_type)
    requires nonzero(host.fail)
    requires nonzero(host.node)
    requires nonzero(host.put_type)
    requires nonzero(host.same_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.typed)
    requires nonzero(host.unary_type)
{
    QueryCursor cursor=unsafe_position<QueryCursor>(state);
    Node node=host.node(cursor.source);
    if(cursor.stage==0){
        if(node.kind==0||node.kind==5){
            cursor.request=cursor.source;cursor.slot=0;cursor.stage=1;
            return give_type(state,cursor,TypeNeed::Path);
        }
        if(node.kind==1){put_condition(context,number_type(context,node.text,false,host),host);return give_type(state,cursor,TypeNeed::Done);}
        if(node.kind==2||node.kind==3){
            host.put_type(context,0,TypeResult(host.scalar_type(node.kind==2 ? 3 : 1,1u64),0,1));
            return give_type(state,cursor,TypeNeed::Done);
        }
        if(node.kind==4){
            host.put_type(context,0,TypeResult(host.copy_type(host.constant(node.left).ty),0,1));
            return give_type(state,cursor,TypeNeed::Done);
        }
        if(node.kind==6){
            Node operand=host.node(node.left);
            if(text_is(node.text,45,0,1u64)&&operand.kind==1){
                put_condition(context,number_type(context,operand.text,true,host),host);
                return give_type(state,cursor,TypeNeed::Done);
            }
        }
        cursor.request=node.left;cursor.slot=0;cursor.stage=node.kind==6 ? 2 : 3;
        return give_type(state,cursor,TypeNeed::Child);
    }
    if(cursor.stage==2){
        UnaryRule rule=host.unary_type(context,node.text,host.typed(context,0).ty);
        return give_type(state,cursor,TypeNeed::Done);
    }
    if(cursor.stage==3){
        cursor.request=node.right;cursor.slot=1;cursor.stage=4;
        return give_type(state,cursor,TypeNeed::Child);
    }
    if(cursor.stage==4){
        TypeResult result=binary_condition(context,node,host.typed(context,0),host.typed(context,1),host);
        if(result.ok!=0){host.put_type(context,0,result);host.clear_type(context,1);}
    }
    return give_type(state,cursor,TypeNeed::Done);
}

extern "C" u64 positional_index(Bytes name,u64 count) on cpu {
    if (name.count<=8u64) { return count; }
    array<u8,8> prefix="argument";
    for (index in 0..8) {
        u8 current=unsafe_position<u8>(unsafe_address(address_bits(name.data)+u64(index)));
        if (current!=prefix[index]) { return count; }
    }
    address suffix=unsafe_address(address_bits(name.data)+8u64);
    u8 first=unsafe_position<u8>(suffix);
    if (first<48 || first>57 || (first==48 && name.count>9u64)) { return count; }
    ParsedNumber ordinal=decimal_number(suffix,name.count-8u64,count);
    return ordinal.error==NumberError::None ? ordinal.value : count;
}
