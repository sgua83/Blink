module bootstrap.operator_types;
import bootstrap.ir_dataflow;
import bootstrap.c_function;

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
extern "C" u32 integer_math(u32 extremum,usize count,MathQueries queries) on cpu
    requires nonzero(queries.numeric) requires nonzero(queries.argument) {
    if (extremum==0) { return 0; }
    usize index=0u64;
    while (index<count) {
        if (queries.numeric(queries.context,index)==0) { return 0; }
        MathArgument argument=queries.argument(queries.context,index);
        if (floating(argument.scalar) || argument.scalar==3) { return 0; }
        index+=1u64;
    }
    return extremum;
}
extern "C" MathShape floating_math(usize count,MathQueries queries) on cpu
    requires nonzero(queries.argument) requires nonzero(queries.width) {
    MathShape result=MathShape(0,queries.argument(queries.context,0u64).scalar,0u64);
    usize index=0u64;
    while (index<count) {
        usize width=queries.width(queries.context,index);
        if (width>result.width) { result.width=width; }
        index+=1u64;
    }
    index=0u64;
    while (index<count) {
        MathArgument argument=queries.argument(queries.context,index);
        if (!floating(argument.scalar) || argument.scalar!=result.scalar || argument.flags!=0) {
            result.status=154;return result;
        }
        usize width=queries.width(queries.context,index);
        if (width!=1u64 && width!=result.width) { result.status=154;return result; }
        index+=1u64;
    }
    return result;
}


extern "C" GeometryBuiltin source_geometry(address bytes,usize length) on cpu {
    if (source_spelling(bytes,length,"dot")) { return GeometryBuiltin::Dot; }
    if (source_spelling(bytes,length,"length")) { return GeometryBuiltin::Length; }
    if (source_spelling(bytes,length,"normalize")) { return GeometryBuiltin::Normalize; }
    if (source_spelling(bytes,length,"cross")) { return GeometryBuiltin::Cross; }
    return GeometryBuiltin::Unknown;
}
extern "C" GeometryPlan geometry_plan(GeometryBuiltin builtin,usize count,GeometryQueries queries) on cpu
    requires nonzero(queries.values.numeric) requires nonzero(queries.values.argument)
    requires nonzero(queries.values.width) requires nonzero(queries.same) requires nonzero(queries.float3) {
    GeometryPlan result=GeometryPlan(GeometryStatus::Valid,0,0u64,1u64);
    usize arity=builtin==GeometryBuiltin::Dot || builtin==GeometryBuiltin::Cross ? 2u64 : 1u64;
    if (count!=arity) { result.status=GeometryStatus::Arity;return result; }
    address context=queries.values.context;
    if (builtin==GeometryBuiltin::Cross) {
        for (side in 0..2) {
            if (queries.float3(context,usize(side))==0) {
                result.status=GeometryStatus::CrossType;result.side=side;return result;
            }
        }
        result.width=3u64;result.result_width=3u64;return result;
    }
    if (builtin==GeometryBuiltin::Dot && queries.same(context)==0) {
        result.status=GeometryStatus::Same;return result;
    }
    MathArgument argument=queries.values.argument(context,0u64);
    if (builtin==GeometryBuiltin::Dot) {
        if (argument.scalar!=0 || queries.values.numeric(context,0u64)==0) {
            result.status=GeometryStatus::DotType;return result;
        }
    } else if (argument.scalar!=0 || argument.flags!=0) {
        result.status=GeometryStatus::LengthType;return result;
    }
    result.width=queries.values.width(context,0u64);
    if (builtin==GeometryBuiltin::Normalize) { result.result_width=result.width; }
    return result;
}
GeometryOperand geometry_slot(u32 slot,u32 take) on cpu { return GeometryOperand(slot,0u64,0,take); }
GeometryOperand geometry_component(u32 slot,usize component,u32 take) on cpu { return GeometryOperand(slot,component,1,take); }
extern "C" void expand_geometry(GeometryBuiltin builtin,usize width,GeometryEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output) {
    if (builtin==GeometryBuiltin::Cross) {
        for (axis in 0..3) {
            usize i=usize((axis+1)%3);usize j=usize((axis+2)%3);
            emitter.emit(emitter.context,GeometryStep(GeometryAction::Multiply,
                geometry_component(0,i,0),geometry_component(1,j,0),2,1u64));
            u32 last=axis==2 ? 1 : 0;
            emitter.emit(emitter.context,GeometryStep(GeometryAction::Multiply,
                geometry_component(0,j,last),geometry_component(1,i,last),3,1u64));
            emitter.emit(emitter.context,GeometryStep(GeometryAction::Subtract,
                geometry_slot(2,1),geometry_slot(3,1),4,1u64));
            emitter.output(emitter.context,4,1);
        }
        return;
    }
    if (builtin==GeometryBuiltin::Dot) {
        emitter.emit(emitter.context,GeometryStep(GeometryAction::Multiply,
            geometry_slot(0,1),geometry_slot(1,1),2,width));
    } else {
        emitter.emit(emitter.context,GeometryStep(GeometryAction::Multiply,
            geometry_slot(0,0),geometry_slot(0,builtin==GeometryBuiltin::Length ? 1 : 0),2,width));
    }
    GeometryOperand first=geometry_component(2,0u64,width==1u64 ? 1 : 0);
    emitter.emit(emitter.context,GeometryStep(GeometryAction::Copy,first,first,3,1u64));
    usize index=1u64;
    while (index<width) {
        emitter.emit(emitter.context,GeometryStep(GeometryAction::Add,
            geometry_slot(3,1),geometry_component(2,index,index+1u64==width ? 1 : 0),3,1u64));
        index+=1u64;
    }
    if (builtin!=GeometryBuiltin::Dot) {
        emitter.emit(emitter.context,GeometryStep(GeometryAction::Sqrt,
            geometry_slot(3,1),geometry_slot(3,1),3,1u64));
        if (builtin==GeometryBuiltin::Normalize) {
            emitter.emit(emitter.context,GeometryStep(GeometryAction::Divide,
                geometry_slot(0,1),geometry_slot(3,1),3,width));
        }
    }
    emitter.output(emitter.context,3,0);
}

