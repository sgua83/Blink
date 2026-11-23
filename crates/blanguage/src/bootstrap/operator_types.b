module bootstrap.operator_types;
import bootstrap.ir_dataflow;
import bootstrap.c_function;
import bootstrap.scalar_fold;

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
extern "C" AtomicBuiltin source_atomic_name(address bytes,usize length,bool statement) on cpu {
    if(length>=7u64&&source_spelling(bytes,7u64,"atomic_")){
        return source_atomic(unsafe_address(address_bits(bytes)+7u64),length-7u64,statement);
    }
    return AtomicBuiltin(AtomicSourceKind::Unknown,Access::Value,0,0);
}
extern "C" u32 source_custom_reduce(address bytes,usize length) on cpu {
    usize size=length;
    if(size>=4u64&&source_spelling(unsafe_address(address_bits(bytes)+u64(size)-4u64),4u64,"_map")){size-=4u64;}
    if(source_spelling(bytes,size,"reduce")){return 1;}
    if(source_spelling(bytes,size,"scan_exclusive")){return 2;}
    if(source_spelling(bytes,size,"scan_segmented_exclusive")){return 3;}
    if(source_spelling(bytes,size,"reduce_segmented")){return 4;}
    return 0;
}
BuiltinCallStep builtin_call_need(BuiltinCallCursor cursor,BuiltinCallNeed need) on cpu {
    return BuiltinCallStep(cursor,need,AtomicBuiltin(AtomicSourceKind::Unknown,Access::Value,0,0));
}
BuiltinCallStep builtin_call_values(BuiltinCallCursor cursor,address bytes,usize length) on cpu {
    if(source_spelling(bytes,length,"address_of")){return builtin_call_need(cursor,BuiltinCallNeed::AddressOf);}
    BuiltinCallCursor c=cursor;c.phase=3;c.index=0u64;
    return builtin_call_need(c,BuiltinCallNeed::Values);
}
extern "C" BuiltinCallStep builtin_call_step(BuiltinCallCursor cursor,BuiltinCallFacts facts,address bytes,usize length) on cpu {
    BuiltinCallCursor c=cursor;
    if(c.phase==0){
        if(source_spelling(bytes,length,"c_function")){return builtin_call_need(c,BuiltinCallNeed::CFunction);}
        if(source_spelling(bytes,length,"Ok")||source_spelling(bytes,length,"Err")){return builtin_call_need(c,BuiltinCallNeed::ResultContext);}
        if(source_spelling(bytes,length,"upload")||source_spelling(bytes,length,"download")){return builtin_call_need(c,BuiltinCallNeed::Transfer);}
        if(source_spelling(bytes,length,"println")){return builtin_call_need(c,BuiltinCallNeed::Println);}
        if(source_spelling(bytes,length,"texture_sample")){return builtin_call_need(c,BuiltinCallNeed::TextureSample);}
        if(source_spelling(bytes,length,"append")){return builtin_call_need(c,BuiltinCallNeed::Append);}
        c.collective=source_collective(bytes,length).form;
        if(c.collective==CollectiveForm::TileReduce||c.collective==CollectiveForm::TileScan){return builtin_call_need(c,BuiltinCallNeed::TileCollective);}
        if(source_spelling(bytes,length,"tile_publish")){return builtin_call_need(c,BuiltinCallNeed::TilePublish);}
        if(source_spelling(bytes,length,"tile_input")){return builtin_call_need(c,BuiltinCallNeed::TileInput);}
        c.phase=1;return builtin_call_need(c,BuiltinCallNeed::Subgroup);
    }
    if(c.phase==1){
        if(facts.present!=0){return builtin_call_need(c,BuiltinCallNeed::SubgroupResult);}
        if(source_spelling(bytes,length,"trace_closest")||source_spelling(bytes,length,"trace_any")){return builtin_call_need(c,BuiltinCallNeed::RayQuery);}
        if(source_spelling(bytes,length,"unsafe_read")){return builtin_call_need(c,BuiltinCallNeed::UnsafeRead);}
        if(source_spelling(bytes,length,"ordinal")){return builtin_call_need(c,BuiltinCallNeed::Ordinal);}
        if(source_spelling(bytes,length,"index")){
            return builtin_call_need(c,facts.count!=0u64||facts.in_function!=0 ? BuiltinCallNeed::IndexScope : BuiltinCallNeed::Index);
        }
        if(source_spelling(bytes,length,"bind")||source_spelling(bytes,length,"barrier")||source_spelling(bytes,length,"group_barrier")
            ||source_spelling(bytes,length,"compact")||source_spelling(bytes,length,"stable_sort_by_key")||source_spelling(bytes,length,"ddx")||source_spelling(bytes,length,"ddy")
            ||source_custom_reduce(bytes,length)!=0||c.collective==CollectiveForm::Reduce||c.collective==CollectiveForm::SegmentedReduce
            ||c.collective==CollectiveForm::Scan||c.collective==CollectiveForm::SegmentedScan){return builtin_call_need(c,BuiltinCallNeed::Unsupported);}
        AtomicBuiltin atomic=source_atomic_name(bytes,length,false);
        if(atomic.kind==AtomicSourceKind::Store){return builtin_call_need(c,BuiltinCallNeed::AtomicStore);}
        if(atomic.kind!=AtomicSourceKind::Unknown){return BuiltinCallStep(c,BuiltinCallNeed::Atomic,atomic);}
        if(source_spelling(bytes,length,"tile_barrier")){return builtin_call_need(c,BuiltinCallNeed::TileBarrier);}
        if(facts.record==2){return builtin_call_need(c,BuiltinCallNeed::RecordValues);}
        if(facts.record==0){c.phase=2;return builtin_call_need(c,BuiltinCallNeed::RecordLookup);}
        return builtin_call_values(c,bytes,length);
    }
    if(c.phase==2){
        if(facts.present!=0){return builtin_call_need(c,BuiltinCallNeed::RecordExpression);}
        return builtin_call_values(c,bytes,length);
    }
    if(c.phase==4){
        if(facts.present==0){return builtin_call_need(c,BuiltinCallNeed::Empty);}
        c.phase=5;return builtin_call_need(c,BuiltinCallNeed::ArgumentValue);
    }
    if(c.phase==5){c.index+=1u64;c.phase=3;}
    if(c.index>=facts.count){return builtin_call_need(c,BuiltinCallNeed::ArgumentsComplete);}
    c.phase=4;return builtin_call_need(c,BuiltinCallNeed::Argument);
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


bool construct_has(usize count,ConstructFact fact,ConstructQueries queries) on cpu requires nonzero(queries.fact) {
    usize index=0u64;
    while (index<count) {
        if (queries.fact(queries.context,index,fact)!=0u64) { return true; }
        index+=1u64;
    }
    return false;
}
ConstructPlan construct_identity(ConstructPlan input,usize count,ConstructStatus failure,ConstructQueries queries) on cpu
    requires nonzero(queries.same) {
    ConstructPlan plan=input;
    if (count==0u64) { plan.kind=ConstructKind::Zero;return plan; }
    if (count==1u64 && queries.same(queries.context,0u64)!=0) { plan.kind=ConstructKind::Identity;return plan; }
    plan.status=failure;return plan;
}
ConstructPlan construct_matrix(ConstructPlan input,usize count,ConstructQueries queries) on cpu
    requires nonzero(queries.fact) requires nonzero(queries.same) {
    ConstructPlan plan=input;
    if (count==1u64 && queries.same(queries.context,0u64)!=0) { plan.kind=ConstructKind::Identity;return plan; }
    usize index=0u64;
    while (index<count) {
        if (queries.fact(queries.context,index,ConstructFact::Fields)!=0u64 ||
            queries.fact(queries.context,index,ConstructFact::Views)!=0u64 ||
            queries.fact(queries.context,index,ConstructFact::Array)!=0u64) {
            plan.status=ConstructStatus::MatrixInput;return plan;
        }
        index+=1u64;
    }
    plan.scalar=0;
    if (count==1u64 && queries.fact(queries.context,0u64,ConstructFact::Width)==1u64) {
        plan.kind=ConstructKind::Diagonal;return plan;
    }
    usize width=0u64;index=0u64;
    while (index<count) { width+=queries.fact(queries.context,index,ConstructFact::Width);index+=1u64; }
    if (width!=plan.rows*plan.columns) { plan.status=ConstructStatus::MatrixComponents;return plan; }
    plan.kind=ConstructKind::Matrix;return plan;
}
extern "C" ConstructPlan construct_plan(ConstructEntry entry,Operand target,usize count,ConstructQueries queries) on cpu
    requires nonzero(queries.fact) requires nonzero(queries.same) requires nonzero(queries.scalar_identity) {
    ConstructPlan plan=ConstructPlan(ConstructStatus::Valid,ConstructKind::Numeric,target.scalar,0,
        usize(target.components),usize(target.rows),usize(target.columns));
    if (entry==ConstructEntry::Matrix) { return construct_matrix(plan,count,queries); }
    if (has(target,1)) { return construct_identity(plan,count,ConstructStatus::CFunctionTarget,queries); }
    if (construct_has(count,ConstructFact::CFunction,queries)) { plan.status=ConstructStatus::CFunctionInput;return plan; }
    if (has(target,2)) { return construct_identity(plan,count,ConstructStatus::AddressTarget,queries); }
    if (construct_has(count,ConstructFact::Address,queries)) { plan.status=ConstructStatus::AddressInput;return plan; }
    if (has(target,4)) { return construct_identity(plan,count,ConstructStatus::HandleTarget,queries); }
    if (construct_has(count,ConstructFact::Handle,queries)) { plan.status=ConstructStatus::HandleInput;return plan; }
    if (construct_has(count,ConstructFact::Variants,queries)) { plan.status=ConstructStatus::Variants;return plan; }
    if (construct_has(count,ConstructFact::Enumeration,queries)) {
        if (count!=1u64 || queries.scalar_identity(queries.context)==0) { plan.status=ConstructStatus::Enumeration;return plan; }
        plan.kind=ConstructKind::Enumeration;return plan;
    }
    if (has(target,128)) { return construct_matrix(plan,count,queries); }
    if (construct_has(count,ConstructFact::Array,queries)) { plan.status=ConstructStatus::Array;return plan; }
    if (construct_has(count,ConstructFact::Matrix,queries)) { plan.status=ConstructStatus::Matrix;return plan; }
    if (construct_has(count,ConstructFact::Views,queries)) { plan.status=ConstructStatus::Views;return plan; }
    if (count==1u64 && queries.same(queries.context,0u64)!=0) { plan.kind=ConstructKind::Identity;return plan; }
    usize width=0u64;usize index=0u64;
    while (index<count) { width+=queries.fact(queries.context,index,ConstructFact::Width);index+=1u64; }
    if (width==1u64 && plan.width>1u64) { plan.splat=1; }
    else if (width!=plan.width) { plan.status=ConstructStatus::Components; }
    return plan;
}
extern "C" void expand_construct(ConstructPlan plan,usize count,ConstructEmitter emitter) on cpu
    requires nonzero(emitter.argument) requires nonzero(emitter.component) requires nonzero(emitter.splat) {
    if (plan.kind==ConstructKind::Numeric) {
        ConstructArgument mode=plan.width==1u64 ? ConstructArgument::ReplaceProof : ConstructArgument::Append;
        usize index=0u64;
        while (index<count) { emitter.argument(emitter.context,plan.scalar,mode);index+=1u64; }
        if (plan.splat!=0) { emitter.splat(emitter.context,plan.width); }
        return;
    }
    if (plan.kind==ConstructKind::Diagonal) {
        emitter.argument(emitter.context,0,ConstructArgument::Diagonal);
        usize row=0u64;
        while (row<plan.rows) {
            usize column=0u64;
            while (column<plan.columns) {
                if (row==column) {
                    u32 last=row+1u64==plan.rows || column+1u64==plan.columns ? 1 : 0;
                    emitter.component(emitter.context,ConstructComponent::Diagonal,0u64,last);
                } else {
                    u32 last=row*plan.columns+column+1u64==plan.width ||
                        (plan.rows==plan.columns && row+1u64==plan.rows && column+2u64==plan.columns) ? 1 : 0;
                    emitter.component(emitter.context,ConstructComponent::Zero,0u64,last);
                }
                column+=1u64;
            }
            row+=1u64;
        }
        return;
    }
    usize index=0u64;
    while (index<count) {
        usize width=emitter.argument(emitter.context,0,ConstructArgument::Matrix);
        usize component=0u64;
        while (component<width) {
            emitter.component(emitter.context,ConstructComponent::Project,component,component+1u64==width ? 1 : 0);
            component+=1u64;
        }
        index+=1u64;
    }
}


extern "C" BitcastSignature source_bitcast(address bytes,usize length) on cpu {
    if (source_spelling(bytes,length,"bitcast_u32")) { return BitcastSignature(1,0,1,4u64); }
    if (source_spelling(bytes,length,"bitcast_f32")) { return BitcastSignature(1,1,0,4u64); }
    if (source_spelling(bytes,length,"bitcast_u64")) { return BitcastSignature(1,6,4,1u64); }
    if (source_spelling(bytes,length,"bitcast_f64")) { return BitcastSignature(1,4,6,1u64); }
    return BitcastSignature(0,0,0,0u64);
}
extern "C" BitcastPlan bitcast_plan(BitcastSignature signature,usize count,address context,
    cfn<usize,address> ensures memory(read) width,
    cfn<u32,address,u32,usize> ensures memory(read) builtin) on cpu requires nonzero(width) requires nonzero(builtin) {
    BitcastPlan result=BitcastPlan(BitcastStatus::Valid,signature.source,signature.target,0u64);
    if (count!=1u64) { result.status=BitcastStatus::Arity;return result; }
    result.width=width(context);
    if (result.width<1u64 || result.width>signature.maximum || builtin(context,signature.source,result.width)==0) {
        result.status=BitcastStatus::Type;return result;
    }
    return result;
}
bool same_conversion_scalar(ScalarEncoding a,ScalarEncoding b) on cpu {
    return a.kind==b.kind && a.width==b.width;
}
extern "C" ConversionOutcome convert_value(ConversionInput input,ConversionEmitter emitter) on cpu
    requires nonzero(emitter.source) requires nonzero(emitter.constant) requires nonzero(emitter.action) {
    bool constant=input.constant_present!=0;
    usize index=0u64;
    if (constant && input.mode==ConversionMode::Bitcast) {
        usize emitted=emitter.action(emitter.context,ConversionAction::ConstantBitcast,0u64);
        while (index<emitted) { emitter.action(emitter.context,ConversionAction::BitcastOrigin,index);index+=1u64; }
        return ConversionOutcome::Constant;
    }
    if (constant) {
        emitter.action(emitter.context,ConversionAction::BeginConstants,0u64);
        while (index<input.constant_count) {
            u64 bits=unsafe_position<u64>(unsafe_address(address_bits(input.constants)+u64(index)*8u64));
            ScalarEncoding source=emitter.source(emitter.context,index,ConversionSource::Type);
                FoldedCast value;
                if (same_conversion_scalar(source,input.target)) {
                    value=FoldedCast(bits & (18446744073709551615u64 >> u64(64-input.target.width)),1);
                } else { value=bootstrap.scalar_fold::cast(source,input.target,bits); }
                if (value.present==0) {
                    emitter.action(emitter.context,ConversionAction::DiscardConstants,0u64);constant=false;break;
                }
                bits=value.bits;
            emitter.constant(emitter.context,bits);index+=1u64;
        }
    }
    if (input.mode==ConversionMode::Numeric) {
        emitter.action(emitter.context,ConversionAction::CastProof,0u64);
        bool changed=false;index=0u64;
        while (index<input.types) {
            if (!same_conversion_scalar(emitter.source(emitter.context,index,ConversionSource::Type),input.target)) {
                changed=true;break;
            }
            index+=1u64;
        }
        if (changed && constant) {
            usize emitted=emitter.action(emitter.context,ConversionAction::ConstantValue,0u64);
            index=0u64;
            while (index<emitted) {
                if (!same_conversion_scalar(emitter.source(emitter.context,index,ConversionSource::Type),input.target)) {
                    emitter.action(emitter.context,ConversionAction::CastOrigin,index);
                }
                index+=1u64;
            }
            return ConversionOutcome::Constant;
        }
    } else { emitter.action(emitter.context,ConversionAction::BitcastProof,0u64); }
    index=0u64;
    while (index<input.registers) {
        ConversionAction action=ConversionAction::Bitcast;
        if (input.mode==ConversionMode::Numeric) {
            action=same_conversion_scalar(emitter.source(emitter.context,index,ConversionSource::Register),input.target)
                ? ConversionAction::Reuse : ConversionAction::Cast;
        }
        emitter.action(emitter.context,action,index);index+=1u64;
    }
    return ConversionOutcome::Registers;
}


extern "C" MathPreparation prepare_math(usize arity,u32 extremum,usize count,MathQueries queries) on cpu
    requires nonzero(queries.numeric) requires nonzero(queries.argument) {
    if (count!=arity) { return MathPreparation(1,MathPath::Floating,0); }
    u32 integer=integer_math(extremum,count,queries);
    MathPath path=MathPath::Floating;
    if (integer==1) { path=MathPath::Minimum; }
    else if (integer==2) { path=MathPath::Maximum; }
    return MathPreparation(0,path,count==2u64 ? 1 : 0);
}
extern "C" MathShape math_shape(MathPath path,usize count,MathQueries queries) on cpu
    requires nonzero(queries.argument) requires nonzero(queries.width) {
    if (path==MathPath::Floating) { return floating_math(count,queries); }
    usize first=queries.width(queries.context,0u64);usize second=queries.width(queries.context,1u64);
    return MathShape(0,queries.argument(queries.context,0u64).scalar,first>second ? first : second);
}
extern "C" MathOutcome expand_math(MathPath path,usize count,usize width,MathEmitter emitter) on cpu
    requires nonzero(emitter.value) requires nonzero(emitter.action) {
    if (path!=MathPath::Floating) {
        MathValueFacts left=emitter.value(emitter.context,0u64);MathValueFacts right=emitter.value(emitter.context,1u64);
        u32 literal=left.literal!=0 && right.literal!=0 ? 1 : 0;
        if (emitter.action(emitter.context,MathStep(MathAction::IntegerProof,0u64,0u64,0u64))!=0u64) {
            usize emitted=emitter.action(emitter.context,MathStep(MathAction::IntegerConstant,0u64,0u64,0u64));
            usize index=0u64;
            while (index<emitted) {
                emitter.action(emitter.context,MathStep(MathAction::KnownOrigin,index,left.width==1u64 ? 0u64 : index,right.width==1u64 ? 0u64 : index));
                index+=1u64;
            }
            return MathOutcome(1,literal,1);
        }
        MathAction comparison=path==MathPath::Minimum ? MathAction::CompareMinimum : MathAction::CompareMaximum;
        emitter.action(emitter.context,MathStep(comparison,0u64,0u64,0u64));
        usize index=0u64;
        while (index<width) {
            emitter.action(emitter.context,MathStep(MathAction::Select,index,left.width==1u64 ? 0u64 : index,right.width==1u64 ? 0u64 : index));
            index+=1u64;
        }
        return MathOutcome(0,literal,left.uniform!=0 && right.uniform!=0 ? 1 : 0);
    }
    bool uniform=true;usize argument=0u64;
    while (argument<count) {
        if (emitter.value(emitter.context,argument).uniform==0) { uniform=false;break; }
        argument+=1u64;
    }
    usize index=0u64;
    while (index<width) {
        emitter.action(emitter.context,MathStep(MathAction::FloatBegin,index,0u64,0u64));
        argument=0u64;
        while (argument<count) {
            MathValueFacts value=emitter.value(emitter.context,argument);
            emitter.action(emitter.context,MathStep(MathAction::FloatInput,index,argument,value.width==1u64 ? 0u64 : index));
            argument+=1u64;
        }
        emitter.action(emitter.context,MathStep(MathAction::FloatEnd,index,0u64,0u64));index+=1u64;
    }
    emitter.action(emitter.context,MathStep(MathAction::FloatingProof,0u64,0u64,0u64));
    return MathOutcome(0,0,uniform ? 1 : 0);
}


extern "C" ValueUnaryOutcome expand_unary_value(ValueUnaryInput input,ValueUnaryEmitter emitter) on cpu
    requires nonzero(emitter.constant) requires nonzero(emitter.action) {
    if (input.constant_present!=0) {
        emitter.action(emitter.context,ValueUnaryAction::BeginConstants,0u64);
        UnaryOperation operation=UnaryOperation::Negate;
        if (input.operation==1) { operation=UnaryOperation::FloatNegate; }
        else if (input.operation==4) { operation=UnaryOperation::LogicalNot; }
        else if (input.operation==5) { operation=UnaryOperation::BitNot; }
        usize index=0u64;
        while (index<input.constant_count) {
            u64 bits=unsafe_position<u64>(unsafe_address(address_bits(input.constants)+u64(index)*8u64));
            emitter.constant(emitter.context,bootstrap.scalar_fold::unary(operation,input.width,bits));
            index+=1u64;
        }
    }
    emitter.action(emitter.context,ValueUnaryAction::Proof,0u64);
    if (input.constant_present!=0) {
        usize emitted=emitter.action(emitter.context,ValueUnaryAction::ConstantValue,0u64);
        usize index=0u64;
        while (index<emitted) { emitter.action(emitter.context,ValueUnaryAction::Origin,index);index+=1u64; }
        return ValueUnaryOutcome::Constant;
    }
    usize index=0u64;
    while (index<input.registers) { emitter.action(emitter.context,ValueUnaryAction::Emit,index);index+=1u64; }
    return ValueUnaryOutcome::Registers;
}


FoldedCast numeric_binary_fold(u32 code,ScalarEncoding scalar,u64 left,u64 right) on cpu {
    bool floating=scalar.kind==ScalarKind::Float;
    if ((!floating && code>=1 && code<=4) || (floating && ((code>=5 && code<=7) || code>=14))) {
        return FoldedCast(0u64,0);
    }
    BinaryOperation operation=BinaryOperation::Add;
    if (code==2 || code==6) { operation=BinaryOperation::Subtract; }
    else if (code==3 || code==7) { operation=BinaryOperation::Multiply; }
    else if (code==4) { operation=BinaryOperation::Divide; }
    else if (code==8) { operation=BinaryOperation::Equal; }
    else if (code==9) { operation=BinaryOperation::NotEqual; }
    else if (code==10) { operation=BinaryOperation::Less; }
    else if (code==11) { operation=BinaryOperation::Greater; }
    else if (code==12) { operation=BinaryOperation::LessEqual; }
    else if (code==13) { operation=BinaryOperation::GreaterEqual; }
    else if (code==14) { operation=BinaryOperation::And; }
    else if (code==15) { operation=BinaryOperation::Or; }
    else if (code==16) { operation=BinaryOperation::Xor; }
    else if (code==17) { operation=BinaryOperation::ShiftLeft; }
    else if (code==18) { operation=BinaryOperation::ShiftRight; }
    else if (code==19) { operation=BinaryOperation::ShiftRightSigned; }
    u64 bits;
    if (floating) { bits=bootstrap.scalar_fold::float_binary(operation,scalar.width,left,right); }
    else { bits=bootstrap.scalar_fold::binary(operation,scalar.width,scalar.kind==ScalarKind::Signed ? 1 : 0,left,right); }
    return FoldedCast(bits,1);
}
extern "C" ValueBinaryOutcome expand_binary_value(ValueBinaryInput input,ValueBinaryEmitter emitter) on cpu
    requires nonzero(emitter.divisor) requires nonzero(emitter.divide)
    requires nonzero(emitter.constant) requires nonzero(emitter.action) {
    bool folded=input.left.constant_present!=0 && input.right.constant_present!=0;
    usize index=0u64;
    if (folded) {
        emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::BeginConstants,0u64,0u64,0u64,0u64));
        while (index<input.count) {
            usize ai=input.left.constant_count==1u64 ? 0u64 : index;
            usize bi=input.right.constant_count==1u64 ? 0u64 : index;
            u64 left=unsafe_position<u64>(unsafe_address(address_bits(input.left.constants)+u64(ai)*8u64));
            u64 right=unsafe_position<u64>(unsafe_address(address_bits(input.right.constants)+u64(bi)*8u64));
            FoldedCast result;
            if (input.divisors_present!=0) {
                ValueDivisor divisor=emitter.divisor(emitter.context,input.divisor_count==1u64 ? 0u64 : index);
                if (divisor.constant!=0) { result=FoldedCast(emitter.divide(emitter.context,left,divisor.nonzero),1); }
                else { result=FoldedCast(0u64,0); }
            } else { result=numeric_binary_fold(input.operation,input.scalar,left,right); }
            if (result.present==0) {
                emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::DiscardConstants,0u64,0u64,0u64,0u64));
                folded=false;break;
            }
            emitter.constant(emitter.context,result.bits);index+=1u64;
        }
    }
    ValueBinaryAction proof=input.operation>=8 && input.operation<=13 ? ValueBinaryAction::ComparisonProof : ValueBinaryAction::ArithmeticProof;
    emitter.action(emitter.context,ValueBinaryStep(proof,0u64,0u64,0u64,0u64));
    bool known=false;
    if (!folded && input.comparison!=0) {
        known=emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::KnownComparison,0u64,0u64,0u64,0u64))!=0u64;
    }
    if (folded || known) {
        ValueBinaryAction action=folded ? ValueBinaryAction::FoldConstantValue : ValueBinaryAction::KnownConstantValue;
        usize emitted=emitter.action(emitter.context,ValueBinaryStep(action,0u64,0u64,0u64,0u64));
        index=0u64;
        while (index<emitted) {
            usize left=folded ? input.left.constant_count : input.left.registers;
            usize right=folded ? input.right.constant_count : input.right.registers;
            ValueBinaryAction origin=folded ? ValueBinaryAction::ConstantOrigin : ValueBinaryAction::KnownOrigin;
            emitter.action(emitter.context,ValueBinaryStep(origin,index,left==1u64 ? 0u64 : index,right==1u64 ? 0u64 : index,0u64));
            index+=1u64;
        }
        return ValueBinaryOutcome::Constant;
    }
    index=0u64;
    while (index<input.count) {
        usize di=input.divisor_count==1u64 ? 0u64 : index;
        ValueBinaryAction action=ValueBinaryAction::Binary;
        if (input.divisors_present!=0) {
            ValueDivisor divisor=emitter.divisor(emitter.context,di);
            if (input.scalar.kind==ScalarKind::Signed && divisor.constant!=0 &&
                divisor.bits==(18446744073709551615u64 >> u64(64-input.scalar.width))) {
                action=input.remainder!=0 ? ValueBinaryAction::Zero : ValueBinaryAction::Negate;
            } else { action=input.remainder!=0 ? ValueBinaryAction::Remainder : ValueBinaryAction::Quotient; }
        }
        usize instruction=emitter.action(emitter.context,ValueBinaryStep(action,index,
            input.left.registers==1u64 ? 0u64 : index,input.right.registers==1u64 ? 0u64 : index,di));
        if (input.scalar.kind==ScalarKind::Float) {
            emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::PreserveFloat,index,instruction,0u64,0u64));
        }
        index+=1u64;
    }
    emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::FinishRegisters,0u64,0u64,0u64,0u64));
    emitter.action(emitter.context,ValueBinaryStep(ValueBinaryAction::ObserveInteger,0u64,0u64,0u64,0u64));
    return ValueBinaryOutcome::Registers;
}


