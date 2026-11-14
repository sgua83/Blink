module bootstrap.return_scope;

address at(address base,u64 offset) on cpu {return unsafe_address(address_bits(base)+offset);}
T node<T>(address nodes,u64 offset) on cpu {return unsafe_position<T>(at(nodes,offset));}
u64 member(address nodes,Slice list,u64 index) on cpu {return node<u64>(nodes,list.offset+index*8u64);}
bool is_try(address source,address stored,Text name) on cpu {
    if(name.range.end-name.range.start!=3u64){return false;}
    address bytes=at(name.kind==0 ? source : stored,name.range.start);
    return unsafe_position<u8>(bytes)==u8(116)
        && unsafe_position<u8>(at(bytes,1u64))==u8(114)
        && unsafe_position<u8>(at(bytes,2u64))==u8(121);
}
bool propagating_arguments(address nodes,address source,address stored,Slice arguments) on cpu {
    u64 i=0u64;
    while(i<arguments.count){
        if(propagating_expression(nodes,source,stored,ExprRef(member(nodes,arguments,i)))){return true;}
        i+=1u64;
    }
    return false;
}
bool propagating_expression(address nodes,address source,address stored,ExprRef reference) on cpu {
    ExprNode expression=node<ExprNode>(nodes,reference.offset);
    match(expression.kind){
        ExprKind::Number(unused){return false;}
        ExprKind::StaticU32(unused){return false;}
        ExprKind::Bool(unused){return false;}
        ExprKind::Name(unused){return false;}
        ExprKind::FunctionRef(unused){return false;}
        ExprKind::Call(call){return is_try(source,stored,call.name)||propagating_arguments(nodes,source,stored,call.arguments);}
        ExprKind::Invoke(call){return propagating_expression(nodes,source,stored,call.callee)||propagating_arguments(nodes,source,stored,call.arguments);}
        ExprKind::Specialize(call){return propagating_arguments(nodes,source,stored,call.arguments);}
        ExprKind::Cooperate(call){return propagating_arguments(nodes,source,stored,call.arguments);}
        ExprKind::Construct(call){return propagating_arguments(nodes,source,stored,call.arguments);}
        ExprKind::Target(value){return propagating_expression(nodes,source,stored,value.value);}
        ExprKind::Member(value){return propagating_expression(nodes,source,stored,value.base);}
        ExprKind::Unary(value){return propagating_expression(nodes,source,stored,value.value);}
        ExprKind::Index(value){return propagating_expression(nodes,source,stored,value.base)||propagating_expression(nodes,source,stored,value.index);}
        ExprKind::Binary(value){return propagating_expression(nodes,source,stored,value.left)||propagating_expression(nodes,source,stored,value.right);}
        ExprKind::Select(value){return propagating_expression(nodes,source,stored,value.condition)||propagating_expression(nodes,source,stored,value.yes)||propagating_expression(nodes,source,stored,value.no);}
    }
}
bool propagating_arms(address nodes,address source,address stored,Slice arms) on cpu {
    u64 i=0u64;
    while(i<arms.count){ArmNode arm=node<ArmNode>(nodes,member(nodes,arms,i));if(propagating_body(nodes,source,stored,arm.body)){return true;}i+=1u64;}
    return false;
}
bool propagating_body(address nodes,address source,address stored,Slice body) on cpu {
    u64 i=0u64;
    while(i<body.count){
        StmtNode statement=node<StmtNode>(nodes,member(nodes,body,i));
        match(statement.kind){
            StmtKind::Declare(unused){}
            StmtKind::Let(value){if(propagating_expression(nodes,source,stored,value.value)){return true;}}
            StmtKind::Assign(value){if(propagating_expression(nodes,source,stored,value.target)||propagating_expression(nodes,source,stored,value.value)){return true;}}
            StmtKind::Expr(value){if(propagating_expression(nodes,source,stored,value)){return true;}}
            StmtKind::If(value){if(propagating_expression(nodes,source,stored,value.condition)||propagating_body(nodes,source,stored,value.yes)||propagating_body(nodes,source,stored,value.no)){return true;}}
            StmtKind::Match(value){if(propagating_expression(nodes,source,stored,value.value)||propagating_arms(nodes,source,stored,value.arms)){return true;}}
            StmtKind::While(value){if(propagating_expression(nodes,source,stored,value.condition)||propagating_body(nodes,source,stored,value.body)){return true;}}
            StmtKind::For(value){
                if(propagating_expression(nodes,source,stored,value.start)){return true;}
                if(value.end.present!=0 && propagating_expression(nodes,source,stored,value.end.value)){return true;}
                if(propagating_body(nodes,source,stored,value.body)){return true;}
            }
            StmtKind::Parallel(value){
                ParallelNode domain=node<ParallelNode>(nodes,value.domain.offset);
                if(domain.count.present!=0 && propagating_expression(nodes,source,stored,domain.count.value)){return true;}
                if(domain.selection.present!=0 && propagating_expression(nodes,source,stored,domain.selection.value)){return true;}
                if(propagating_body(nodes,source,stored,value.body)){return true;}
            }
            StmtKind::Return(value){if(value.present!=0 && propagating_expression(nodes,source,stored,value.value)){return true;}}
            StmtKind::Break(){}
            StmtKind::Continue(){}
            StmtKind::Phase(){}
            StmtKind::Seek(value){if(propagating_expression(nodes,source,stored,value.value)){return true;}}
        }
        i+=1u64;
    }
    return false;
}
bool returning_arms(address nodes,Slice arms) on cpu {
    u64 i=0u64;
    while(i<arms.count){ArmNode arm=node<ArmNode>(nodes,member(nodes,arms,i));if(has_return(nodes,arm.body)){return true;}i+=1u64;}
    return false;
}
bool has_return(address nodes,Slice body) on cpu {
    u64 i=0u64;
    while(i<body.count){
        StmtNode statement=node<StmtNode>(nodes,member(nodes,body,i));
        match(statement.kind){
            StmtKind::Return(unused){return true;}
            StmtKind::If(value){if(has_return(nodes,value.yes)||has_return(nodes,value.no)){return true;}}
            StmtKind::Match(value){if(returning_arms(nodes,value.arms)){return true;}}
            StmtKind::While(value){if(has_return(nodes,value.body)){return true;}}
            StmtKind::For(value){if(has_return(nodes,value.body)){return true;}}
            StmtKind::Declare(unused){}
            StmtKind::Let(unused){}
            StmtKind::Assign(unused){}
            StmtKind::Expr(unused){}
            StmtKind::Parallel(unused){}
            StmtKind::Break(){}
            StmtKind::Continue(){}
            StmtKind::Phase(){}
            StmtKind::Seek(unused){}
        }
        i+=1u64;
    }
    return false;
}
extern "C" bool early_returns(address nodes,address source,address stored,Slice body) on cpu {
    if(propagating_body(nodes,source,stored,body)){return true;}
    u64 i=0u64;
    while(i<body.count){
        StmtNode statement=node<StmtNode>(nodes,member(nodes,body,i));
        match(statement.kind){
            StmtKind::Return(unused){if(i+1u64!=body.count){return true;}}
            StmtKind::If(value){if(has_return(nodes,value.yes)||has_return(nodes,value.no)){return true;}}
            StmtKind::Match(value){if(returning_arms(nodes,value.arms)){return true;}}
            StmtKind::While(value){if(has_return(nodes,value.body)){return true;}}
            StmtKind::For(value){if(has_return(nodes,value.body)){return true;}}
            StmtKind::Declare(unused){}
            StmtKind::Let(unused){}
            StmtKind::Assign(unused){}
            StmtKind::Expr(unused){}
            StmtKind::Parallel(unused){}
            StmtKind::Break(){}
            StmtKind::Continue(){}
            StmtKind::Phase(){}
            StmtKind::Seek(unused){}
        }
        i+=1u64;
    }
    return false;
}