extern "C" MatrixBuiltin source_matrix_intrinsic(address bytes,usize length) on cpu {
    if (source_spelling(bytes,length,"transpose")) { return MatrixBuiltin::Transpose; }
    if (source_spelling(bytes,length,"determinant")) { return MatrixBuiltin::Determinant; }
    if (source_spelling(bytes,length,"inverse")) { return MatrixBuiltin::Inverse; }
    return MatrixBuiltin::Unknown;
}
extern "C" MatrixIntrinsicPlan matrix_intrinsic(MatrixBuiltin builtin,usize count,address context,
    cfn<MatrixIntrinsicInput,address> ensures memory(read) inspect) on cpu requires nonzero(inspect) {
    MatrixIntrinsicPlan result=MatrixIntrinsicPlan(MatrixIntrinsicStatus::Valid,0,0,0,0,0,0,0);
    if (count!=1u64) { result.status=MatrixIntrinsicStatus::Arity;return result; }
    MatrixIntrinsicInput input=inspect(context);
    if (input.present==0) { result.status=MatrixIntrinsicStatus::Matrix;return result; }
    result.rows=input.rows;result.columns=input.columns;
    if (builtin==MatrixBuiltin::Transpose) {
        result.result_rows=input.columns;result.result_columns=input.rows;
        result.outputs=input.rows*input.columns;result.transpose=1;
        return result;
    }
    if (input.rows!=input.columns) { result.status=MatrixIntrinsicStatus::Square;return result; }
    result.inverse=builtin!=MatrixBuiltin::Determinant ? 1 : 0;
    if (result.inverse!=0) {
        result.result_rows=input.rows;result.result_columns=input.columns;
        result.outputs=input.rows*input.columns;
    } else { result.outputs=1; }
    return result;
}
extern "C" void expand_transpose(u32 rows,u32 columns,address context,
    cfn<void,address,u32> output) on cpu requires nonzero(output) {
    for (column in 0..columns) {
        for (row in 0..rows) { output(context,row*columns+column); }
    }
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
    if(has(a,128)||has(b,128)){
        Plan result=matrix_type(op,a,a_literal,b,b_literal,queries);
        result.operation=binary_instruction(op,0);
        return result;
    }
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


extern "C" FixedProjection fixed_projection(u32 array, usize width, u32 known, usize index) on cpu {
    if (known != 0) {
        return FixedProjection(FixedProjectionKind::Components, index * width);
    }
    return FixedProjection(array != 0 ? FixedProjectionKind::Array : FixedProjectionKind::Vector, 0u64);
}

extern "C" SelectedComponents select_indexed_components(
    usize index, usize count, u32 paired, ComponentEmitter emitter
) on cpu requires nonzero(emitter.choice) requires nonzero(emitter.equal) requires nonzero(emitter.select) {
    ComponentChoice first = emitter.choice(emitter.context, 0u64);
    SelectedComponents selected = SelectedComponents(first.registers);
    usize ordinal = 1u64;
    while (ordinal < count) {
        ComponentChoice item = emitter.choice(emitter.context, ordinal);
        let test = emitter.equal(emitter.context, index, item.key);
        selected.registers[0] = emitter.select(emitter.context, test, item.registers[0], selected.registers[0]);
        if (paired != 0) {
            selected.registers[1] = emitter.select(emitter.context, test, item.registers[1], selected.registers[1]);
        }
        ordinal += 1u64;
    }
    return selected;
}

MatrixOperand read_matrix_slot(u32 slot) on cpu { return MatrixOperand(slot,0); }
MatrixOperand take_matrix_slot(u32 slot) on cpu { return MatrixOperand(slot,1); }

u32 first_matrix_axis(u32 axes,u32 width) on cpu {
    for (axis in 0..width) {
        if ((axes & (1 << u32(axis))) != 0) { return axis; }
    }
    return 0;
}

void expand_matrix_minor(u32 width,u32 size,u32 rows,u32 columns,u32 destination,u32 scratch,
                          MatrixEmitter emitter) on cpu requires nonzero(emitter.emit) {
    let row=first_matrix_axis(rows,width);
    let next_rows=rows & ~(1 << u32(row));
    if (size==1) {
        let source=read_matrix_slot(row*width+first_matrix_axis(columns,width));
        emitter.emit(emitter.context,MatrixStep(u32(MatrixAction::Copy),source,source,destination));
        return;
    }
    let next_row=size==2 ? first_matrix_axis(next_rows,width) : 0;
    u32 produced=0;
    for (column in 0..width) {
        let bit=1 << u32(column);
        if ((columns & bit)==0) { continue; }
        let next_columns=columns & ~bit;
        MatrixOperand minor;
        if (size==2) {
            minor=read_matrix_slot(next_row*width+first_matrix_axis(next_columns,width));
        } else {
            let child=scratch+size;
            expand_matrix_minor(width,size-1,next_rows,next_columns,child,child+1,emitter);
            minor=take_matrix_slot(child);
        }
        let element=read_matrix_slot(row*width+column);
        let product=MatrixStep(u32(MatrixAction::Multiply),element,minor,scratch+produced);
        emitter.emit(emitter.context,product);
        produced+=1;
    }
    // Lower every term before the ordered alternating sum, as in the original IR.
    for (term in 1..size) {
        let action=(term & 1)==0 ? MatrixAction::Add : MatrixAction::Subtract;
        let target=term+1==size ? destination : scratch;
        let combine=MatrixStep(u32(action),take_matrix_slot(scratch),take_matrix_slot(scratch+term),target);
        emitter.emit(emitter.context,combine);
    }
}

extern "C" void expand_square_matrix(u32 width,bool inverse,MatrixEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output) {
    let inputs=width*width;
    let axes=(1 << u32(width))-1;
    if (!inverse) {
        expand_matrix_minor(width,width,axes,axes,inputs,inputs+1,emitter);
        emitter.output(emitter.context,inputs);
        return;
        }

    let determinant=2*inputs;
    let scratch=determinant+1;
    for (row in 0..width) {
        let rows=axes & ~(1 << u32(row));
        for (column in 0..width) {
            let columns=axes & ~(1 << u32(column));
            let target=inputs+row*width+column;
            expand_matrix_minor(width,width-1,rows,columns,target,scratch,emitter);
            if (((row+column) & 1)!=0) {
                let value=take_matrix_slot(target);
                emitter.emit(emitter.context,MatrixStep(u32(MatrixAction::Negate),value,value,target));
            }
        }
    }
    for (column in 0..width) {
        let target=column==0 ? determinant : scratch;
        let product=MatrixStep(u32(MatrixAction::Multiply),read_matrix_slot(column),read_matrix_slot(inputs+column),target);
        emitter.emit(emitter.context,product);
        if (column!=0) {
            let sum=MatrixStep(u32(MatrixAction::Add),take_matrix_slot(determinant),take_matrix_slot(scratch),determinant);
            emitter.emit(emitter.context,sum);
        }
    }
    for (row in 0..width) {
        for (column in 0..width) {
            let numerator=take_matrix_slot(inputs+column*width+row);
            let quotient=MatrixStep(u32(MatrixAction::Divide),numerator,read_matrix_slot(determinant),scratch);
            emitter.emit(emitter.context,quotient);
            emitter.output(emitter.context,scratch);
        }
    }
}

extern "C" void expand_matrix_binary(Plan plan,u32 left_count,u32 input_count,MatrixEmitter emitter) on cpu
    requires nonzero(emitter.emit) requires nonzero(emitter.output) {
    let scratch=input_count;
    if (plan.kind==3 || plan.kind==5 || plan.kind==6) {
        for (component in 0..u32(plan.count)) {
            let left=read_matrix_slot(plan.kind==6 ? 0 : component);
            let right=read_matrix_slot(left_count+(plan.kind==5 ? 0 : component));
            emitter.emit(emitter.context,MatrixStep(plan.operation,left,right,scratch));
            emitter.output(emitter.context,scratch);
        }
        return;
    }
    let rows=plan.kind==8 ? 1 : u32(plan.rows);
    let columns=plan.kind==7 ? 1 : u32(plan.columns);
    let inner=plan.kind==4 ? u32(plan.inner) : (plan.kind==7 ? u32(plan.columns) : u32(plan.rows));
    for (row in 0..rows) {
        for (column in 0..columns) {
            let left_row=row*inner;
            let right_column=left_count+column;
            let left=read_matrix_slot(left_row);
            let right=read_matrix_slot(right_column);
            emitter.emit(emitter.context,MatrixStep(u32(MatrixAction::Multiply),left,right,scratch));
            for (index in 1..inner) {
                let next_left=read_matrix_slot(left_row+index);
                let next_right=read_matrix_slot(right_column+index*columns);
                let term=scratch+1;
                emitter.emit(emitter.context,MatrixStep(u32(MatrixAction::Multiply),next_left,next_right,term));
                emitter.emit(emitter.context,MatrixStep(u32(MatrixAction::Add),take_matrix_slot(scratch),take_matrix_slot(term),scratch));
            }
            emitter.output(emitter.context,scratch);
        }
    }
}

bool source_spelling<const N:u32>(address bytes, usize length, read array<u8,N> expected) on cpu {
    if (length != usize(N)) { return false; }
    for (index in 0..N) {
        if (unsafe_position<u8>(unsafe_address(address_bits(bytes) + u64(index))) != expected[index]) {
            return false;
        }
    }
    return true;
}
extern "C" AtomicBuiltin source_atomic(address bytes, usize length, bool statement) on cpu {
    if (source_spelling(bytes, length, "store")) {
        return AtomicBuiltin(AtomicSourceKind::Store, Access::Write, 3, 2);
    }
    if (statement) { return AtomicBuiltin(AtomicSourceKind::Unknown, Access::Value, 0, 0); }
    if (source_spelling(bytes, length, "load")) {
        return AtomicBuiltin(AtomicSourceKind::Load, Access::Read, 2, 1);
    }
    if (source_spelling(bytes, length, "compare_exchange")) {
        return AtomicBuiltin(AtomicSourceKind::CompareExchange, Access::InOut, 5, 3);
    }
    AtomicSourceKind kind;
    if (source_spelling(bytes, length, "add")) { kind = AtomicSourceKind::Add; }
    else if (source_spelling(bytes, length, "sub")) { kind = AtomicSourceKind::Sub; }
    else if (source_spelling(bytes, length, "exchange")) { kind = AtomicSourceKind::Exchange; }
    else if (source_spelling(bytes, length, "min")) { kind = AtomicSourceKind::Min; }
    else if (source_spelling(bytes, length, "max")) { kind = AtomicSourceKind::Max; }
    else if (source_spelling(bytes, length, "and")) { kind = AtomicSourceKind::And; }
    else if (source_spelling(bytes, length, "or")) { kind = AtomicSourceKind::Or; }
    else if (source_spelling(bytes, length, "xor")) { kind = AtomicSourceKind::Xor; }
    else { return AtomicBuiltin(AtomicSourceKind::Unknown, Access::Value, 0, 0); }
    return AtomicBuiltin(kind, Access::InOut, 3, 2);
}
extern "C" bool atomic_order_allowed(AtomicSourceKind kind, u32 order, u32 failure) on cpu {
    // Compiler MemoryOrder encodes acquire in bit 0 and release in bit 1.
    if (kind == AtomicSourceKind::Load) { return (order & 2) == 0; }
    if (kind == AtomicSourceKind::Store) { return (order & 1) == 0; }
    return (failure & 2) == 0 && (failure & order) == failure;
}

extern "C" SubgroupBuiltin source_subgroup(address bytes, usize length) on cpu {
    if (source_spelling(bytes, length, "lane")) { return SubgroupBuiltin(SubgroupOperation::Lane, 0, SubgroupRule::Index); }
    if (source_spelling(bytes, length, "size")) { return SubgroupBuiltin(SubgroupOperation::Size, 0, SubgroupRule::Index); }
    if (source_spelling(bytes, length, "id")) { return SubgroupBuiltin(SubgroupOperation::Id, 0, SubgroupRule::Index); }
    if (source_spelling(bytes, length, "elect")) { return SubgroupBuiltin(SubgroupOperation::Elect, 0, SubgroupRule::Elect); }
    if (source_spelling(bytes, length, "any")) { return SubgroupBuiltin(SubgroupOperation::Any, 1, SubgroupRule::Vote); }
    if (source_spelling(bytes, length, "all")) { return SubgroupBuiltin(SubgroupOperation::All, 1, SubgroupRule::Vote); }
    if (source_spelling(bytes, length, "count")) { return SubgroupBuiltin(SubgroupOperation::Count, 1, SubgroupRule::VoteCount); }
    if (source_spelling(bytes, length, "prefix_count")) { return SubgroupBuiltin(SubgroupOperation::PrefixCount, 1, SubgroupRule::VoteCount); }
    if (source_spelling(bytes, length, "broadcast_first")) { return SubgroupBuiltin(SubgroupOperation::BroadcastFirst, 1, SubgroupRule::Transfer); }
    if (source_spelling(bytes, length, "shuffle")) { return SubgroupBuiltin(SubgroupOperation::Shuffle, 2, SubgroupRule::Transfer); }
    if (source_spelling(bytes, length, "shuffle_xor")) { return SubgroupBuiltin(SubgroupOperation::ShuffleXor, 2, SubgroupRule::Transfer); }
    if (source_spelling(bytes, length, "shuffle_up")) { return SubgroupBuiltin(SubgroupOperation::ShuffleUp, 2, SubgroupRule::Transfer); }
    if (source_spelling(bytes, length, "shuffle_down")) { return SubgroupBuiltin(SubgroupOperation::ShuffleDown, 2, SubgroupRule::Transfer); }
    if (source_spelling(bytes, length, "sum")) { return SubgroupBuiltin(SubgroupOperation::Sum, 1, SubgroupRule::Arithmetic); }
    if (source_spelling(bytes, length, "exclusive_sum")) { return SubgroupBuiltin(SubgroupOperation::ExclusiveSum, 1, SubgroupRule::Arithmetic); }
    if (source_spelling(bytes, length, "min")) { return SubgroupBuiltin(SubgroupOperation::Min, 1, SubgroupRule::Arithmetic); }
    if (source_spelling(bytes, length, "max")) { return SubgroupBuiltin(SubgroupOperation::Max, 1, SubgroupRule::Arithmetic); }
    if (source_spelling(bytes, length, "bit_and")) { return SubgroupBuiltin(SubgroupOperation::BitAnd, 1, SubgroupRule::Bits); }
    if (source_spelling(bytes, length, "bit_or")) { return SubgroupBuiltin(SubgroupOperation::BitOr, 1, SubgroupRule::Bits); }
    if (source_spelling(bytes, length, "bit_xor")) { return SubgroupBuiltin(SubgroupOperation::BitXor, 1, SubgroupRule::Bits); }
    if (source_spelling(bytes, length, "exclusive_bit_and")) { return SubgroupBuiltin(SubgroupOperation::ExclusiveBitAnd, 1, SubgroupRule::Bits); }
    if (source_spelling(bytes, length, "exclusive_bit_or")) { return SubgroupBuiltin(SubgroupOperation::ExclusiveBitOr, 1, SubgroupRule::Bits); }
    if (source_spelling(bytes, length, "exclusive_bit_xor")) { return SubgroupBuiltin(SubgroupOperation::ExclusiveBitXor, 1, SubgroupRule::Bits); }
    return SubgroupBuiltin(SubgroupOperation::Lane, 0, SubgroupRule::Unknown);
}
extern "C" SubgroupResult subgroup_result(SubgroupRule rule, address ty, cfn<u32,address,SubgroupFact> fact) on cpu
    requires nonzero(fact) {
    if (rule == SubgroupRule::Index) { return SubgroupResult::U32; }
    if (rule == SubgroupRule::Elect) { return SubgroupResult::Bool; }
    if (rule == SubgroupRule::Vote) { return SubgroupResult::PredicateBool; }
    if (rule == SubgroupRule::VoteCount) { return SubgroupResult::PredicateU32; }
    if (rule == SubgroupRule::Transfer) {
        return fact(ty, SubgroupFact::Shape) != 0 ? SubgroupResult::Operand : SubgroupResult::BadShape;
    }
    if (fact(ty, SubgroupFact::Numeric) == 0) { return SubgroupResult::BadShape; }
    if (rule == SubgroupRule::Bits && fact(ty, SubgroupFact::Integer) == 0) { return SubgroupResult::BadBits; }
    return SubgroupResult::Operand;
}

bool collective_prefix<const N:u32>(address bytes, usize length, read array<u8,N> expected, inout usize prefix) on cpu {
    if (length < usize(N) || !source_spelling(bytes, usize(N), expected)) { return false; }
    prefix = usize(N);
    return true;
}

extern "C" CollectiveBuiltin source_collective(address bytes, usize length) on cpu {
    usize prefix = usize(0);
    CollectiveForm form;
    if (collective_prefix(bytes, length, "tile_reduce_", prefix)) { form = CollectiveForm::TileReduce; }
    else if (collective_prefix(bytes, length, "tile_scan_exclusive_", prefix)) { form = CollectiveForm::TileScan; }
    else if (collective_prefix(bytes, length, "reduce_segmented_", prefix)) { form = CollectiveForm::SegmentedReduce; }
    else if (collective_prefix(bytes, length, "reduce_", prefix)) { form = CollectiveForm::Reduce; }
    else if (collective_prefix(bytes, length, "scan_segmented_exclusive_", prefix)) { form = CollectiveForm::SegmentedScan; }
    else if (collective_prefix(bytes, length, "scan_exclusive_", prefix)) { form = CollectiveForm::Scan; }
    else { return CollectiveBuiltin(CollectiveForm::Unknown, CollectiveOperator::Sum); }

    address suffix = unsafe_address(address_bits(bytes) + u64(prefix));
    usize size = length - prefix;
    CollectiveOperator operation;
    if (source_spelling(suffix, size, "sum")) { operation = CollectiveOperator::Sum; }
    else if (source_spelling(suffix, size, "min")) { operation = CollectiveOperator::Min; }
    else if (source_spelling(suffix, size, "max")) { operation = CollectiveOperator::Max; }
    else if (source_spelling(suffix, size, "bit_and")) { operation = CollectiveOperator::BitAnd; }
    else if (source_spelling(suffix, size, "bit_or")) { operation = CollectiveOperator::BitOr; }
    else if (source_spelling(suffix, size, "bit_xor")) { operation = CollectiveOperator::BitXor; }
    else { return CollectiveBuiltin(CollectiveForm::Unknown, CollectiveOperator::Sum); }
    return CollectiveBuiltin(form, operation);
}

extern "C" bool collective_accepts(CollectiveOperator operation, bool numeric, bool integer) on cpu {
    return numeric && (integer || operation == CollectiveOperator::Sum
        || operation == CollectiveOperator::Min || operation == CollectiveOperator::Max);
}