extern "C" u32 lower_source_binary(SourceBinaryInput input,SourceBinaryEmitter emitter) on cpu
    requires nonzero(emitter.action) requires nonzero(emitter.reenter) {
    if (input.plan.kind==1) {
        emitter.action(emitter.context,SourceBinaryAction::Identity,0);
        return emitter.reenter(emitter.context,SourceBinaryToken::Original);
    }
    if (input.plan.kind==2) {
        emitter.action(emitter.context,SourceBinaryAction::Handle,0);
        if (emitter.reenter(emitter.context,SourceBinaryToken::Xor)==0) { return 0; }
        emitter.action(emitter.context,SourceBinaryAction::HandleLeft,0);
        emitter.action(emitter.context,SourceBinaryAction::HandleRight,1);
        if (emitter.reenter(emitter.context,SourceBinaryToken::Or)==0) { return 0; }
        emitter.action(emitter.context,SourceBinaryAction::DifferenceWithZero,0);
        return emitter.reenter(emitter.context,SourceBinaryToken::Original);
    }
    if (input.plan.kind!=0) { return emitter.action(emitter.context,SourceBinaryAction::Matrix,0); }
    u32 literal=input.left_literal!=0 && input.right_literal!=0 && !floating(input.plan.scalar) && input.plan.comparison==0 ? 1 : 0;
    if (input.plan.cast_left!=0) { emitter.action(emitter.context,SourceBinaryAction::CastLeft,input.plan.scalar); }
    if (input.plan.cast_right!=0) { emitter.action(emitter.context,SourceBinaryAction::CastRight,input.plan.scalar); }
    if (input.division!=0 && !floating(input.plan.scalar)) {
        if (emitter.action(emitter.context,SourceBinaryAction::IntegerDivisors,0)==0) {
            if (emitter.action(emitter.context,SourceBinaryAction::PendingDivisors,0)!=0) {
                if (emitter.action(emitter.context,SourceBinaryAction::DeferredDivisors,0)==0) { return 0; }
            } else { return emitter.action(emitter.context,SourceBinaryAction::MissingDivisors,0); }
        }
    }
    return emitter.action(emitter.context,SourceBinaryAction::Numeric,literal);
}


