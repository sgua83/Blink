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
    requires nonzero(host.copy_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.make_constant)
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
    requires nonzero(host.scalar_type)
    requires nonzero(host.fail)
{
    Literal decoded=number_value(context,text,negative,host);if(decoded.error!=LiteralError::None){return failed_type();}
    u32 scalar=literal_scalar(decoded.kind);return TypeResult(host.scalar_type(scalar,1u64),scalar_width(scalar)==32 ? 1 : 0,1);
}
Term number_term(address context,address original,Bytes text,bool negative,Host host) on cpu
    requires nonzero(host.copy_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.make_constant)
    requires nonzero(host.fail)
{
    Literal decoded=number_value(context,text,negative,host);if(decoded.error!=LiteralError::None){return failed();}
    u32 scalar=literal_scalar(decoded.kind);
    return scalar_term(original,scalar,decoded.bits,scalar_width(scalar)==32 ? 1 : 0,host);
}
TypeResult term_type(address context,Term term,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.path_type)
    requires nonzero(host.unary_type)
    requires nonzero(host.binary_type)
    requires nonzero(host.same_type)
    requires nonzero(host.fail)
{
    if(!empty(term.ty)){return TypeResult(host.copy_type(term.ty),term.literal,1);}
    return condition_type(context,term.value,host);
}
Term cast_term(Term term,u32 scalar,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.type_info)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.drop_bits)
    requires nonzero(host.make_constant)
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
extern "C" Result bind(address context,address value,u32 is_public,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.type_info)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_member)
    requires nonzero(host.make_unary)
    requires nonzero(host.make_binary)
    requires nonzero(host.parameter)
    requires nonzero(host.lookup)
    requires nonzero(host.evaluate)
    requires nonzero(host.variant)
    requires nonzero(host.member)
    requires nonzero(host.drop_indices)
    requires nonzero(host.fail)
{
    Node node=host.node(value);
    if(node.kind==0&&!returned_name(node.text)&&host.parameter(context,node.text)==0){
        Lookup found=host.lookup(context,node.text);
        if(found.found==0||found.constant==0){host.fail(context,1,node.text);return Result(nil(),0);}
        if(is_public!=0&&found.is_public==0){host.fail(context,2,node.text);return Result(nil(),0);}
        return host.evaluate(context,found.key);
    }
    if(node.kind==3){
        Term term=scalar_term(value,1,u64(node.flag),0,host);host.drop_type(term.ty);return Result(term.value,1);
    }
    if(node.kind==5){
        Result base=bind(context,node.left,is_public,host);if(base.ok==0){return base;}
        Node bound=host.node(base.value);if(bound.kind!=4){return Result(host.make_member(base.value,node.text),1);}
        Constant constant=host.constant(bound.left);TypeInfo info=host.type_info(constant.ty);
        address ty=nil();address bits=nil();
        if(info.is_variant!=0){
            Variant variant=host.variant(constant.ty,node.text);
            if(variant.found==0){host.drop_node(base.value);host.fail(context,3,node.text);return Result(nil(),0);}
            ty=host.scalar_type(3,1u64);bits=host.bits(1u64);
            host.push_bit(bits,bit_at(constant.bits,0u64)==u64(variant.discriminant) ? 1u64 : 0u64);
        }
        else if(info.is_array!=0&&length_name(node.text)){
            ty=host.scalar_type(1,1u64);bits=host.bits(1u64);host.push_bit(bits,info.length);
        }
        else {
            Member member=host.member(context,constant.ty,node.text);
            if(member.ok==0){host.drop_node(base.value);return Result(nil(),0);}
            ty=member.ty;bits=host.bits(member.count);
            u64 i=0u64;while(i<member.count){host.push_bit(bits,bit_at(constant.bits,bit_at(member.indices,i)));i+=1u64;}
            host.drop_indices(member.owner);
        }
        address selected=host.make_constant(ty,bits,bound.left);host.drop_node(base.value);return Result(selected,1);
    }
    if(node.kind==6){
        Result operand=bind(context,node.left,is_public,host);if(operand.ok==0){return operand;}
        return Result(host.make_unary(node.text,operand.value),1);
    }
    if(node.kind==7){
        Result left=bind(context,node.left,is_public,host);if(left.ok==0){return left;}
        Result right=bind(context,node.right,is_public,host);
        if(right.ok==0){host.drop_node(left.value);return right;}
        return Result(host.make_binary(node.text,left.value,right.value),1);
    }
    return Result(host.copy_node(value),1);
}
extern "C" Term identity(address context,address value,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.broadcast_bit)
    requires nonzero(host.type_info)
    requires nonzero(host.copy_node)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_node)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.bits)
    requires nonzero(host.push_bit)
    requires nonzero(host.drop_bits)
    requires nonzero(host.make_constant)
    requires nonzero(host.make_unary)
    requires nonzero(host.make_binary)
    requires nonzero(host.path_type)
    requires nonzero(host.unary_type)
    requires nonzero(host.binary_type)
    requires nonzero(host.same_type)
    requires nonzero(host.divide)
    requires nonzero(host.fail)
{
    Node node=host.node(value);
    if(node.kind==1){return number_term(context,value,node.text,false,host);}
    if(node.kind==2||node.kind==3){return scalar_term(value,node.kind==2 ? 3 : 1,u64(node.flag),0,host);}
    if(node.kind==4){return Term(host.copy_node(value),value,host.copy_type(host.constant(node.left).ty),0,1);}
    if(node.kind==0||node.kind==5){return Term(host.copy_node(value),value,nil(),0,1);}
    if(node.kind==6){
        Node original_operand=host.node(node.left);
        if(text_is(node.text,45,0,1u64)&&original_operand.kind==1){return number_term(context,value,original_operand.text,true,host);}
        Term operand=identity(context,node.left,host);if(operand.ok==0){return failed();}
        if(operand.literal!=0){
          Node folded=host.node(operand.value);if(folded.kind==4){
            TypeResult typed=term_type(context,operand,host);
            if(typed.ok==0){drop_term(operand,host);return failed();}
            UnaryRule rule=host.unary_type(context,node.text,typed.ty);
            if(rule.ok==0){host.drop_type(typed.ty);drop_term(operand,host);return failed();}
            Constant constant=host.constant(folded.left);TypeInfo info=host.type_info(typed.ty);address bits=host.bits(constant.count);
            u64 i=0u64;while(i<constant.count){host.push_bit(bits,unary_bits(rule.operation,info.width,bit_at(constant.bits,i)));i+=1u64;}
            drop_term(operand,host);return constant_term(value,typed.ty,bits,1,host);
          }
        }
        if(!empty(operand.ty)){host.drop_type(operand.ty);}
        address ty=nil();if(text_is(node.text,33,0,1u64)){ty=host.scalar_type(3,1u64);}
        return Term(host.make_unary(node.text,operand.value),value,ty,0,1);
    }
    Term left=identity(context,node.left,host);if(left.ok==0){return failed();}
    Term right=identity(context,node.right,host);if(right.ok==0){drop_term(left,host);return failed();}
    address ty=nil();u32 literal=0;
    if(comparison(node.text)||logical(node.text)){ty=host.scalar_type(3,1u64);}
    if(!logical(node.text)&&(left.literal!=0||right.literal!=0)){
        TypeResult a=term_type(context,left,host);
        if(a.ok==0){if(!empty(ty)){host.drop_type(ty);}drop_term(left,host);drop_term(right,host);return failed();}
        TypeResult b=term_type(context,right,host);
        if(b.ok==0){host.drop_type(a.ty);if(!empty(ty)){host.drop_type(ty);}drop_term(left,host);drop_term(right,host);return failed();}
        BinaryRule rule=host.binary_type(context,node.text,a.ty,left.literal,b.ty,right.literal);
        host.drop_type(a.ty);host.drop_type(b.ty);
        if(rule.ok==0){if(!empty(ty)){host.drop_type(ty);}drop_term(left,host);drop_term(right,host);return failed();}
        if(!empty(ty)){host.drop_type(ty);}ty=rule.ty;
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
extern "C" TypeResult condition_type(address context,address value,Host host) on cpu
    requires nonzero(host.node)
    requires nonzero(host.constant)
    requires nonzero(host.copy_type)
    requires nonzero(host.drop_type)
    requires nonzero(host.scalar_type)
    requires nonzero(host.path_type)
    requires nonzero(host.unary_type)
    requires nonzero(host.binary_type)
    requires nonzero(host.same_type)
    requires nonzero(host.fail)
{
    Node node=host.node(value);
    if(node.kind==0||node.kind==5){Result result=host.path_type(context,value);return TypeResult(result.value,0,result.ok);}
    if(node.kind==1){return number_type(context,node.text,false,host);}
    if(node.kind==2||node.kind==3){return TypeResult(host.scalar_type(node.kind==2 ? 3 : 1,1u64),0,1);}
    if(node.kind==4){return TypeResult(host.copy_type(host.constant(node.left).ty),0,1);}
    if(node.kind==6){
        Node operand=host.node(node.left);
        if(text_is(node.text,45,0,1u64)&&operand.kind==1){
            return number_type(context,operand.text,true,host);
        }
        TypeResult result=condition_type(context,node.left,host);if(result.ok==0){return result;}
        UnaryRule rule=host.unary_type(context,node.text,result.ty);
        if(rule.ok==0){host.drop_type(result.ty);return failed_type();}return result;
    }
    TypeResult a=condition_type(context,node.left,host);if(a.ok==0){return a;}
    TypeResult b=condition_type(context,node.right,host);if(b.ok==0){host.drop_type(a.ty);return b;}
    if(logical(node.text)){
        address boolean=host.scalar_type(3,1u64);u32 ok=host.same_type(context,boolean,a.ty);
        if(ok!=0){ok=host.same_type(context,boolean,b.ty);}host.drop_type(a.ty);host.drop_type(b.ty);
        if(ok==0){host.drop_type(boolean);return failed_type();}return TypeResult(boolean,0,1);
    }
    BinaryRule rule=host.binary_type(context,node.text,a.ty,a.literal,b.ty,b.literal);
    host.drop_type(a.ty);host.drop_type(b.ty);if(rule.ok==0){return failed_type();}
    u32 literal=rule.kind==0&&rule.comparison==0&&a.literal!=0&&b.literal!=0&&rule.scalar!=0&&rule.scalar!=6 ? 1 : 0;
    return TypeResult(rule.ty,literal,1);
}
