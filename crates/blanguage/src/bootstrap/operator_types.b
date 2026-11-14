module bootstrap.operator_types;

Plan decision(u32 status) on cpu {return Plan(status,0,0,0,0,0,0,0u64,0u64,0u64,0u64);}
bool floating(u32 scalar) on cpu {return scalar==0||scalar==6;}
bool signed_integer(u32 scalar) on cpu {return scalar==2||scalar==5||scalar==8||scalar==10;}
bool has(Operand value,u32 flag) on cpu {return (value.flags&flag)!=0;}
u32 operation(address bytes,u64 length) on cpu {
    if(length==1u64){
        u8 first=unsafe_position<u8>(bytes);
        if(first==43){return 1;}if(first==45){return 2;}if(first==42){return 3;}
        if(first==47){return 4;}if(first==37){return 5;}if(first==38){return 12;}
        if(first==124){return 13;}if(first==94){return 14;}if(first==126){return 17;}
        if(first==33){return 18;}if(first==60){return 8;}if(first==62){return 9;}
    }
    if(length==2u64){
        u8 first=unsafe_position<u8>(bytes);
        u8 second=unsafe_position<u8>(unsafe_address(address_bits(bytes)+1u64));
        if(second==61){
            if(first==61){return 6;}if(first==33){return 7;}
            if(first==60){return 10;}if(first==62){return 11;}
        }
        if(first==60&&second==60){return 15;}
        if(first==62&&second==62){return 16;}
    }
    return 0;
}
extern "C" Plan numeric(Operand a,bool a_literal,Operand b,bool b_literal) on cpu {
    Plan result=decision(0);
    if(a.scalar==b.scalar){result.scalar=a.scalar;}
    else if(a_literal&&b_literal){
        result.scalar=a.scalar==0||b.scalar==0 ? 0 : 2;
        result.cast_left=1;result.cast_right=1;
    }
    else if(a_literal&&b.scalar!=3){result.scalar=b.scalar;result.cast_left=1;}
    else if(b_literal&&a.scalar!=3){result.scalar=a.scalar;result.cast_right=1;}
    else {return decision(148);}
    if(a.components!=b.components&&a.components!=1u64&&b.components!=1u64){return decision(149);}
    result.count=a.components>b.components ? a.components : b.components;
    return result;
}
Plan matrix_type(u32 op,Operand a,bool a_literal,Operand b,bool b_literal,Queries queries) on cpu
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric) {
    Plan result=decision(0);
    if(has(a,128)&&has(b,128)){
        if((op==1||op==2)&&a.rows==b.rows&&a.columns==b.columns){
            result.kind=3;result.count=a.rows*a.columns;return result;
        }
        if(op==3&&a.columns==b.rows){
            result.kind=4;result.rows=a.rows;result.inner=a.columns;result.columns=b.columns;return result;
        }
        return decision(2373);
    }
    if(has(a,128)){
        bool cast=b_literal&&b.components==1u64&&queries.numeric(queries.context,1);
        bool scalar=queries.builtin(queries.context,1,0,1u64)||cast;
        if(scalar&&(op==3||op==4)){
            result.kind=5;result.count=a.rows*a.columns;result.cast_right=cast ? 1 : 0;return result;
        }
        if(op==3&&queries.builtin(queries.context,1,0,a.columns)){
            result.kind=7;result.rows=a.rows;result.columns=a.columns;return result;
        }
        return decision(2371);
    }
    if(has(b,128)){
        bool cast=a_literal&&a.components==1u64&&queries.numeric(queries.context,0);
        bool scalar=queries.builtin(queries.context,0,0,1u64)||cast;
        if(op==3&&scalar){
            result.kind=6;result.count=b.rows*b.columns;result.cast_left=cast ? 1 : 0;return result;
        }
        if(op==3&&queries.builtin(queries.context,0,0,b.rows)){
            result.kind=8;result.rows=b.rows;result.columns=b.columns;return result;
        }
        return decision(2372);
    }
    return decision(2373);
}
u32 binary_instruction(u32 op,u32 scalar) on cpu {
    if(op==1){return floating(scalar) ? 1 : 5;}
    if(op==2){return floating(scalar) ? 2 : 6;}
    if(op==3){return floating(scalar) ? 3 : 7;}
    if(op==4&&floating(scalar)){return 4;}
    if(op>=6&&op<=11){return op+2;}
    if(op>=12&&op<=15){return op+2;}
    if(op==16){return signed_integer(scalar) ? 19 : 18;}
    return 0;
}
extern "C" Plan binary(address bytes,u64 length,Operand a,bool a_literal,Operand b,bool b_literal,Queries queries) on cpu
    requires nonzero(queries.same)
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric)
    requires nonzero(queries.views) {
    u32 op=operation(bytes,length);
    if(has(a,7)||has(b,7)){
        if(!queries.same(queries.context)){return decision(124);}
        if(op!=6&&op!=7){
            if(has(a,1)){return decision(1471);}
            if(has(a,2)){return decision(1472);}
            return decision(1473);
        }
        Plan result=decision(0);result.kind=has(a,4) ? 2 : 1;result.scalar=3;result.count=1u64;return result;
    }
    if(has(a,8)||has(b,8)){return decision(269);}
    bool comparison=op>=6&&op<=11;
    if(has(a,16)||has(b,16)){
        if(!queries.same(queries.context)){return decision(124);}
        if(!comparison){return decision(268);}
    }
    if(has(a,128)||has(b,128)){return matrix_type(op,a,a_literal,b,b_literal,queries);}
    if(a.components==0u64||b.components==0u64||has(a,96)||has(b,96)
        ||queries.views(queries.context,0)||queries.views(queries.context,1)){return decision(1474);}
    Plan result=numeric(a,a_literal,b,b_literal);
    if(result.status!=0){return result;}
    if(result.scalar==3&&op!=6&&op!=7){return decision(150);}
    if(floating(result.scalar)&&(op==5||(op>=12&&op<=16))){return decision(151);}
    result.comparison=comparison ? 1 : 0;
    if((op==4||op==5)&&!floating(result.scalar)){return result;}
    result.operation=binary_instruction(op,result.scalar);
    if(result.operation==0){return decision(178);}
    return result;
}
extern "C" Plan unary(address bytes,u64 length,Operand value,Queries queries) on cpu
    requires nonzero(queries.builtin)
    requires nonzero(queries.numeric) {
    u32 op=operation(bytes,length);
    if(op==18){if(!queries.builtin(queries.context,0,3,1u64)){return decision(124);}}
    else if(!queries.numeric(queries.context,0)&&!((op==1||op==2)&&has(value,128))){return decision(146);}
    Plan result=decision(0);
    if(op==1){return result;}
    if(op==2){
        result.operation=floating(value.scalar) ? 1 : (signed_integer(value.scalar) ? 2 : 3);
        return result;
    }
    if(op==18){result.operation=4;return result;}
    if(op==17&&!floating(value.scalar)){result.operation=5;return result;}
    return decision(177);
}