WordScalar word_raw(WordScalar scalar) on cpu {
    if (scalar.encoding.width==64) { return WordScalar(4,ScalarEncoding(ScalarKind::Unsigned,64)); }
    return WordScalar(1,ScalarEncoding(ScalarKind::Unsigned,32));
}
extern "C" void lower_variant_component(WordScalar source,WordScalar target,usize index,WordEmitter emitter) on cpu
    requires nonzero(emitter.action) {
    emitter.action(emitter.context,WordStep(WordAction::BeginComponent,index,source.code,WordOperation::ShiftRight,0,0u64));
    if (source.encoding.kind==ScalarKind::Float || target.encoding.kind==ScalarKind::Float) {
        if (source.code==target.code) { return; }
        if (source.encoding.kind==ScalarKind::Float) {
            emitter.action(emitter.context,WordStep(WordAction::Bitcast,0u64,word_raw(source).code,WordOperation::ShiftRight,0,0u64));
        }
        WordScalar raw=target.encoding.kind==ScalarKind::Float ? word_raw(target) : target;
        emitter.action(emitter.context,WordStep(WordAction::Cast,0u64,raw.code,WordOperation::ShiftRight,0,0u64));
        if (target.encoding.kind==ScalarKind::Float) {
            emitter.action(emitter.context,WordStep(WordAction::Bitcast,0u64,target.code,WordOperation::ShiftRight,0,0u64));
        }
        return;
    }
    WordAction action=(source.code==3 && target.code==1)||(source.code==1 && target.code==3) ? WordAction::CastKeepProof : WordAction::Cast;
    emitter.action(emitter.context,WordStep(action,0u64,target.code,WordOperation::ShiftRight,0,0u64));
}
void emit_word_operation(WordOperation operation,WordScalar target,WordEmitter emitter) on cpu
    requires nonzero(emitter.known) requires nonzero(emitter.action) {
    WordConstants known=emitter.known(emitter.context);
    u32 present=known.left_present!=0 && known.right_present!=0 ? 1 : 0;
    u64 bits=0u64;
    if (present!=0) {
        u32 code=18;
        if (operation==WordOperation::And) { code=14; }
        else if (operation==WordOperation::ShiftLeft) { code=17; }
        else if (operation==WordOperation::Or) { code=15; }
        bits=numeric_binary_fold(code,target.encoding,known.left,known.right).bits;
    }
    emitter.action(emitter.context,WordStep(WordAction::Emit,0u64,target.code,operation,present,bits));
}
void emit_word_slice(WordOperation operation,WordScalar target,u64 bits,bool needed,WordEmitter emitter) on cpu
    requires nonzero(emitter.known) requires nonzero(emitter.action) {
    emitter.action(emitter.context,WordStep(WordAction::Cast,0u64,target.code,WordOperation::ShiftRight,0,0u64));
    if (!needed) { return; }
    emitter.action(emitter.context,WordStep(WordAction::RightConstant,0u64,target.code,WordOperation::ShiftRight,0,bits));
    emit_word_operation(operation,target,emitter);
}
extern "C" void lower_variant_word(WordInput input,WordEmitter emitter) on cpu
    requires nonzero(emitter.component) requires nonzero(emitter.known) requires nonzero(emitter.action) {
    WordScalar raw=word_raw(input.target);
    u64 mask=18446744073709551615u64 >> u64(64-raw.encoding.width);
    usize index=0u64;
    while (index<input.components) {
        WordComponent source=emitter.component(emitter.context,index);
        u32 begin=max(input.offset,source.offset);
        u32 end=min(input.offset+input.target.encoding.width/8,source.offset+source.scalar.encoding.width/8);
        if (begin<end) {
            if (input.offset==source.offset && input.target.encoding.width==source.scalar.encoding.width) {
                emitter.action(emitter.context,WordStep(WordAction::Project,index,input.target.code,WordOperation::ShiftRight,0,0u64));return;
            }
            WordScalar source_raw=word_raw(source.scalar);
            emitter.action(emitter.context,WordStep(WordAction::Project,index,source_raw.code,WordOperation::ShiftRight,0,0u64));
            u64 right=u64((begin-source.offset)*8);
            emit_word_slice(WordOperation::ShiftRight,source_raw,right,right!=0u64,emitter);
            u64 selected=mask >> u64(raw.encoding.width-(end-begin)*8);
            emit_word_slice(WordOperation::And,raw,selected,selected!=mask,emitter);
            u64 left=u64((begin-input.offset)*8);
            emit_word_slice(WordOperation::ShiftLeft,raw,left,left!=0u64,emitter);
            emitter.action(emitter.context,WordStep(WordAction::Cast,0u64,raw.code,WordOperation::ShiftRight,0,0u64));
            if (emitter.action(emitter.context,WordStep(WordAction::TakeJoined,0u64,raw.code,WordOperation::ShiftRight,0,0u64))!=0) {
                emit_word_operation(WordOperation::Or,raw,emitter);
            }
            emitter.action(emitter.context,WordStep(WordAction::Accumulate,0u64,raw.code,WordOperation::ShiftRight,0,0u64));
        }
        index+=1u64;
    }
    if (emitter.action(emitter.context,WordStep(WordAction::TakeAccumulated,0u64,raw.code,WordOperation::ShiftRight,0,0u64))==0) {
        emitter.action(emitter.context,WordStep(WordAction::Zero,0u64,raw.code,WordOperation::ShiftRight,0,0u64));
    }
    emitter.action(emitter.context,WordStep(WordAction::FinalComponent,0u64,input.target.code,WordOperation::ShiftRight,0,0u64));
    if (input.target.encoding.kind==ScalarKind::Unsigned || input.target.encoding.kind==ScalarKind::Signed) {
        if (emitter.action(emitter.context,WordStep(WordAction::NonzeroBytes,0u64,input.target.code,WordOperation::ShiftRight,0,0u64))!=0) {
            emitter.action(emitter.context,WordStep(WordAction::MarkNonzero,0u64,input.target.code,WordOperation::ShiftRight,0,0u64));
        }
    }
}


usize variant_component_at(VariantEmitter emitter,u32 offset) on cpu requires nonzero(emitter.field) {
    usize index=0u64;
    while (true) {
        if (emitter.field(emitter.context,VariantSide::Storage,index).offset==offset) { return index; }
        index+=1u64;
    }
}
void variant_borrow_map(VariantInput input,bool packing,VariantEmitter emitter) on cpu
    requires nonzero(emitter.field) requires nonzero(emitter.views) requires nonzero(emitter.next) requires nonzero(emitter.action) {
    emitter.views(emitter.context,VariantSide::Payload);
    while (true) {
        VariantView view=emitter.next(emitter.context);
        if (view.present==0) { return; }
        u32 offset=input.offset+emitter.field(emitter.context,VariantSide::Payload,view.component).offset;
        usize storage=variant_component_at(emitter,offset);
        emitter.action(emitter.context,VariantStep(VariantAction::Borrow,packing ? storage : view.component,packing ? view.component : storage,0,0,0,0u64));
    }
}
extern "C" void build_variant_value(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.field) requires nonzero(emitter.load) requires nonzero(emitter.views)
    requires nonzero(emitter.next) requires nonzero(emitter.active) requires nonzero(emitter.action) {
    emitter.action(emitter.context,VariantStep(VariantAction::InitializeBuild,0u64,0u64,0,0,0,0u64));
    emitter.action(emitter.context,VariantStep(VariantAction::Tag,0u64,0u64,input.scalar,0,0,input.discriminant));
    if (input.payload!=0) {
        bool known=true;usize index=1u64;
        while (index<input.components) {
            VariantField field=emitter.field(emitter.context,VariantSide::Storage,index);
            VariantWord word=emitter.load(emitter.context,field.offset-input.offset,field.scalar);
            known=known && word.constant!=0;
            emitter.action(emitter.context,VariantStep(VariantAction::StoreBuild,index,0u64,0,0,known ? 1 : 0,word.bits));index+=1u64;
        }
        variant_borrow_map(input,true,emitter);
        emitter.action(emitter.context,VariantStep(VariantAction::PayloadFacts,0u64,0u64,0,0,0,0u64));
    }
    emitter.action(emitter.context,VariantStep(VariantAction::RetainBuild,0u64,0u64,0,0,0,0u64));
    emitter.action(emitter.context,VariantStep(VariantAction::StartActive,0u64,0u64,0,0,0,0u64));
    emitter.views(emitter.context,VariantSide::Selected);
    while (true) {
        VariantView view=emitter.next(emitter.context);if (view.present==0) { break; }
        u32 offset=input.offset+emitter.field(emitter.context,VariantSide::Selected,view.component).offset;
        usize storage=variant_component_at(emitter,offset);
        emitter.action(emitter.context,VariantStep(VariantAction::AddActive,storage,0u64,0,0,0,0u64));
    }
    emitter.views(emitter.context,VariantSide::Storage);
    while (true) {
        VariantView view=emitter.next(emitter.context);if (view.present==0) { break; }
        if (emitter.active(emitter.context,view.component)==0) {
            emitter.action(emitter.context,VariantStep(VariantAction::Inactive,view.component,0u64,0,0,0,0u64));
        }
    }
    emitter.action(emitter.context,VariantStep(VariantAction::EndActive,0u64,0u64,0,0,0,0u64));
}
extern "C" void read_variant_payload(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.field) requires nonzero(emitter.load) requires nonzero(emitter.views)
    requires nonzero(emitter.next) requires nonzero(emitter.action) {
    if (input.unit!=0) { return; }
    emitter.action(emitter.context,VariantStep(VariantAction::InitializeRead,0u64,0u64,0,0,0,0u64));
    bool known=true;usize index=0u64;
    while (index<input.components) {
        VariantField field=emitter.field(emitter.context,VariantSide::Payload,index);
        VariantWord word=emitter.load(emitter.context,input.offset+field.offset,field.scalar);known=known && word.constant!=0;
        emitter.action(emitter.context,VariantStep(VariantAction::StoreRead,index,0u64,0,0,known ? 1 : 0,word.bits));index+=1u64;
    }
    variant_borrow_map(input,false,emitter);
    emitter.action(emitter.context,VariantStep(VariantAction::FinishRead,0u64,0u64,0,0,0,0u64));
}
extern "C" void test_variant_tag(VariantInput input,VariantEmitter emitter) on cpu
    requires nonzero(emitter.load) requires nonzero(emitter.action) {
    emitter.load(emitter.context,0,input.scalar);
    emitter.action(emitter.context,VariantStep(VariantAction::ExpectedTag,0u64,0u64,input.scalar,0,0,input.discriminant));
    emitter.action(emitter.context,VariantStep(VariantAction::CompareTag,0u64,0u64,0,0,0,0u64));
}

extern "C" BuiltinValueCursor builtin_value_start(address bytes,u64 length) on cpu {
 BuiltinValueKind kind=BuiltinValueKind::Other;
 if(source_spelling(bytes,length,"function_address")){kind=BuiltinValueKind::FunctionAddress;}
 else if(source_spelling(bytes,length,"unsafe_handle")){kind=BuiltinValueKind::UnsafeHandle;}
 else if(source_spelling(bytes,length,"unsafe_address")){kind=BuiltinValueKind::UnsafeAddress;}
 else if(source_spelling(bytes,length,"address_bits")){kind=BuiltinValueKind::AddressBits;}
 return BuiltinValueCursor(0,kind);
}
BuiltinValueStep builtin_values(BuiltinValueCursor cursor,BuiltinValueKind kind) on cpu {BuiltinValueCursor c=cursor;c.kind=kind;c.phase=20;return BuiltinValueStep(c,BuiltinValueNeed::Values,0);}
extern "C" BuiltinValueStep builtin_value_step(BuiltinValueCursor cursor,BuiltinValueFacts facts,address bytes,u64 length) on cpu {
 BuiltinValueCursor c=cursor;
 if(c.phase==0){if(c.kind!=BuiltinValueKind::Other){return builtin_values(c,c.kind);}c.phase=1;return BuiltinValueStep(c,BuiltinValueNeed::Type,0);}
 if(c.phase==1){if(facts.present!=0){return builtin_values(c,BuiltinValueKind::Type);}c.phase=2;return BuiltinValueStep(c,BuiltinValueNeed::Bitcast,0);}
 if(c.phase==2){if(facts.present!=0){return builtin_values(c,BuiltinValueKind::Bitcast);}c.phase=3;return BuiltinValueStep(c,BuiltinValueNeed::Math,0);}
 if(c.phase==3){if(facts.present!=0){return builtin_values(c,BuiltinValueKind::Math);}c.phase=4;return BuiltinValueStep(c,BuiltinValueNeed::Matrix,0);}
 if(c.phase==4){if(facts.present!=0){return builtin_values(c,BuiltinValueKind::Matrix);}c.phase=5;return BuiltinValueStep(c,BuiltinValueNeed::Geometry,0);}
 if(c.phase==5){if(facts.present!=0){return builtin_values(c,BuiltinValueKind::Geometry);}if(source_spelling(bytes,length,"clamp")){return builtin_values(c,BuiltinValueKind::Clamp);}return BuiltinValueStep(c,BuiltinValueNeed::Unknown,0);}
 if(c.phase==20){
  if(facts.present==0){return BuiltinValueStep(c,BuiltinValueNeed::Empty,0);}
  if(c.kind==BuiltinValueKind::FunctionAddress){if(facts.cpu==0){return BuiltinValueStep(c,BuiltinValueNeed::BadFunctionTarget,0);}if(facts.count!=1u64){return BuiltinValueStep(c,BuiltinValueNeed::BadFunctionCount,0);}c.phase=21;return BuiltinValueStep(c,BuiltinValueNeed::Pop,0);}
  if(c.kind==BuiltinValueKind::UnsafeHandle){if(facts.count!=1u64){return BuiltinValueStep(c,BuiltinValueNeed::BadHandleCount,0);}c.phase=23;return BuiltinValueStep(c,BuiltinValueNeed::Pop,0);}
  if(c.kind==BuiltinValueKind::UnsafeAddress||c.kind==BuiltinValueKind::AddressBits){if(facts.cpu==0){return BuiltinValueStep(c,BuiltinValueNeed::BadAddressTarget,0);}if(facts.count!=1u64){return BuiltinValueStep(c,BuiltinValueNeed::BadAddressCount,0);}c.phase=27;return BuiltinValueStep(c,BuiltinValueNeed::Pop,0);}
  if(c.kind==BuiltinValueKind::Type){return BuiltinValueStep(c,BuiltinValueNeed::Constructor,0);}
  if(c.kind==BuiltinValueKind::Bitcast){return BuiltinValueStep(c,BuiltinValueNeed::InvokeBitcast,0);}
  if(c.kind==BuiltinValueKind::Math){return BuiltinValueStep(c,BuiltinValueNeed::InvokeMath,0);}
  if(c.kind==BuiltinValueKind::Matrix){return BuiltinValueStep(c,BuiltinValueNeed::InvokeMatrix,0);}
  if(c.kind==BuiltinValueKind::Geometry){return BuiltinValueStep(c,BuiltinValueNeed::InvokeGeometry,0);}
  if(facts.count!=3u64){return BuiltinValueStep(c,BuiltinValueNeed::BadClampCount,0);}c.phase=30;return BuiltinValueStep(c,BuiltinValueNeed::ClampMax,0);
 }
 if(c.phase==21){c.phase=22;return BuiltinValueStep(c,BuiltinValueNeed::FunctionKind,0);}
 if(c.phase==22){if(facts.function==0){return BuiltinValueStep(c,BuiltinValueNeed::BadFunctionKind,0);}c.phase=40;return BuiltinValueStep(c,BuiltinValueNeed::FunctionToAddress,0);}
 if(c.phase==23){c.phase=24;return BuiltinValueStep(c,BuiltinValueNeed::RequireU64,0);}
 if(c.phase==24){c.phase=25;return BuiltinValueStep(c,BuiltinValueNeed::HandleLow,0);}
 if(c.phase==25){c.phase=26;return BuiltinValueStep(c,BuiltinValueNeed::HandleHigh,0);}
 if(c.phase==26){c.phase=40;u32 flags=0;if(facts.low_known!=0&&facts.high_known!=0){flags|=1;}if(facts.low_uniform!=0&&facts.high_uniform!=0){flags|=2;}return BuiltinValueStep(c,BuiltinValueNeed::HandlePackage,flags);}
 if(c.phase==27){c.phase=28;return BuiltinValueStep(c,c.kind==BuiltinValueKind::UnsafeAddress?BuiltinValueNeed::RequireU64:BuiltinValueNeed::RequireAddress,0);}
 if(c.phase==28){c.phase=40;return BuiltinValueStep(c,c.kind==BuiltinValueKind::UnsafeAddress?BuiltinValueNeed::RawToAddress:BuiltinValueNeed::AddressToBits,0);}
 if(c.phase==30){c.phase=40;return BuiltinValueStep(c,BuiltinValueNeed::ClampMin,0);}
 return BuiltinValueStep(c,BuiltinValueNeed::Complete,0);
}
extern "C" FunctionFromAddressStep function_from_address_step(FunctionFromAddressCursor cursor,FunctionFromAddressFacts facts) on cpu {
 FunctionFromAddressCursor c=cursor;
 if(c.phase==0){if(facts.cpu==0){return FunctionFromAddressStep(c,FunctionFromAddressNeed::BadTarget);}if(facts.explicit_type==0||facts.count!=1u64){return FunctionFromAddressStep(c,FunctionFromAddressNeed::BadArguments);}c.phase=1;return FunctionFromAddressStep(c,FunctionFromAddressNeed::Type);}
 if(c.phase==1){if(facts.function==0){return FunctionFromAddressStep(c,FunctionFromAddressNeed::BadSignature);}c.phase=2;return FunctionFromAddressStep(c,FunctionFromAddressNeed::Argument);}
 if(c.phase==2){if(facts.present==0){return FunctionFromAddressStep(c,FunctionFromAddressNeed::Empty);}c.phase=3;return FunctionFromAddressStep(c,FunctionFromAddressNeed::RequireAddress);}
 if(c.phase==3){c.phase=4;return FunctionFromAddressStep(c,FunctionFromAddressNeed::Retype);}
 return FunctionFromAddressStep(c,FunctionFromAddressNeed::Complete);
}
