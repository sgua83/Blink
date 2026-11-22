module codec.value_format;

struct ReadBytes { Bytes value;ReadStatus status; };
struct ReadAccess { AccessTag value;ReadStatus status; };
struct ReadTarget { TargetTag value;ReadStatus status; };

address item(Span values,u64 index) on cpu {
 return unsafe_address(address_bits(values.data)+index*values.stride);
}

void byte(u8 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 append(output,address_of(value),1u64);
}

void word(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 array<u8,4> bytes;
 bytes[0]=u8(value);bytes[1]=u8(value>>8);bytes[2]=u8(value>>16);bytes[3]=u8(value>>24);
 append(output,address_of(bytes[0]),4u64);
}

void wide(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 array<u8,8> bytes;
 bytes[0]=u8(value);bytes[1]=u8(value>>8);bytes[2]=u8(value>>16);bytes[3]=u8(value>>24);
 bytes[4]=u8(value>>32);bytes[5]=u8(value>>40);bytes[6]=u8(value>>48);bytes[7]=u8(value>>56);
 append(output,address_of(bytes[0]),8u64);
}

void text(Bytes value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 wide(value.count,output,append);append(output,value.data,value.count);
}

void type_option(address value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 bool present=address_bits(value)!=0u64;byte(present?u8(1):u8(0),output,host.append);
 if(present){write_type(value,output,host);}
}

void arguments(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host.append);u64 index=0u64;
 while(index<values.count){
  Argument value=host.argument(item(values,index));
  if(value.kind==0){word(0,output,host.append);type_name(value.value,output,host);}
  else{word(1,output,host.append);shape(value.value,output,host);}
  index+=1u64;
 }
}

void predicates(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host.append);u64 index=0u64;
 while(index<values.count){predicate(item(values,index),output,host);index+=1u64;}
}

void type_name(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 TypeName value=host.type_name(source);text(value.name,output,host.append);byte(u8(value.canonical),output,host.append);
 arguments(value.arguments,output,host);byte(u8(value.dataset),output,host.append);
 byte(u8(value.view),output,host.append);byte(u8(value.tile),output,host.append);
 bool count=address_bits(value.count)!=0u64;byte(count?u8(1):u8(0),output,host.append);
 if(count){shape(value.count,output,host);}
 byte(u8(value.contracts_present),output,host.append);
 if(value.contracts_present!=0){predicates(value.contracts,output,host);}
}

void shape(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Shape value=host.shape(source);
 if(value.kind==0){word(0,output,host.append);word(value.literal,output,host.append);}
 else if(value.kind==1){word(1,output,host.append);text(value.name,output,host.append);}
 else{word(2,output,host.append);expression(value.expression,output,host);}
}

void expressions(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host.append);u64 index=0u64;
 while(index<values.count){expression(item(values,index),output,host);index+=1u64;}
}

void expression(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Expression value=host.expression(source);
 // Description kinds are actual syntax variants; the wire tags and payload
 // order below belong to this encoder, not the Rust field projection.
 if(value.kind==0){word(0,output,host.append);text(value.name,output,host.append);}
 else if(value.kind==1){word(1,output,host.append);word(value.literal,output,host.append);}
 else if(value.kind==2){word(2,output,host.append);byte(u8(value.literal),output,host.append);}
 else if(value.kind==3){word(3,output,host.append);text(value.name,output,host.append);}
 else if(value.kind==4){word(4,output,host.append);expression(value.a,output,host);text(value.name,output,host.append);}
 else if(value.kind==5){word(5,output,host.append);expression(value.a,output,host);expression(value.b,output,host);}
 else if(value.kind==6){word(6,output,host.append);text(value.name,output,host.append);expressions(value.arguments,output,host);}
 else if(value.kind==7){word(7,output,host.append);expression(value.a,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==8){word(8,output,host.append);type_name(value.ty,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==9){word(9,output,host.append);text(value.name,output,host.append);arguments(value.types,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==10){word(10,output,host.append);text(value.name,output,host.append);arguments(value.types,output,host);}
 else if(value.kind==11){
  word(11,output,host.append);text(value.name,output,host.append);byte(u8(value.types_present),output,host.append);
  if(value.types_present!=0){arguments(value.types,output,host);}expressions(value.arguments,output,host);
 }else if(value.kind==12){word(12,output,host.append);word(value.target,output,host.append);expression(value.a,output,host);}
 else if(value.kind==13){word(13,output,host.append);text(value.name,output,host.append);expression(value.a,output,host);}
 else if(value.kind==14){word(14,output,host.append);text(value.name,output,host.append);expression(value.a,output,host);expression(value.b,output,host);}
 else{word(15,output,host.append);expression(value.a,output,host);expression(value.b,output,host);expression(value.c,output,host);}
 wide(value.start,output,host.append);wide(value.end,output,host.append);
}

void constant(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Constant value=host.constant(source);write_type(value.ty,output,host);wide(value.bits.count,output,host.append);u64 index=0u64;
 while(index<value.bits.count){wide(unsafe_position<u64>(item(value.bits,index)),output,host.append);index+=1u64;}
}

void condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Condition value=host.condition(source);
 if(value.kind==0){word(0,output,host.append);text(value.name,output,host.append);}
 else if(value.kind==1){word(1,output,host.append);text(value.name,output,host.append);}
 else if(value.kind==2){word(2,output,host.append);byte(u8(value.literal),output,host.append);}
 else if(value.kind==3){word(3,output,host.append);condition(value.a,output,host);text(value.name,output,host.append);}
 else if(value.kind==4){word(4,output,host.append);text(value.name,output,host.append);condition(value.a,output,host);}
 else if(value.kind==5){word(5,output,host.append);text(value.name,output,host.append);condition(value.a,output,host);condition(value.b,output,host);}
 else if(value.kind==6){word(6,output,host.append);word(value.literal,output,host.append);}
 else{word(7,output,host.append);constant(value.constant,output,host);}
}

void predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Predicate value=host.predicate(source);
 if(value.kind==0){word(0,output,host.append);text(value.first,output,host.append);text(value.second,output,host.append);text(value.third,output,host.append);}
 else if(value.kind==1){word(1,output,host.append);text(value.first,output,host.append);}
 else if(value.kind==2){word(2,output,host.append);text(value.first,output,host.append);}
 else if(value.kind==3){word(4,output,host.append);text(value.first,output,host.append);}
 else if(value.kind==4){word(8,output,host.append);byte(u8(value.access_present),output,host.append);if(value.access_present!=0){word(value.access,output,host.append);}}
 else if(value.kind==5){word(3,output,host.append);text(value.first,output,host.append);text(value.second,output,host.append);}
 else if(value.kind==6){
  word(9,output,host.append);text(value.first,output,host.append);wide(value.intervals.count,output,host.append);u64 index=0u64;
  while(index<value.intervals.count){Interval bounds=host.interval(item(value.intervals,index));condition(bounds.lower,output,host);condition(bounds.upper,output,host);index+=1u64;}
 }else if(value.kind==7){word(6,output,host.append);text(value.first,output,host.append);text(value.second,output,host.append);}
 else if(value.kind==8){word(5,output,host.append);text(value.first,output,host.append);condition(value.condition,output,host);}
 else{word(7,output,host.append);text(value.first,output,host.append);condition(value.condition,output,host);}
}

extern "C" void write_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Type value=host.ty(source);text(value.name,output,host.append);wide(value.arguments.count,output,host.append);u64 index=0u64;
 while(index<value.arguments.count){write_type(item(value.arguments,index),output,host);index+=1u64;}
 wide(value.values.count,output,host.append);index=0u64;
 while(index<value.values.count){word(unsafe_position<u32>(item(value.values,index)),output,host.append);index+=1u64;}
 word(value.size,output,host.append);word(value.alignment,output,host.append);wide(value.components.count,output,host.append);index=0u64;
 while(index<value.components.count){Component part=host.component(item(value.components,index));word(part.scalar,output,host.append);word(part.word,output,host.append);index+=1u64;}
 wide(value.fields.count,output,host.append);index=0u64;
 while(index<value.fields.count){Field field=host.field(item(value.fields,index));text(field.name,output,host.append);word(field.word,output,host.append);write_type(field.ty,output,host);index+=1u64;}
 type_option(value.view,output,host);type_option(value.array,output,host);
 byte(u8(value.enumeration),output,host.append);byte(u8(value.result),output,host.append);wide(value.variants.count,output,host.append);index=0u64;
 while(index<value.variants.count){Variant variant=host.variant(item(value.variants,index));text(variant.name,output,host.append);word(variant.discriminant,output,host.append);write_type(variant.ty,output,host);index+=1u64;}
 bool dataset=address_bits(value.dataset)!=0u64;byte(dataset?u8(1):u8(0),output,host.append);
 if(dataset){type_name(value.dataset,output,host);}
 byte(u8(value.cleanup_present),output,host.append);if(value.cleanup_present!=0){text(value.cleanup,output,host.append);}
 bool callable=address_bits(value.function)!=0u64;byte(callable?u8(1):u8(0),output,host.append);
 if(callable){
  Function function=host.function(value.function);type_name(function.result,output,host);wide(function.parameters.count,output,host.append);index=0u64;
  while(index<function.parameters.count){Parameter parameter=host.parameter(item(function.parameters,index));word(parameter.access,output,host.append);type_name(parameter.ty,output,host);index+=1u64;}
  predicates(function.contracts,output,host);
 }
}

void name_token<const N:u32 in {3,4,5,6,7,8,10,16,18,21}>(
    read array<u8,N> token, address output, Host host
) on cpu requires nonzero(host.append) {
    host.append(output, address_of(token[0]), u64(N));
}

void name_bytes(Bytes value, address output, Host host) on cpu requires nonzero(host.append) {
    host.append(output, value.data, value.count);
}

void name_decimal(u32 value, address output, Host host) on cpu requires nonzero(host.append) {
    array<u8,10> digits;
    u64 cursor = 10u64;
    u32 remaining = value;
    while (cursor == 10u64 || remaining != 0) {
        cursor -= 1u64;
        unsafe_position<u8>(unsafe_address(address_bits(address_of(digits[0])) + cursor)) =
            u8(48 + remaining % 10);
        remaining /= 10;
    }
    host.append(output, unsafe_address(address_bits(address_of(digits[0])) + cursor), 10u64 - cursor);
}

void name_boolean(u32 value, address output, Host host) on cpu requires nonzero(host.append) {
    if (value != 0) { name_token("true", output, host); }
    else { name_token("false", output, host); }
}

void name_shape(address source, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    Shape value = host.shape(source);
    if (value.kind == 0) { name_decimal(value.literal, output, host); }
    else if (value.kind == 1) { name_bytes(value.name, output, host); }
    else { write_expression_name(value.expression, output, host); }
}

void name_arguments(Span values, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    u64 index = 0u64;
    while (index < values.count) {
        if (index != 0u64) { byte(u8(44), output, host.append); }
        Argument value = host.argument(item(values, index));
        if (value.kind == 0) { write_source_type_name(value.value, output, host); }
        else { name_shape(value.value, output, host); }
        index += 1u64;
    }
}

void name_expressions(Span values, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    u64 index = 0u64;
    while (index < values.count) {
        if (index != 0u64) { byte(u8(44), output, host.append); }
        write_expression_name(item(values, index), output, host);
        index += 1u64;
    }
}

void name_hexadecimal(u64 value, address output, Host host) on cpu requires nonzero(host.append) {
    array<u8,16> digits;
    for (index in 0..16) {
        u32 digit = u32((value >> (u64(15 - index) * 4u64)) & 15u64);
        digits[index] = digit < 10 ? u8(48 + digit) : u8(87 + digit);
    }
    host.append(output, address_of(digits[0]), 16u64);
}

void name_condition(address source, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    Condition value = host.condition(source);
    if (value.kind == 0 || value.kind == 1) { name_bytes(value.name, output, host); }
    else if (value.kind == 2) { name_boolean(value.literal, output, host); }
    else if (value.kind == 6) { name_decimal(value.literal, output, host); }
    else if (value.kind == 3) {
        name_condition(value.a, output, host);
        byte(u8(46), output, host.append);
        name_bytes(value.name, output, host);
    } else if (value.kind == 4 || value.kind == 5) {
        byte(u8(40), output, host.append);
        if (value.kind == 5) { name_condition(value.a, output, host); }
        name_bytes(value.name, output, host);
        name_condition(value.kind == 4 ? value.a : value.b, output, host);
        byte(u8(41), output, host.append);
    } else {
        Constant constant = host.constant(value.constant);
        name_bytes(host.ty(constant.ty).name, output, host);
        byte(u8(91), output, host.append);
        u64 index = 0u64;
        while (index < constant.bits.count) {
            if (index != 0u64) { byte(u8(44), output, host.append); }
            name_hexadecimal(unsafe_position<u64>(item(constant.bits, index)), output, host);
            index += 1u64;
        }
        byte(u8(93), output, host.append);
    }
}

extern "C" void write_contract_names(Span values, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    u64 index = 0u64;
    while (index < values.count) {
        Predicate value = host.predicate(item(values, index));
        if (value.kind == 1) {
            name_token(" requires nonzero(", output, host);
            name_bytes(value.first, output, host);
            byte(u8(41), output, host.append);
        } else if (value.kind == 4) {
            name_token(" ensures memory(", output, host);
            if (value.access_present == 0) { name_token("none", output, host); }
            else if (value.access == 0) { name_token("value", output, host); }
            else if (value.access == 1) { name_token("read", output, host); }
            else if (value.access == 2) { name_token("write", output, host); }
            else if (value.access == 3) { name_token("inout", output, host); }
            else { name_token("atomic", output, host); }
            byte(u8(41), output, host.append);
        } else if (value.kind == 8) {
            name_token(" ensures initialized(", output, host);
            name_bytes(value.first, output, host);
            byte(u8(44), output, host.append);
            name_condition(value.condition, output, host);
            byte(u8(41), output, host.append);
        }
        index += 1u64;
    }
}

extern "C" void write_source_type_name(address source, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    TypeName value = host.type_name(source);
    if (value.tile != 0) { name_token("tile<", output, host); }
    if (value.view != 0) { name_token("view<", output, host); }
    if (value.dataset != 0) { name_token("dataset<", output, host); }
    name_bytes(value.name, output, host);
    if (value.arguments.count != 0u64) {
        byte(u8(60), output, host.append);
        name_arguments(value.arguments, output, host);
        byte(u8(62), output, host.append);
    }
    u32 wrappers = value.tile + value.view + value.dataset;
    for (index in 0..wrappers) {
        if (address_bits(value.count) != 0u64) {
            byte(u8(44), output, host.append);
            name_shape(value.count, output, host);
        }
        byte(u8(62), output, host.append);
    }
    write_contract_names(value.contracts, output, host);
}

extern "C" void write_expression_name(address source, address output, Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    Expression value = host.expression(source);
    if (value.kind == 0 || value.kind == 3) { name_bytes(value.name, output, host); }
    else if (value.kind == 1) { name_decimal(value.literal, output, host); }
    else if (value.kind == 2) { name_boolean(value.literal, output, host); }
    else if (value.kind == 4) {
        write_expression_name(value.a, output, host);
        byte(u8(46), output, host.append);
        name_bytes(value.name, output, host);
    } else if (value.kind == 5) {
        write_expression_name(value.a, output, host);
        byte(u8(91), output, host.append);
        write_expression_name(value.b, output, host);
        byte(u8(93), output, host.append);
    } else if (value.kind == 6 || value.kind == 7 || value.kind == 8) {
        if (value.kind == 6) { name_bytes(value.name, output, host); }
        else if (value.kind == 7) { write_expression_name(value.a, output, host); }
        else { write_source_type_name(value.ty, output, host); }
        byte(u8(40), output, host.append);
        name_expressions(value.arguments, output, host);
        byte(u8(41), output, host.append);
    } else if (value.kind == 9 || value.kind == 10 || value.kind == 11) {
        if (value.kind == 11) { name_token("cooperate ", output, host); }
        name_bytes(value.name, output, host);
        if (value.kind != 11 || value.types_present != 0) {
            byte(u8(60), output, host.append);
            name_arguments(value.types, output, host);
            byte(u8(62), output, host.append);
        }
        if (value.kind != 10) {
            byte(u8(40), output, host.append);
            name_expressions(value.arguments, output, host);
            byte(u8(41), output, host.append);
        }
    } else if (value.kind == 12) {
        name_token("target(", output, host);
        if (value.target == 0) { name_token("compute", output, host); }
        else if (value.target == 1) { name_token("cpu", output, host); }
        else { name_token("gpu", output, host); }
        byte(u8(44), output, host.append);
        write_expression_name(value.a, output, host);
        byte(u8(41), output, host.append);
    } else {
        byte(u8(40), output, host.append);
        if (value.kind == 13) {
            name_bytes(value.name, output, host);
            write_expression_name(value.a, output, host);
        } else {
            write_expression_name(value.a, output, host);
            if (value.kind == 14) { name_bytes(value.name, output, host); }
            else { byte(u8(63), output, host.append); }
            write_expression_name(value.b, output, host);
            if (value.kind == 15) {
                byte(u8(58), output, host.append);
                write_expression_name(value.c, output, host);
            }
        }
        byte(u8(41), output, host.append);
    }
}


bool source_token<const N:u32 in {5,8,9,10,11,12,13,15}>(Bytes value, read array<u8,N> token) on cpu {
    if (value.count != u64(N)) { return false; }
    for (index in 0..N) {
        u8 byte = unsafe_position<u8>(unsafe_address(address_bits(value.data) + u64(index)));
        if (byte != token[index]) { return false; }
    }
    return true;
}

bool source_equal(Bytes left, Bytes right) on cpu {
    if (left.count != right.count) { return false; }
    u64 index = 0u64;
    while (index < left.count) {
        u8 a = unsafe_position<u8>(unsafe_address(address_bits(left.data) + index));
        u8 b = unsafe_position<u8>(unsafe_address(address_bits(right.data) + index));
        if (a != b) { return false; }
        index += 1u64;
    }
    return true;
}

bool source_wrapped(TypeName value) on cpu {
    return value.dataset != 0 || value.view != 0 || value.tile != 0;
}

extern "C" SourceBasicKind source_basic_kind(address source, Host host) on cpu requires nonzero(host.type_name) {
    TypeName value = host.type_name(source);
    if (source_wrapped(value) || value.arguments.count != 0u64) { return SourceBasicKind::Other; }
    if (source_token(value.name, "acceleration")) { return SourceBasicKind::Acceleration; }
    if (source_token(value.name, "never") && address_bits(value.count) == 0u64 && value.contracts_present == 0) {
        return SourceBasicKind::Never;
    }
    return SourceBasicKind::Other;
}

extern "C" u32 source_mentions_owned(address source, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    TypeName value = host.type_name(source);
    if (value.dataset != 0) { return 1; }
    u64 index = 0u64;
    while (index < value.arguments.count) {
        Argument argument = host.argument(item(value.arguments, index));
        if (argument.kind == 0 && source_mentions_owned(argument.value, host) != 0) { return 1; }
        index += 1u64;
    }
    return 0;
}

extern "C" u32 source_mentions(address source, Bytes name, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    TypeName value = host.type_name(source);
    if (source_equal(value.name, name)) { return 1; }
    u64 index = 0u64;
    while (index < value.arguments.count) {
        Argument argument = host.argument(item(value.arguments, index));
        if (argument.kind == 0 && source_mentions(argument.value, name, host) != 0) { return 1; }
        index += 1u64;
    }
    return 0;
}

extern "C" SourceFormat source_format_name(Bytes name) on cpu {
    if (source_token(name, "rgba32_float")) { return SourceFormat(1, SourceTexelFormat::Rgba32Float); }
    if (source_token(name, "rgba8_unorm")) { return SourceFormat(1, SourceTexelFormat::Rgba8Unorm); }
    if (source_token(name, "rgba8_srgb")) { return SourceFormat(1, SourceTexelFormat::Rgba8Srgb); }
    if (source_token(name, "r8_unorm")) { return SourceFormat(1, SourceTexelFormat::R8Unorm); }
    if (source_token(name, "rg16_unorm")) { return SourceFormat(1, SourceTexelFormat::Rg16Unorm); }
    if (source_token(name, "rgb10a2_unorm")) { return SourceFormat(1, SourceTexelFormat::Rgb10A2Unorm); }
    if (source_token(name, "r11g11b10_float")) { return SourceFormat(1, SourceTexelFormat::R11G11B10Float); }
    if (source_token(name, "r16_float")) { return SourceFormat(1, SourceTexelFormat::R16Float); }
    return SourceFormat(0, SourceTexelFormat::Rgba32Float);
}

extern "C" u32 source_argument_shape(address source, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    Argument argument = host.argument(source);
    if (argument.kind == 1) { return 1; }
    TypeName value = host.type_name(argument.value);
    return value.arguments.count == 0u64 && !source_wrapped(value) ? 1 : 0;
}

SourceFormat source_format_argument(TypeName value, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    SourceFormat missing = SourceFormat(0, SourceTexelFormat::Rgba32Float);
    if (value.arguments.count != 1u64) { return missing; }
    Argument argument = host.argument(item(value.arguments, 0u64));
    if (argument.kind != 0) { return missing; }
    TypeName format = host.type_name(argument.value);
    if (format.canonical != 0 || format.arguments.count != 0u64 || source_wrapped(format)
        || address_bits(format.count) != 0u64 || format.contracts_present != 0) { return missing; }
    return source_format_name(format.name);
}

SourceTexture classify_source_texture(TypeName value, u32 arrays, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    SourceFormat no_format = SourceFormat(0, SourceTexelFormat::Rgba32Float);
    SourceTexture missing = SourceTexture(SourceTextureKind::Missing, no_format, address());
    if (source_wrapped(value)) { return missing; }
    if (arrays != 0 && source_token(value.name, "array")) {
        if (value.arguments.count != 2u64) { return missing; }
        Argument element = host.argument(item(value.arguments, 0u64));
        if (element.kind != 0) { return missing; }
        address length = item(value.arguments, 1u64);
        if (source_argument_shape(length, host) == 0) { return missing; }
        SourceTexture result = classify_source_texture(host.type_name(element.value), 0, host);
        if (result.kind == SourceTextureKind::Missing) { return missing; }
        result.length = length;
        return result;
    }
    SourceTextureKind kind;
    if (source_token(value.name, "texture2d")) { kind = SourceTextureKind::D2; }
    else if (source_token(value.name, "texture3d")) { kind = SourceTextureKind::D3; }
    else { return missing; }
    SourceFormat format = no_format;
    if (value.arguments.count != 0u64) {
        format = source_format_argument(value, host);
        if (format.present == 0) { return missing; }
    }
    return SourceTexture(kind, format, address());
}

extern "C" SourceTexture source_texture(address source, u32 arrays, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    return classify_source_texture(host.type_name(source), arrays, host);
}

extern "C" SourceFormat source_texture_format(address source, Host host) on cpu requires nonzero(host.type_name) requires nonzero(host.argument) {
    TypeName value = host.type_name(source);
    if (source_token(value.name, "array")) {
        SourceTexture texture = classify_source_texture(value, 1, host);
        if (texture.kind != SourceTextureKind::Missing) { return texture.format; }
    }
    return source_format_argument(value, host);
}


struct SourceAccessName { u32 count; array<u8,6> bytes; };
SourceAccessName source_access_name(u32 access) on cpu {
    if (access==1) { return SourceAccessName(4,"read\0\0"); }
    if (access==2) { return SourceAccessName(5,"write\0"); }
    if (access==3) { return SourceAccessName(5,"inout\0"); }
    if (access==4) { return SourceAccessName(6,"atomic"); }
    return SourceAccessName(0,"\0\0\0\0\0\0");
}
Bytes source_syntax_base(Bytes name) on cpu {
    u64 count=0u64;
    while (count<name.count) {
        u8 value=unsafe_position<u8>(unsafe_address(address_bits(name.data)+count));
        if (value==60) { break; }
        count+=1u64;
    }
    return Bytes(name.data,count);
}
u32 source_syntax_canonical(Bytes name) on cpu {
    u64 index=1u64;
    while (index<name.count) {
        u8 previous=unsafe_position<u8>(unsafe_address(address_bits(name.data)+index-1u64));
        u8 current=unsafe_position<u8>(unsafe_address(address_bits(name.data)+index));
        if (previous==58&&current==58) { return 1; }
        index+=1u64;
    }
    return 0;
}
void source_syntax_named<const N:u32 in {3,5}>(address output,read array<u8,N> name,u32 canonical,u64 arguments,SourceSyntaxHost construct) on cpu requires nonzero(construct.name) {
    construct.name(output,Bytes(address_of(name[0]),u64(N)),canonical,arguments);
}
extern "C" void function_syntax(address source,address output,Host host,SourceSyntaxHost construct) on cpu requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(construct.name) requires nonzero(construct.name_argument) requires nonzero(construct.contracts) {
    Function value=host.function(source);
    source_syntax_named(output,"cfn",0,value.parameters.count+1u64,construct);
    construct.name_argument(value.result,output,Bytes(address(),0u64));
    u64 index=0u64;
    while (index<value.parameters.count) {
        Parameter parameter=host.parameter(item(value.parameters,index));
        SourceAccessName wrapper=source_access_name(parameter.access);
        construct.name_argument(parameter.ty,output,Bytes(address_of(wrapper.bytes[0]),u64(wrapper.count)));
        index+=1u64;
    }
    if (value.contracts.count!=0u64) { construct.contracts(source,output); }
}
extern "C" void type_syntax(address source,address output,Host host,SourceSyntaxHost construct) on cpu requires nonzero(host.ty) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(construct.name) requires nonzero(construct.type_argument) requires nonzero(construct.name_argument) requires nonzero(construct.contracts) requires nonzero(construct.dataset) requires nonzero(construct.view) requires nonzero(construct.value_argument) requires nonzero(construct.release_argument) {
    Type value=host.ty(source);
    if (address_bits(value.function)!=0u64) { function_syntax(value.function,output,host,construct);return; }
    if (value.cleanup_present!=0) {
        source_syntax_named(output,"owned",0,2u64,construct);
        construct.type_argument(value.arguments,0u64,output);
        construct.release_argument(output,value.cleanup);
        return;
    }
    if (address_bits(value.dataset)!=0u64) {
        u32 present=value.values.count!=0u64?1:0;
        u32 count=0;
        if (present!=0) { count=unsafe_position<u32>(item(value.values,0u64)); }
        construct.dataset(value.dataset,output,present,count);
        return;
    }
    if (address_bits(value.array)!=0u64) {
        source_syntax_named(output,"array",0,2u64,construct);
        construct.type_argument(Span(value.array,1u64,value.arguments.stride),0u64,output);
        construct.value_argument(value.values,0u64,output);
        return;
    }
    if (address_bits(value.view)!=0u64) { type_syntax(value.view,output,host,construct);construct.view(output);return; }
    Bytes name=source_syntax_base(value.name);
    construct.name(output,name,source_syntax_canonical(name),value.arguments.count+value.values.count);
    u64 index=0u64;
    while (index<value.arguments.count) { construct.type_argument(value.arguments,index,output);index+=1u64; }
    index=0u64;
    while (index<value.values.count) { construct.value_argument(value.values,index,output);index+=1u64; }
}
extern "C" void write_function_name(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.condition) requires nonzero(host.constant) {
    Function value=host.function(source);
    name_token("cfn",output,host);byte(u8(60),output,host.append);
    write_source_type_name(value.result,output,host);
    u64 index=0u64;
    while (index<value.parameters.count) {
        byte(u8(44),output,host.append);
        Parameter parameter=host.parameter(item(value.parameters,index));
        SourceAccessName wrapper=source_access_name(parameter.access);
        if (wrapper.count!=0) {
            name_bytes(Bytes(address_of(wrapper.bytes[0]),u64(wrapper.count)),output,host);
            byte(u8(60),output,host.append);
        }
        write_source_type_name(parameter.ty,output,host);
        if (wrapper.count!=0) { byte(u8(62),output,host.append); }
        index+=1u64;
    }
    byte(u8(62),output,host.append);
    write_contract_names(value.contracts,output,host);
}


ReadBytes take_bytes(Bytes input,inout u64 position,u64 size) on cpu {
 if(size>0xffffffffffffffffu64-position){return ReadBytes(Bytes(address(),0u64),ReadStatus::Overflow);}
 u64 end=position+size;
 if(end>input.count){return ReadBytes(Bytes(address(),0u64),ReadStatus::Truncated);}
 Bytes value=Bytes(unsafe_address(address_bits(input.data)+position),size);position=end;
 return ReadBytes(value,ReadStatus::Success);
}

ReadValue fixed_value(Bytes input,inout u64 position,u32 bytes) on cpu requires range(bytes,1,8) {
 ReadBytes part=take_bytes(input,position,u64(bytes));
 if(part.status!=ReadStatus::Success){return ReadValue(0u64,part.status);}
 u64 value=0u64;u32 index=0;
 while(index<bytes){value|=u64(unsafe_position<u8>(unsafe_address(address_bits(part.value.data)+u64(index))))<<(u64(index)*8u64);index+=1;}
 return ReadValue(value,ReadStatus::Success);
}

ReadValue read_boolean(Bytes input,inout u64 position) on cpu {
 ReadValue value=fixed_value(input,position,1);
 if(value.status!=ReadStatus::Success){return value;}
 if(value.value>1u64){return ReadValue(0u64,ReadStatus::Boolean);}
 return value;
}

ReadBytes counted_bytes(Bytes input,inout u64 position) on cpu {
 ReadValue size=fixed_value(input,position,8);
 if(size.status!=ReadStatus::Success){return ReadBytes(Bytes(address(),0u64),size.status);}
 return take_bytes(input,position,size.value);
}

ReadAccess read_access(Bytes input,inout u64 position) on cpu {
 ReadValue value=fixed_value(input,position,4);
 if(value.status!=ReadStatus::Success){return ReadAccess(AccessTag::Value,value.status);}
 if(value.value==0u64){return ReadAccess(AccessTag::Value,ReadStatus::Success);}
 if(value.value==1u64){return ReadAccess(AccessTag::Read,ReadStatus::Success);}
 if(value.value==2u64){return ReadAccess(AccessTag::Write,ReadStatus::Success);}
 if(value.value==3u64){return ReadAccess(AccessTag::InOut,ReadStatus::Success);}
 if(value.value==4u64){return ReadAccess(AccessTag::Atomic,ReadStatus::Success);}
 return ReadAccess(AccessTag::Value,ReadStatus::Access);
}

ReadTarget read_target(Bytes input,inout u64 position) on cpu {
 ReadValue value=fixed_value(input,position,4);
 if(value.status!=ReadStatus::Success){return ReadTarget(TargetTag::Compute,value.status);}
 if(value.value==0u64){return ReadTarget(TargetTag::Compute,ReadStatus::Success);}
 if(value.value==1u64){return ReadTarget(TargetTag::Cpu,ReadStatus::Success);}
 if(value.value==2u64){return ReadTarget(TargetTag::Gpu,ReadStatus::Success);}
 return ReadTarget(TargetTag::Compute,ReadStatus::Target);
}

ReadScalar read_scalar(Bytes input,inout u64 position) on cpu {
 ReadValue value=fixed_value(input,position,4);
 if(value.status!=ReadStatus::Success){return ReadScalar(ScalarTag::F32,value.status);}
 if(value.value==0u64){return ReadScalar(ScalarTag::F32,ReadStatus::Success);}
 if(value.value==1u64){return ReadScalar(ScalarTag::U32,ReadStatus::Success);}
 if(value.value==2u64){return ReadScalar(ScalarTag::I32,ReadStatus::Success);}
 if(value.value==3u64){return ReadScalar(ScalarTag::Bool,ReadStatus::Success);}
 if(value.value==4u64){return ReadScalar(ScalarTag::U64,ReadStatus::Success);}
 if(value.value==5u64){return ReadScalar(ScalarTag::I64,ReadStatus::Success);}
 if(value.value==6u64){return ReadScalar(ScalarTag::F64,ReadStatus::Success);}
 if(value.value==7u64){return ReadScalar(ScalarTag::U8,ReadStatus::Success);}
 if(value.value==8u64){return ReadScalar(ScalarTag::I8,ReadStatus::Success);}
 if(value.value==9u64){return ReadScalar(ScalarTag::U16,ReadStatus::Success);}
 if(value.value==10u64){return ReadScalar(ScalarTag::I16,ReadStatus::Success);}
 return ReadScalar(ScalarTag::F32,ReadStatus::Scalar);
}

ReadStatus read_text_field(Bytes input,inout u64 position,address output,address context,cfn<ReadStatus,address,address,Bytes> text) on cpu requires nonzero(text){
 ReadBytes value=counted_bytes(input,position);if(value.status!=ReadStatus::Success){return value.status;}return text(context,output,value.value);
}

ReadStatus read_word_field(Bytes input,inout u64 position,address output) on cpu {
 ReadValue value=fixed_value(input,position,4);
 if(value.status!=ReadStatus::Success){return value.status;}
 unsafe_position<u32>(output)=u32(value.value);return ReadStatus::Success;
}

ReadStatus read_bool_field(Bytes input,inout u64 position,address output) on cpu {
 ReadValue value=read_boolean(input,position);
 if(value.status!=ReadStatus::Success){return value.status;}
 // This slot is the original Rust bool, whose physical storage is one byte.
 unsafe_position<u8>(output)=u8(value.value);return ReadStatus::Success;
}

ReadStatus read_owned_option(Bytes input,inout u64 position,address output,address context,
 cfn<ReadStatus,address,address> construct) on cpu requires nonzero(construct) {
 ReadValue present=read_boolean(input,position);
 if(present.status!=ReadStatus::Success){return present.status;}
 if(present.value!=0u64){return construct(context,output);}
 return ReadStatus::Success;
}

ReadStatus read_owned_values(Bytes input,inout u64 position,address output,address context,
 cfn<ReadStatus,address,address> push) on cpu requires nonzero(push) {
 ReadValue count=fixed_value(input,position,8);
 if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){ReadStatus status=push(context,output);if(status!=ReadStatus::Success){return status;}index+=1u64;}
 return ReadStatus::Success;
}

ReadStatus read_u32_values(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){ReadValue value=fixed_value(input,position,4);if(value.status!=ReadStatus::Success){return value.status;}host.push_u32(output,u32(value.value));index+=1u64;}
 return ReadStatus::Success;
}

ReadStatus read_u64_values(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){ReadValue value=fixed_value(input,position,8);if(value.status!=ReadStatus::Success){return value.status;}host.push_u64(output,value.value);index+=1u64;}
 return ReadStatus::Success;
}

ReadStatus read_components(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){
  ReadScalar scalar=read_scalar(input,position);if(scalar.status!=ReadStatus::Success){return scalar.status;}
  ReadValue word=fixed_value(input,position,4);if(word.status!=ReadStatus::Success){return word.status;}
  host.push_component(output,scalar.value,u32(word.value));index+=1u64;
 }
 return ReadStatus::Success;
}

ReadStatus read_string_option(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue present=read_boolean(input,position);if(present.status!=ReadStatus::Success){return present.status;}
 if(present.value==0u64){return ReadStatus::Success;}
 ReadBytes value=counted_bytes(input,position);if(value.status!=ReadStatus::Success){return value.status;}
 return host.string_option(host.context,output,value.value);
}

extern "C" ReadStatus read_named_type(Bytes input,inout u64 position,address name,address word,address ty,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadStatus read_step_1=read_text_field(input,position,name,host.context,host.text);if(read_step_1!=ReadStatus::Success){return read_step_1;}
 ReadStatus read_step_2=read_word_field(input,position,word);if(read_step_2!=ReadStatus::Success){return read_step_2;}
 return read_type(input,position,ty,host);
}

extern "C" ReadStatus read_parameter(Bytes input,inout u64 position,address access,address ty,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadAccess value=read_access(input,position);if(value.status!=ReadStatus::Success){return value.status;}
 host.access(access,value.value);return read_type_name(input,position,ty,host);
}

extern "C" ReadStatus read_interval(Bytes input,inout u64 position,address lower,address upper,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadStatus read_step_3=read_condition(input,position,lower,host);if(read_step_3!=ReadStatus::Success){return read_step_3;}
 return read_condition(input,position,upper,host);
}

extern "C" ReadStatus read_type(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 TypeFields fields=host.type_fields(output);
 ReadStatus read_step_4=read_text_field(input,position,fields.name,host.context,host.text);if(read_step_4!=ReadStatus::Success){return read_step_4;}
 ReadStatus read_step_5=read_owned_values(input,position,fields.arguments,host.context,host.push_type);if(read_step_5!=ReadStatus::Success){return read_step_5;}
 ReadStatus read_step_6=read_u32_values(input,position,fields.values,host);if(read_step_6!=ReadStatus::Success){return read_step_6;}
 ReadStatus read_step_7=read_word_field(input,position,fields.size);if(read_step_7!=ReadStatus::Success){return read_step_7;}
 ReadStatus read_step_8=read_word_field(input,position,fields.alignment);if(read_step_8!=ReadStatus::Success){return read_step_8;}
 ReadStatus read_step_9=read_components(input,position,fields.components,host);if(read_step_9!=ReadStatus::Success){return read_step_9;}
 ReadStatus read_step_10=read_owned_values(input,position,fields.fields,host.context,host.push_field);if(read_step_10!=ReadStatus::Success){return read_step_10;}
 ReadStatus read_step_11=read_owned_option(input,position,fields.view,host.context,host.type_box);if(read_step_11!=ReadStatus::Success){return read_step_11;}
 ReadStatus read_step_12=read_owned_option(input,position,fields.array,host.context,host.type_box);if(read_step_12!=ReadStatus::Success){return read_step_12;}
 ReadStatus read_step_13=read_bool_field(input,position,fields.enumeration);if(read_step_13!=ReadStatus::Success){return read_step_13;}
 ReadStatus read_step_14=read_bool_field(input,position,fields.result);if(read_step_14!=ReadStatus::Success){return read_step_14;}
 ReadStatus read_step_15=read_owned_values(input,position,fields.variants,host.context,host.push_variant);if(read_step_15!=ReadStatus::Success){return read_step_15;}
 ReadStatus read_step_16=read_owned_option(input,position,fields.dataset,host.context,host.type_name_box);if(read_step_16!=ReadStatus::Success){return read_step_16;}
 ReadStatus read_step_17=read_string_option(input,position,fields.cleanup,host);if(read_step_17!=ReadStatus::Success){return read_step_17;}
 return read_owned_option(input,position,fields.function,host.context,host.function_box);
}

extern "C" ReadStatus read_type_name(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 NameFields fields=host.name_fields(output);
 ReadStatus read_step_18=read_text_field(input,position,fields.name,host.context,host.text);if(read_step_18!=ReadStatus::Success){return read_step_18;}
 ReadStatus read_step_19=read_bool_field(input,position,fields.canonical);if(read_step_19!=ReadStatus::Success){return read_step_19;}
 ReadStatus read_step_20=read_arguments(input,position,fields.arguments,host);if(read_step_20!=ReadStatus::Success){return read_step_20;}
 ReadStatus read_step_21=read_bool_field(input,position,fields.dataset);if(read_step_21!=ReadStatus::Success){return read_step_21;}
 ReadStatus read_step_22=read_bool_field(input,position,fields.view);if(read_step_22!=ReadStatus::Success){return read_step_22;}
 ReadStatus read_step_23=read_bool_field(input,position,fields.tile);if(read_step_23!=ReadStatus::Success){return read_step_23;}
 ReadStatus read_step_24=read_owned_option(input,position,fields.count,host.context,host.shape_option);if(read_step_24!=ReadStatus::Success){return read_step_24;}
 return read_owned_option(input,position,fields.contracts,host.context,host.predicates_option);
}

extern "C" ReadStatus read_function(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 FunctionFields fields=host.function_fields(output);
 ReadStatus read_step_25=read_type_name(input,position,fields.result,host);if(read_step_25!=ReadStatus::Success){return read_step_25;}
 ReadStatus read_step_26=read_owned_values(input,position,fields.parameters,host.context,host.push_parameter);if(read_step_26!=ReadStatus::Success){return read_step_26;}
 return read_predicates(input,position,fields.contracts,host);
}

ReadStatus read_constant(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ConstantFields fields=host.constant_fields(output);
 ReadStatus read_step_27=read_type(input,position,fields.ty,host);if(read_step_27!=ReadStatus::Success){return read_step_27;}
 return read_u64_values(input,position,fields.bits,host);
}

extern "C" ReadStatus read_arguments(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){
  ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
  ArgumentTag kind;
  if(tag.value==0u64){kind=ArgumentTag::Type;}else if(tag.value==1u64){kind=ArgumentTag::Value;}else{return ReadStatus::Argument;}
  ReadStatus read_step_28=host.push_argument(host.context,output,kind);if(read_step_28!=ReadStatus::Success){return read_step_28;}
  index+=1u64;
 }
 return ReadStatus::Success;
}

extern "C" ReadStatus read_predicates(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 return read_owned_values(input,position,output,host.context,host.push_predicate);
}

extern "C" ReadStatus read_shape(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){
  ReadValue value=fixed_value(input,position,4);if(value.status!=ReadStatus::Success){return value.status;}
  host.shape_literal(output,u32(value.value));return ReadStatus::Success;
 }
 if(tag.value==1u64){ReadBytes value=counted_bytes(input,position);if(value.status!=ReadStatus::Success){return value.status;}return host.shape_name(host.context,output,value.value);}
 if(tag.value==2u64){return host.shape_expression(host.context,output);}
 return ReadStatus::Shape;
}

extern "C" ReadStatus read_expression(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 ExpressionTag kind;
 if(tag.value==0u64){kind=ExpressionTag::Number;}
 else if(tag.value==1u64){kind=ExpressionTag::StaticU32;}
 else if(tag.value==2u64){kind=ExpressionTag::Bool;}
 else if(tag.value==3u64){kind=ExpressionTag::Name;}
 else if(tag.value==4u64){kind=ExpressionTag::Member;}
 else if(tag.value==5u64){kind=ExpressionTag::Index;}
 else if(tag.value==6u64){kind=ExpressionTag::Call;}
 else if(tag.value==7u64){kind=ExpressionTag::Invoke;}
 else if(tag.value==8u64){kind=ExpressionTag::Construct;}
 else if(tag.value==9u64){kind=ExpressionTag::Specialize;}
 else if(tag.value==10u64){kind=ExpressionTag::FunctionRef;}
 else if(tag.value==11u64){kind=ExpressionTag::Cooperate;}
 else if(tag.value==12u64){kind=ExpressionTag::Target;}
 else if(tag.value==13u64){kind=ExpressionTag::Unary;}
 else if(tag.value==14u64){kind=ExpressionTag::Binary;}
 else if(tag.value==15u64){kind=ExpressionTag::Select;}
 else{return ReadStatus::Expression;}
 ReadStatus read_step_29=host.expression(host.context,output,kind);if(read_step_29!=ReadStatus::Success){return read_step_29;}
 ReadValue start=fixed_value(input,position,8);if(start.status!=ReadStatus::Success){return start.status;}
 ReadValue end=fixed_value(input,position,8);if(end.status!=ReadStatus::Success){return end.status;}
 host.expression_span(output,start.value,end.value);return ReadStatus::Success;
}

ReadStatus read_condition(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 ConditionTag kind;
 if(tag.value==0u64){kind=ConditionTag::Name;}else if(tag.value==1u64){kind=ConditionTag::Number;}
 else if(tag.value==2u64){kind=ConditionTag::Bool;}else if(tag.value==3u64){kind=ConditionTag::Member;}
 else if(tag.value==4u64){kind=ConditionTag::Unary;}else if(tag.value==5u64){kind=ConditionTag::Binary;}
 else if(tag.value==6u64){kind=ConditionTag::StaticU32;}else if(tag.value==7u64){kind=ConditionTag::Constant;}
 else{return ReadStatus::Condition;}
 return host.condition(host.context,output,kind);
}

extern "C" ReadStatus read_predicate(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 PredicateTag kind;
 if(tag.value==0u64){kind=PredicateTag::Prefix;}else if(tag.value==1u64){kind=PredicateTag::Nonzero;}
 else if(tag.value==2u64){kind=PredicateTag::Nonempty;}else if(tag.value==3u64){kind=PredicateTag::Index;}
 else if(tag.value==4u64){kind=PredicateTag::Contiguous;}else if(tag.value==5u64){kind=PredicateTag::Initialized;}
 else if(tag.value==6u64){kind=PredicateTag::SameLength;}else if(tag.value==7u64){kind=PredicateTag::Cleanup;}
 else if(tag.value==8u64){kind=PredicateTag::Memory;}else if(tag.value==9u64){kind=PredicateTag::Range;}
 else{return ReadStatus::Predicate;}
 return host.predicate(host.context,output,kind);
}

extern "C" ReadStatus read_expression_payload(ExpressionTag kind,ExpressionFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 if(kind==ExpressionTag::Number||kind==ExpressionTag::Name){return read_text_field(input,position,fields.text,host.context,host.text);}
 if(kind==ExpressionTag::StaticU32){return read_word_field(input,position,fields.literal);}
 if(kind==ExpressionTag::Bool){return read_bool_field(input,position,fields.literal);}
 if(kind==ExpressionTag::Member){ReadStatus read_step_30=read_expression(input,position,fields.a,host);if(read_step_30!=ReadStatus::Success){return read_step_30;}return read_text_field(input,position,fields.text,host.context,host.text);}
 if(kind==ExpressionTag::Index){ReadStatus read_step_31=read_expression(input,position,fields.a,host);if(read_step_31!=ReadStatus::Success){return read_step_31;}return read_expression(input,position,fields.b,host);}
 if(kind==ExpressionTag::Call){ReadStatus read_step_32=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_32!=ReadStatus::Success){return read_step_32;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Invoke){ReadStatus read_step_33=read_expression(input,position,fields.a,host);if(read_step_33!=ReadStatus::Success){return read_step_33;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Construct){ReadStatus read_step_34=read_type_name(input,position,fields.ty,host);if(read_step_34!=ReadStatus::Success){return read_step_34;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Specialize){ReadStatus read_step_35=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_35!=ReadStatus::Success){return read_step_35;}ReadStatus read_step_36=read_arguments(input,position,fields.types,host);if(read_step_36!=ReadStatus::Success){return read_step_36;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::FunctionRef){ReadStatus read_step_37=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_37!=ReadStatus::Success){return read_step_37;}return read_arguments(input,position,fields.types,host);}
 if(kind==ExpressionTag::Cooperate){ReadStatus read_step_38=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_38!=ReadStatus::Success){return read_step_38;}ReadStatus read_step_39=read_owned_option(input,position,fields.types,host.context,host.arguments_option);if(read_step_39!=ReadStatus::Success){return read_step_39;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Target){ReadTarget target=read_target(input,position);if(target.status!=ReadStatus::Success){return target.status;}host.target(fields.target,target.value);return read_expression(input,position,fields.a,host);}
 if(kind==ExpressionTag::Unary){ReadStatus read_step_40=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_40!=ReadStatus::Success){return read_step_40;}return read_expression(input,position,fields.a,host);}
 if(kind==ExpressionTag::Binary){ReadStatus read_step_41=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_41!=ReadStatus::Success){return read_step_41;}ReadStatus read_step_42=read_expression(input,position,fields.a,host);if(read_step_42!=ReadStatus::Success){return read_step_42;}return read_expression(input,position,fields.b,host);}
 ReadStatus read_step_43=read_expression(input,position,fields.a,host);if(read_step_43!=ReadStatus::Success){return read_step_43;}
 ReadStatus read_step_44=read_expression(input,position,fields.b,host);if(read_step_44!=ReadStatus::Success){return read_step_44;}
 return read_expression(input,position,fields.c,host);
}

extern "C" ReadStatus read_condition_payload(ConditionTag kind,ConditionFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 if(kind==ConditionTag::Name||kind==ConditionTag::Number){return read_text_field(input,position,fields.text,host.context,host.text);}
 if(kind==ConditionTag::Bool){return read_bool_field(input,position,fields.literal);}
 if(kind==ConditionTag::StaticU32){return read_word_field(input,position,fields.literal);}
 if(kind==ConditionTag::Member){ReadStatus read_step_45=read_condition(input,position,fields.a,host);if(read_step_45!=ReadStatus::Success){return read_step_45;}return read_text_field(input,position,fields.text,host.context,host.text);}
 if(kind==ConditionTag::Unary){ReadStatus read_step_46=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_46!=ReadStatus::Success){return read_step_46;}return read_condition(input,position,fields.a,host);}
 if(kind==ConditionTag::Binary){ReadStatus read_step_47=read_text_field(input,position,fields.text,host.context,host.text);if(read_step_47!=ReadStatus::Success){return read_step_47;}ReadStatus read_step_48=read_condition(input,position,fields.a,host);if(read_step_48!=ReadStatus::Success){return read_step_48;}return read_condition(input,position,fields.b,host);}
 return read_constant(input,position,fields.constant,host);
}

extern "C" ReadStatus read_predicate_payload(PredicateTag kind,PredicateFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 if(kind==PredicateTag::Memory){
  ReadValue present=read_boolean(input,position);if(present.status!=ReadStatus::Success){return present.status;}
  if(present.value!=0u64){ReadAccess access=read_access(input,position);if(access.status!=ReadStatus::Success){return access.status;}host.optional_access(fields.access,access.value);}
  return ReadStatus::Success;
 }
 ReadStatus read_step_49=read_text_field(input,position,fields.first,host.context,host.text);if(read_step_49!=ReadStatus::Success){return read_step_49;}
 if(kind==PredicateTag::Prefix){ReadStatus read_step_50=read_text_field(input,position,fields.second,host.context,host.text);if(read_step_50!=ReadStatus::Success){return read_step_50;}return read_text_field(input,position,fields.third,host.context,host.text);}
 if(kind==PredicateTag::Index||kind==PredicateTag::SameLength){return read_text_field(input,position,fields.second,host.context,host.text);}
 if(kind==PredicateTag::Initialized||kind==PredicateTag::Cleanup){return read_condition(input,position,fields.condition,host);}
 if(kind==PredicateTag::Range){return read_owned_values(input,position,fields.intervals,host.context,host.push_interval);}
 return ReadStatus::Success;
}

extern "C" void contract_write_sequence(u64 count,address iterator,address output,cfn<address,address> next,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(next) requires nonzero(element) requires nonzero(append){
 wide(count,output,append);u64 index=0u64;while(index<count){address value=next(iterator);element(value,output);index+=1u64;}
}

extern "C" ReadStatus contract_read_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push){
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}u64 index=0u64;
 while(index<count.value){ReadStatus status=push(context,output);if(status!=ReadStatus::Success){return status;}index+=1u64;}return ReadStatus::Success;
}

extern "C" void contract_write_option(address value,address output,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(element) requires nonzero(append){
 bool present=address_bits(value)!=0u64;byte(present?u8(1):u8(0),output,append);if(present){element(value,output);}
}

extern "C" ReadStatus contract_read_option(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> some,cfn<void,address> none) on cpu requires nonzero(some) requires nonzero(none){
 ReadValue present=read_boolean(input,position);if(present.status!=ReadStatus::Success){return present.status;}if(present.value==0u64){none(output);return ReadStatus::Success;}return some(context,output);
}

extern "C" void contract_write_trimmed(Span values,address output,cfn<u32,address> is_empty,cfn<void,address,address> element,cfn<void,address,address,u64> append) on cpu requires nonzero(is_empty) requires nonzero(element) requires nonzero(append){
 u64 width=values.count;while(width!=0u64){if(is_empty(item(values,width-1u64))==0){break;}width-=1u64;}wide(width,output,append);u64 index=0u64;while(index<width){element(item(values,index),output);index+=1u64;}
}

extern "C" void contract_write_text(Bytes value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){text(value,output,append);}

extern "C" ReadStatus contract_read_text(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address,Bytes> text) on cpu requires nonzero(text){return read_text_field(input,position,output,context,text);}

extern "C" void contract_write_u8(u8 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){byte(u8(value),output,append);}

extern "C" ReadStatus contract_read_u8(Bytes input,inout u64 position,inout u8 output) on cpu{ReadValue value=fixed_value(input,position,1);if(value.status!=ReadStatus::Success){return value.status;}output=u8(value.value);return ReadStatus::Success;}

extern "C" void contract_write_u16(u16 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){byte(u8(value&255),output,append);byte(u8(value>>8),output,append);}

extern "C" ReadStatus contract_read_u16(Bytes input,inout u64 position,inout u16 output) on cpu{ReadValue value=fixed_value(input,position,2);if(value.status!=ReadStatus::Success){return value.status;}output=u16(value.value);return ReadStatus::Success;}

extern "C" void contract_write_u32(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(value,output,append);}

extern "C" ReadStatus contract_read_u32(Bytes input,inout u64 position,inout u32 output) on cpu{ReadValue value=fixed_value(input,position,4);if(value.status!=ReadStatus::Success){return value.status;}output=u32(value.value);return ReadStatus::Success;}

extern "C" void contract_write_u64(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){wide(value,output,append);}

extern "C" ReadStatus contract_read_u64(Bytes input,inout u64 position,inout u64 output) on cpu{ReadValue value=fixed_value(input,position,8);if(value.status!=ReadStatus::Success){return value.status;}output=u64(value.value);return ReadStatus::Success;}

extern "C" void contract_write_usize(u64 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){wide(value,output,append);}

extern "C" ReadStatus contract_read_usize(Bytes input,inout u64 position,inout u64 output) on cpu{ReadValue value=fixed_value(input,position,8);if(value.status!=ReadStatus::Success){return value.status;}output=u64(value.value);return ReadStatus::Success;}

extern "C" void contract_write_bool(u32 value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){byte(u8(value),output,append);}

extern "C" ReadStatus contract_read_bool(Bytes input,inout u64 position,inout u32 output) on cpu{ReadValue value=read_boolean(input,position);if(value.status!=ReadStatus::Success){return value.status;}output=u32(value.value);return ReadStatus::Success;}

extern "C" void contract_write_buffer(address output,ContractBufferFields fields,ContractBufferWriter host) on cpu requires nonzero(host.name) requires nonzero(host.access) requires nonzero(host.ty) requires nonzero(host.stride) requires nonzero(host.count){host.name(fields.name,output);host.access(fields.access,output);host.ty(fields.ty,output);host.stride(fields.stride,output);host.count(fields.count,output);}

extern "C" ReadStatus contract_read_buffer(address context,ContractBufferFields fields,ContractBufferReader host) on cpu requires nonzero(host.name) requires nonzero(host.access) requires nonzero(host.ty) requires nonzero(host.stride) requires nonzero(host.count){ReadStatus s0=host.name(context,fields.name);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.access(context,fields.access);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.ty(context,fields.ty);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.stride(context,fields.stride);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.count(context,fields.count);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}

extern "C" void contract_write_uniform(address output,ContractUniformFields fields,ContractUniformWriter host) on cpu requires nonzero(host.name) requires nonzero(host.ty) requires nonzero(host.offset){host.name(fields.name,output);host.ty(fields.ty,output);host.offset(fields.offset,output);}

extern "C" ReadStatus contract_read_uniform(address context,ContractUniformFields fields,ContractUniformReader host) on cpu requires nonzero(host.name) requires nonzero(host.ty) requires nonzero(host.offset){ReadStatus s0=host.name(context,fields.name);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.ty(context,fields.ty);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.offset(context,fields.offset);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}

extern "C" void contract_write_texture_dimension_d2(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}

extern "C" void contract_write_texture_dimension_d3(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}

extern "C" void contract_write_texture_format_rgba32float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}

extern "C" void contract_write_texture_format_rgba8unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}

extern "C" void contract_write_texture_format_rgba8srgb(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}

extern "C" void contract_write_texture_format_r8unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}

extern "C" void contract_write_texture_format_rg16unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(4,output,append);}

extern "C" void contract_write_texture_format_rgb10a2unorm(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(5,output,append);}

extern "C" void contract_write_texture_format_r11g11b10float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(6,output,append);}

extern "C" void contract_write_texture_format_r16float(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(7,output,append);}

extern "C" void contract_write_texture_binding(address output,ContractTextureBindingFields fields,ContractTextureBindingWriter host) on cpu requires nonzero(host.name) requires nonzero(host.count) requires nonzero(host.dimension) requires nonzero(host.format){host.name(fields.name,output);host.count(fields.count,output);host.dimension(fields.dimension,output);host.format(fields.format,output);}

extern "C" ReadStatus contract_read_texture_binding(address context,ContractTextureBindingFields fields,ContractTextureBindingReader host) on cpu requires nonzero(host.name) requires nonzero(host.count) requires nonzero(host.dimension) requires nonzero(host.format){ReadStatus s0=host.name(context,fields.name);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.count(context,fields.count);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.dimension(context,fields.dimension);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.format(context,fields.format);if(s3!=ReadStatus::Success){return s3;}return ReadStatus::Success;}

extern "C" void contract_write_prefix_count(address output,ContractPrefixCountFields fields,ContractPrefixCountWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.word){host.binding(fields.binding,output);host.word(fields.word,output);}

extern "C" ReadStatus contract_read_prefix_count(address context,ContractPrefixCountFields fields,ContractPrefixCountReader host) on cpu requires nonzero(host.binding) requires nonzero(host.word){ReadStatus s0=host.binding(context,fields.binding);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.word(context,fields.word);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}

extern "C" void contract_write_prefix_relation(address output,ContractPrefixRelationFields fields,ContractPrefixRelationWriter host) on cpu requires nonzero(host.domain) requires nonzero(host.domain_region) requires nonzero(host.count){host.domain(fields.domain,output);host.domain_region(fields.domain_region,output);host.count(fields.count,output);}

extern "C" ReadStatus contract_read_prefix_relation(address context,ContractPrefixRelationFields fields,ContractPrefixRelationReader host) on cpu requires nonzero(host.domain) requires nonzero(host.domain_region) requires nonzero(host.count){ReadStatus s0=host.domain(context,fields.domain);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.domain_region(context,fields.domain_region);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.count(context,fields.count);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}

extern "C" void contract_write_element_range(address output,ContractElementRangeFields fields,ContractElementRangeWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.scalar) requires nonzero(host.intervals) requires nonzero(host.known_bits){host.binding(fields.binding,output);host.offset(fields.offset,output);host.scalar(fields.scalar,output);host.intervals(fields.intervals,output);host.known_bits(fields.known_bits,output);}

extern "C" ReadStatus contract_read_element_range(address context,ContractElementRangeFields fields,ContractElementRangeReader host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.scalar) requires nonzero(host.intervals) requires nonzero(host.known_bits){ReadStatus s0=host.binding(context,fields.binding);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.offset(context,fields.offset);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.scalar(context,fields.scalar);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.intervals(context,fields.intervals);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.known_bits(context,fields.known_bits);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}

extern "C" void contract_write_element_fact_range(address output,ContractElementFactRangeFields fields,ContractElementFactRangeWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.range) requires nonzero(append){word(0,output,append);host.range(fields.range,output);}

extern "C" ReadStatus contract_read_element_fact_range(address context,ContractElementFactRangeFields fields,ContractElementFactRangeReader host) on cpu requires nonzero(host.range){ReadStatus s0=host.range(context,fields.range);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}

extern "C" void contract_write_element_fact_index(address output,ContractElementFactIndexFields fields,ContractElementFactIndexWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.target) requires nonzero(append){word(1,output,append);host.binding(fields.binding,output);host.offset(fields.offset,output);host.target(fields.target,output);}

extern "C" ReadStatus contract_read_element_fact_index(address context,ContractElementFactIndexFields fields,ContractElementFactIndexReader host) on cpu requires nonzero(host.binding) requires nonzero(host.offset) requires nonzero(host.target){ReadStatus s0=host.binding(context,fields.binding);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.offset(context,fields.offset);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.target(context,fields.target);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}

extern "C" void contract_write_access_value(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}

extern "C" void contract_write_access_read(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}

extern "C" void contract_write_access_write(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}

extern "C" void contract_write_access_inout(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}

extern "C" void contract_write_access_atomic(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(4,output,append);}

extern "C" void contract_write_target_compute(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}

extern "C" void contract_write_target_cpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}

extern "C" void contract_write_target_gpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}

extern "C" void contract_write_call_parameter_kind_value(address output,ContractCallParameterKindValueFields fields,ContractCallParameterKindValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ty) requires nonzero(append){word(0,output,append);host.ty(fields.ty,output);}

extern "C" ReadStatus contract_read_call_parameter_kind_value(address context,ContractCallParameterKindValueFields fields,ContractCallParameterKindValueReader host) on cpu requires nonzero(host.ty){ReadStatus s0=host.ty(context,fields.ty);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}

extern "C" void contract_write_call_parameter_kind_reference(address output,ContractCallParameterKindReferenceFields fields,ContractCallParameterKindReferenceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ty) requires nonzero(host.access) requires nonzero(append){word(1,output,append);host.ty(fields.ty,output);host.access(fields.access,output);}

extern "C" ReadStatus contract_read_call_parameter_kind_reference(address context,ContractCallParameterKindReferenceFields fields,ContractCallParameterKindReferenceReader host) on cpu requires nonzero(host.ty) requires nonzero(host.access){ReadStatus s0=host.ty(context,fields.ty);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.access(context,fields.access);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}

extern "C" void contract_write_contract_kind_requires(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}

extern "C" void contract_write_contract_kind_ensures(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}

extern "C" void contract_write_scalar_f32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}

extern "C" void contract_write_scalar_u32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}

extern "C" void contract_write_scalar_i32(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}

extern "C" void contract_write_scalar_bool(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}

extern "C" void contract_write_scalar_u64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(4,output,append);}

extern "C" void contract_write_scalar_i64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(5,output,append);}

extern "C" void contract_write_scalar_f64(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(6,output,append);}

extern "C" void contract_write_scalar_u8(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(7,output,append);}

extern "C" void contract_write_scalar_i8(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(8,output,append);}

extern "C" void contract_write_scalar_u16(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(9,output,append);}

extern "C" void contract_write_scalar_i16(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(10,output,append);}

extern "C" void contract_write_pair(address output,ContractPairFields fields,ContractPairWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second){host.first(fields.first,output);host.second(fields.second,output);}

extern "C" ReadStatus contract_read_pair(address context,ContractPairFields fields,ContractPairReader host) on cpu requires nonzero(host.first) requires nonzero(host.second){ReadStatus s0=host.first(context,fields.first);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.second(context,fields.second);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}

extern "C" ReadStatus contract_read_texture_dimension(Bytes input,inout u64 position,address context,address output,ContractTextureDimensionVariants host) on cpu requires nonzero(host.d2) requires nonzero(host.d3){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==2u64){return host.d2(context,output);}if(tag.value==3u64){return host.d3(context,output);}return ReadStatus::ContractTextureDimension;}

extern "C" ReadStatus contract_read_texture_format(Bytes input,inout u64 position,address context,address output,ContractTextureFormatVariants host) on cpu requires nonzero(host.rgba32float) requires nonzero(host.rgba8unorm) requires nonzero(host.rgba8srgb) requires nonzero(host.r8unorm) requires nonzero(host.rg16unorm) requires nonzero(host.rgb10a2unorm) requires nonzero(host.r11g11b10float) requires nonzero(host.r16float){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.rgba32float(context,output);}if(tag.value==1u64){return host.rgba8unorm(context,output);}if(tag.value==2u64){return host.rgba8srgb(context,output);}if(tag.value==3u64){return host.r8unorm(context,output);}if(tag.value==4u64){return host.rg16unorm(context,output);}if(tag.value==5u64){return host.rgb10a2unorm(context,output);}if(tag.value==6u64){return host.r11g11b10float(context,output);}if(tag.value==7u64){return host.r16float(context,output);}return ReadStatus::ContractTextureFormat;}

extern "C" ReadStatus contract_read_element_fact(Bytes input,inout u64 position,address context,address output,ContractElementFactVariants host) on cpu requires nonzero(host.range) requires nonzero(host.index){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.range(context,output);}if(tag.value==1u64){return host.index(context,output);}return ReadStatus::ContractElementFact;}

extern "C" ReadStatus contract_read_access(Bytes input,inout u64 position,address context,address output,ContractAccessVariants host) on cpu requires nonzero(host.value) requires nonzero(host.read) requires nonzero(host.write) requires nonzero(host.inout) requires nonzero(host.atomic){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.value(context,output);}if(tag.value==1u64){return host.read(context,output);}if(tag.value==2u64){return host.write(context,output);}if(tag.value==3u64){return host.inout(context,output);}if(tag.value==4u64){return host.atomic(context,output);}return ReadStatus::Access;}

extern "C" ReadStatus contract_read_target(Bytes input,inout u64 position,address context,address output,ContractTargetVariants host) on cpu requires nonzero(host.compute) requires nonzero(host.cpu) requires nonzero(host.gpu){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.compute(context,output);}if(tag.value==1u64){return host.cpu(context,output);}if(tag.value==2u64){return host.gpu(context,output);}return ReadStatus::Target;}

extern "C" ReadStatus contract_read_call_parameter_kind(Bytes input,inout u64 position,address context,address output,ContractCallParameterKindVariants host) on cpu requires nonzero(host.value) requires nonzero(host.reference){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.value(context,output);}if(tag.value==1u64){return host.reference(context,output);}return ReadStatus::ContractCallParameterKind;}

extern "C" ReadStatus contract_read_contract_kind(Bytes input,inout u64 position,address context,address output,ContractContractKindVariants host) on cpu requires nonzero(host.requires) requires nonzero(host.ensures){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.requires(context,output);}if(tag.value==1u64){return host.ensures(context,output);}return ReadStatus::ContractContractKind;}

extern "C" ReadStatus contract_read_scalar(Bytes input,inout u64 position,address context,address output,ContractScalarVariants host) on cpu requires nonzero(host.f32) requires nonzero(host.u32) requires nonzero(host.i32) requires nonzero(host.bool) requires nonzero(host.u64) requires nonzero(host.i64) requires nonzero(host.f64) requires nonzero(host.u8) requires nonzero(host.i8) requires nonzero(host.u16) requires nonzero(host.i16){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.f32(context,output);}if(tag.value==1u64){return host.u32(context,output);}if(tag.value==2u64){return host.i32(context,output);}if(tag.value==3u64){return host.bool(context,output);}if(tag.value==4u64){return host.u64(context,output);}if(tag.value==5u64){return host.i64(context,output);}if(tag.value==6u64){return host.f64(context,output);}if(tag.value==7u64){return host.u8(context,output);}if(tag.value==8u64){return host.i8(context,output);}if(tag.value==9u64){return host.u16(context,output);}if(tag.value==10u64){return host.i16(context,output);}return ReadStatus::Scalar;}

extern "C" void contract_write_ast_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){write_type(source,output,host);}

extern "C" void contract_write_ast_predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){predicate(source,output,host);}

extern "C" void contract_write_ast_condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){condition(source,output,host);}

extern "C" ReadStatus contract_read_ast_condition(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span){return read_condition(input,position,output,host);}

extern "C" void contract_write_subgroup_capabilities(SubgroupFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 contract_write_bool(value.basic,output,append);
 contract_write_bool(value.vote,output,append);
 contract_write_bool(value.arithmetic,output,append);
 contract_write_bool(value.ballot,output,append);
 contract_write_bool(value.shuffle,output,append);
 contract_write_bool(value.shuffle_relative,output,append);
 contract_write_bool(value.extended_types,output,append);
}

extern "C" ReadStatus contract_read_subgroup_capabilities(Bytes input,inout u64 position,inout SubgroupFields output) on cpu {
 ReadStatus status=contract_read_bool(input,position,output.basic);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.vote);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.arithmetic);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.ballot);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.shuffle);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.shuffle_relative);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.extended_types);if(status!=ReadStatus::Success){return status;}
 return ReadStatus::Success;
}

extern "C" void contract_write_device_requirements(DeviceFields value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
 contract_write_bool(value.ray_query,output,append);
 contract_write_bool(value.float64,output,append);
 contract_write_bool(value.integer8,output,append);
 contract_write_bool(value.integer16,output,append);
 contract_write_bool(value.atomic64,output,append);
 contract_write_bool(value.shared_atomic64,output,append);
 contract_write_bool(value.float32_preserve,output,append);
 contract_write_bool(value.float64_preserve,output,append);
 contract_write_bool(value.sorting,output,append);
 contract_write_bool(value.maximal_reconvergence,output,append);
 contract_write_bool(value.texture_indexing,output,append);
 contract_write_bool(value.position_fetch,output,append);
 contract_write_subgroup_capabilities(value.subgroup,output,append);
}

extern "C" ReadStatus contract_read_device_requirements(Bytes input,inout u64 position,inout DeviceFields output) on cpu {
 ReadStatus status=contract_read_bool(input,position,output.ray_query);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.float64);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.integer8);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.integer16);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.atomic64);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.shared_atomic64);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.float32_preserve);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.float64_preserve);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.sorting);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.maximal_reconvergence);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.texture_indexing);if(status!=ReadStatus::Success){return status;}
 status=contract_read_bool(input,position,output.position_fetch);if(status!=ReadStatus::Success){return status;}
 return contract_read_subgroup_capabilities(input,position,output.subgroup);
}

extern "C" void resource_write_program_layout(address output,ResourceProgramLayoutFields fields,ResourceProgramLayoutWriter host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements) {
 host.write_regions(fields.write_regions,output);
 host.name(fields.name,output);
 host.target(fields.target,output);
 host.buffers(fields.buffers,output);
 host.accelerations(fields.accelerations,output);
 host.textures(fields.textures,output);
 host.uniforms(fields.uniforms,output);
 host.uniform_bytes(fields.uniform_bytes,output);
 host.prefix_requirements(fields.prefix_requirements,output);
 host.prefix_guarantees(fields.prefix_guarantees,output);
 host.element_requirements(fields.element_requirements,output);
 host.element_guarantees(fields.element_guarantees,output);
 host.length_requirements(fields.length_requirements,output);
 host.requirements(fields.requirements,output);
}

extern "C" ReadStatus resource_read_program_layout(address context,ResourceProgramLayoutFields fields,ResourceProgramLayoutReader host) on cpu requires nonzero(host.write_regions) requires nonzero(host.name) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.accelerations) requires nonzero(host.textures) requires nonzero(host.uniforms) requires nonzero(host.uniform_bytes) requires nonzero(host.prefix_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_requirements) requires nonzero(host.element_guarantees) requires nonzero(host.length_requirements) requires nonzero(host.requirements) {
 ReadStatus status_0=host.write_regions(context,fields.write_regions);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.name(context,fields.name);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.target(context,fields.target);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.buffers(context,fields.buffers);if(status_3!=ReadStatus::Success){return status_3;}
 ReadStatus status_4=host.accelerations(context,fields.accelerations);if(status_4!=ReadStatus::Success){return status_4;}
 ReadStatus status_5=host.textures(context,fields.textures);if(status_5!=ReadStatus::Success){return status_5;}
 ReadStatus status_6=host.uniforms(context,fields.uniforms);if(status_6!=ReadStatus::Success){return status_6;}
 ReadStatus status_7=host.uniform_bytes(context,fields.uniform_bytes);if(status_7!=ReadStatus::Success){return status_7;}
 ReadStatus status_8=host.prefix_requirements(context,fields.prefix_requirements);if(status_8!=ReadStatus::Success){return status_8;}
 ReadStatus status_9=host.prefix_guarantees(context,fields.prefix_guarantees);if(status_9!=ReadStatus::Success){return status_9;}
 ReadStatus status_10=host.element_requirements(context,fields.element_requirements);if(status_10!=ReadStatus::Success){return status_10;}
 ReadStatus status_11=host.element_guarantees(context,fields.element_guarantees);if(status_11!=ReadStatus::Success){return status_11;}
 ReadStatus status_12=host.length_requirements(context,fields.length_requirements);if(status_12!=ReadStatus::Success){return status_12;}
 ReadStatus status_13=host.requirements(context,fields.requirements);if(status_13!=ReadStatus::Success){return status_13;}
 return ReadStatus::Success;
}

extern "C" void resource_write_reduction(address output,ResourceReductionFields fields,ResourceReductionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word) {
 host.source(fields.source,output);
 host.source_word(fields.source_word,output);
 host.target(fields.target,output);
 host.target_word(fields.target_word,output);
}

extern "C" ReadStatus resource_read_reduction(address context,ResourceReductionFields fields,ResourceReductionReader host) on cpu requires nonzero(host.source) requires nonzero(host.source_word) requires nonzero(host.target) requires nonzero(host.target_word) {
 ReadStatus status_0=host.source(context,fields.source);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.source_word(context,fields.source_word);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.target(context,fields.target);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.target_word(context,fields.target_word);if(status_3!=ReadStatus::Success){return status_3;}
 return ReadStatus::Success;
}

extern "C" void resource_write_reduction_spec(address output,ResourceReductionSpecFields fields,ResourceReductionSpecWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator) {
 host.fields(fields.fields,output);
 host.ty(fields.ty,output);
 host.operator(fields.operator,output);
}

extern "C" ReadStatus resource_read_reduction_spec(address context,ResourceReductionSpecFields fields,ResourceReductionSpecReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) requires nonzero(host.operator) {
 ReadStatus status_0=host.fields(context,fields.fields);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.ty(context,fields.ty);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.operator(context,fields.operator);if(status_2!=ReadStatus::Success){return status_2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_segmented_reduction(address output,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count) {
 host.values(fields.values,output);
 host.head_word(fields.head_word,output);
 host.count(fields.count,output);
}

extern "C" ReadStatus resource_read_segmented_reduction(address context,ResourceSegmentedReductionFields fields,ResourceSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count) {
 ReadStatus status_0=host.values(context,fields.values);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.head_word(context,fields.head_word);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.count(context,fields.count);if(status_2!=ReadStatus::Success){return status_2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_scan(address output,ResourceScanFields fields,ResourceScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word) {
 host.values(fields.values,output);
 host.ty(fields.ty,output);
 host.operator(fields.operator,output);
 host.head_word(fields.head_word,output);
}

extern "C" ReadStatus resource_read_scan(address context,ResourceScanFields fields,ResourceScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.ty) requires nonzero(host.operator) requires nonzero(host.head_word) {
 ReadStatus status_0=host.values(context,fields.values);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.ty(context,fields.ty);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.operator(context,fields.operator);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.head_word(context,fields.head_word);if(status_3!=ReadStatus::Success){return status_3;}
 return ReadStatus::Success;
}

extern "C" void resource_write_compaction(address output,ResourceCompactionFields fields,ResourceCompactionWriter host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word) {
 host.source(fields.source,output);
 host.flags(fields.flags,output);
 host.flag_word(fields.flag_word,output);
 host.target(fields.target,output);
 host.count(fields.count,output);
 host.count_word(fields.count_word,output);
}

extern "C" ReadStatus resource_read_compaction(address context,ResourceCompactionFields fields,ResourceCompactionReader host) on cpu requires nonzero(host.source) requires nonzero(host.flags) requires nonzero(host.flag_word) requires nonzero(host.target) requires nonzero(host.count) requires nonzero(host.count_word) {
 ReadStatus status_0=host.source(context,fields.source);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.flags(context,fields.flags);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.flag_word(context,fields.flag_word);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.target(context,fields.target);if(status_3!=ReadStatus::Success){return status_3;}
 ReadStatus status_4=host.count(context,fields.count);if(status_4!=ReadStatus::Success){return status_4;}
 ReadStatus status_5=host.count_word(context,fields.count_word);if(status_5!=ReadStatus::Success){return status_5;}
 return ReadStatus::Success;
}

extern "C" void resource_write_sorting(address output,ResourceSortingFields fields,ResourceSortingWriter host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target) {
 host.source(fields.source,output);
 host.keys(fields.keys,output);
 host.target(fields.target,output);
}

extern "C" ReadStatus resource_read_sorting(address context,ResourceSortingFields fields,ResourceSortingReader host) on cpu requires nonzero(host.source) requires nonzero(host.keys) requires nonzero(host.target) {
 ReadStatus status_0=host.source(context,fields.source);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.keys(context,fields.keys);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.target(context,fields.target);if(status_2!=ReadStatus::Success){return status_2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_reduction_layout(address output,ResourceReductionLayoutFields fields,ResourceReductionLayoutWriter host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) {
 host.fields(fields.fields,output);
 host.ty(fields.ty,output);
}

extern "C" ReadStatus resource_read_reduction_layout(address context,ResourceReductionLayoutFields fields,ResourceReductionLayoutReader host) on cpu requires nonzero(host.fields) requires nonzero(host.ty) {
 ReadStatus status_0=host.fields(context,fields.fields);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.ty(context,fields.ty);if(status_1!=ReadStatus::Success){return status_1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_custom_scan(address output,ResourceCustomScanFields fields,ResourceCustomScanWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) {
 host.values(fields.values,output);
 host.head_word(fields.head_word,output);
}

extern "C" ReadStatus resource_read_custom_scan(address context,ResourceCustomScanFields fields,ResourceCustomScanReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) {
 ReadStatus status_0=host.values(context,fields.values);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.head_word(context,fields.head_word);if(status_1!=ReadStatus::Success){return status_1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_custom_segmented_reduction(address output,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionWriter host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count) {
 host.values(fields.values,output);
 host.head_word(fields.head_word,output);
 host.count(fields.count,output);
}

extern "C" ReadStatus resource_read_custom_segmented_reduction(address context,ResourceCustomSegmentedReductionFields fields,ResourceCustomSegmentedReductionReader host) on cpu requires nonzero(host.values) requires nonzero(host.head_word) requires nonzero(host.count) {
 ReadStatus status_0=host.values(context,fields.values);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.head_word(context,fields.head_word);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.count(context,fields.count);if(status_2!=ReadStatus::Success){return status_2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_selection(address output,ResourceSelectionFields fields,ResourceSelectionWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.word) {
 host.binding(fields.binding,output);
 host.word(fields.word,output);
}

extern "C" ReadStatus resource_read_selection(address context,ResourceSelectionFields fields,ResourceSelectionReader host) on cpu requires nonzero(host.binding) requires nonzero(host.word) {
 ReadStatus status_0=host.binding(context,fields.binding);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.word(context,fields.word);if(status_1!=ReadStatus::Success){return status_1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_execution_stage(address output,ResourceExecutionStageFields fields,ResourceExecutionStageWriter host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes) {
 host.domain(fields.domain,output);
 host.prefix(fields.prefix,output);
 host.selection(fields.selection,output);
 host.operation(fields.operation,output);
 host.reads(fields.reads,output);
 host.writes(fields.writes,output);
 host.write_regions(fields.write_regions,output);
 host.prefix_guarantees(fields.prefix_guarantees,output);
 host.element_guarantees(fields.element_guarantees,output);
 host.inputs(fields.inputs,output);
 host.tile_scope(fields.tile_scope,output);
 host.width(fields.width,output);
 host.shared_bytes(fields.shared_bytes,output);
}

extern "C" ReadStatus resource_read_execution_stage(address context,ResourceExecutionStageFields fields,ResourceExecutionStageReader host) on cpu requires nonzero(host.domain) requires nonzero(host.prefix) requires nonzero(host.selection) requires nonzero(host.operation) requires nonzero(host.reads) requires nonzero(host.writes) requires nonzero(host.write_regions) requires nonzero(host.prefix_guarantees) requires nonzero(host.element_guarantees) requires nonzero(host.inputs) requires nonzero(host.tile_scope) requires nonzero(host.width) requires nonzero(host.shared_bytes) {
 ReadStatus status_0=host.domain(context,fields.domain);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.prefix(context,fields.prefix);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.selection(context,fields.selection);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.operation(context,fields.operation);if(status_3!=ReadStatus::Success){return status_3;}
 ReadStatus status_4=host.reads(context,fields.reads);if(status_4!=ReadStatus::Success){return status_4;}
 ReadStatus status_5=host.writes(context,fields.writes);if(status_5!=ReadStatus::Success){return status_5;}
 ReadStatus status_6=host.write_regions(context,fields.write_regions);if(status_6!=ReadStatus::Success){return status_6;}
 ReadStatus status_7=host.prefix_guarantees(context,fields.prefix_guarantees);if(status_7!=ReadStatus::Success){return status_7;}
 ReadStatus status_8=host.element_guarantees(context,fields.element_guarantees);if(status_8!=ReadStatus::Success){return status_8;}
 ReadStatus status_9=host.inputs(context,fields.inputs);if(status_9!=ReadStatus::Success){return status_9;}
 ReadStatus status_10=host.tile_scope(context,fields.tile_scope);if(status_10!=ReadStatus::Success){return status_10;}
 ReadStatus status_11=host.width(context,fields.width);if(status_11!=ReadStatus::Success){return status_11;}
 ReadStatus status_12=host.shared_bytes(context,fields.shared_bytes);if(status_12!=ReadStatus::Success){return status_12;}
 return ReadStatus::Success;
}

extern "C" void resource_write_execution_plan(address output,ResourceExecutionPlanFields fields,ResourceExecutionPlanWriter host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame) {
 host.interface(fields.interface,output);
 host.stages(fields.stages,output);
 host.indirect_buffer(fields.indirect_buffer,output);
 host.auxiliary_buffers(fields.auxiliary_buffers,output);
 host.native_frame(fields.native_frame,output);
}

extern "C" ReadStatus resource_read_execution_plan(address context,ResourceExecutionPlanFields fields,ResourceExecutionPlanReader host) on cpu requires nonzero(host.interface) requires nonzero(host.stages) requires nonzero(host.indirect_buffer) requires nonzero(host.auxiliary_buffers) requires nonzero(host.native_frame) {
 ReadStatus status_0=host.interface(context,fields.interface);if(status_0!=ReadStatus::Success){return status_0;}
 ReadStatus status_1=host.stages(context,fields.stages);if(status_1!=ReadStatus::Success){return status_1;}
 ReadStatus status_2=host.indirect_buffer(context,fields.indirect_buffer);if(status_2!=ReadStatus::Success){return status_2;}
 ReadStatus status_3=host.auxiliary_buffers(context,fields.auxiliary_buffers);if(status_3!=ReadStatus::Success){return status_3;}
 ReadStatus status_4=host.native_frame(context,fields.native_frame);if(status_4!=ReadStatus::Success){return status_4;}
 return ReadStatus::Success;
}

extern "C" void resource_write_sort_order_ascending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(0,output,append);}

extern "C" void resource_write_sort_order_descending(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(1,output,append);}

extern "C" ReadStatus resource_read_sort_order(Bytes input,inout u64 position,address output,ResourceSortOrderReader host) on cpu requires nonzero(host.ascending) requires nonzero(host.descending) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){host.ascending(output);return ReadStatus::Success;}
 if(tag.value==1u64){host.descending(output);return ReadStatus::Success;}
 return ReadStatus::ResourceSortOrder;
}

extern "C" void resource_write_collective_operator_sum(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(0,output,append);}

extern "C" void resource_write_collective_operator_min(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(1,output,append);}

extern "C" void resource_write_collective_operator_max(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(2,output,append);}

extern "C" void resource_write_collective_operator_bit_and(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(3,output,append);}

extern "C" void resource_write_collective_operator_bit_or(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(4,output,append);}

extern "C" void resource_write_collective_operator_bit_xor(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(5,output,append);}

extern "C" ReadStatus resource_read_collective_operator(Bytes input,inout u64 position,address output,ResourceCollectiveOperatorReader host) on cpu requires nonzero(host.sum) requires nonzero(host.min) requires nonzero(host.max) requires nonzero(host.bit_and) requires nonzero(host.bit_or) requires nonzero(host.bit_xor) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){host.sum(output);return ReadStatus::Success;}
 if(tag.value==1u64){host.min(output);return ReadStatus::Success;}
 if(tag.value==2u64){host.max(output);return ReadStatus::Success;}
 if(tag.value==3u64){host.bit_and(output);return ReadStatus::Success;}
 if(tag.value==4u64){host.bit_or(output);return ReadStatus::Success;}
 if(tag.value==5u64){host.bit_xor(output);return ReadStatus::Success;}
 return ReadStatus::ResourceCollectiveOperator;
}

extern "C" void resource_write_operation_map(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(0,output,append);}

extern "C" void resource_write_operation_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(source,output);}

extern "C" void resource_write_operation_custom_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(2,output,append);payload(source,output);}

extern "C" void resource_write_operation_custom_scan(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(3,output,append);payload(source,output);}

extern "C" void resource_write_operation_custom_segmented_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(4,output,append);payload(source,output);}

extern "C" void resource_write_operation_segmented_reduce(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(5,output,append);payload(source,output);}

extern "C" void resource_write_operation_scan_exclusive(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(6,output,append);payload(source,output);}

extern "C" void resource_write_operation_compact(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(7,output,append);payload(source,output);}

extern "C" void resource_write_operation_stable_sort(address source,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(8,output,append);payload(source,output);}

extern "C" ReadStatus resource_read_operation(Bytes input,inout u64 position,address context,address output,ResourceOperationReader host) on cpu requires nonzero(host.map) requires nonzero(host.reduce) requires nonzero(host.custom_reduce) requires nonzero(host.custom_scan) requires nonzero(host.custom_segmented_reduce) requires nonzero(host.segmented_reduce) requires nonzero(host.scan_exclusive) requires nonzero(host.compact) requires nonzero(host.stable_sort) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.map(context,output);}
 if(tag.value==1u64){return host.reduce(context,output);}
 if(tag.value==2u64){return host.custom_reduce(context,output);}
 if(tag.value==3u64){return host.custom_scan(context,output);}
 if(tag.value==4u64){return host.custom_segmented_reduce(context,output);}
 if(tag.value==5u64){return host.segmented_reduce(context,output);}
 if(tag.value==6u64){return host.scan_exclusive(context,output);}
 if(tag.value==7u64){return host.compact(context,output);}
 if(tag.value==8u64){return host.stable_sort(context,output);}
 return ReadStatus::ResourceOperation;
}

extern "C" void contract_write_triple(address output,ContractTripleFields fields,ContractTripleWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third) {
 host.first(fields.first,output);host.second(fields.second,output);host.third(fields.third,output);
}

extern "C" ReadStatus contract_read_triple(address context,ContractTripleFields fields,ContractTripleReader host) on cpu requires nonzero(host.first) requires nonzero(host.second) requires nonzero(host.third) {
 ReadStatus first=host.first(context,fields.first);if(first!=ReadStatus::Success){return first;}
 ReadStatus second=host.second(context,fields.second);if(second!=ReadStatus::Success){return second;}
 return host.third(context,fields.third);
}

extern "C" void resource_write_external_name(address output,ResourceExternalNameFields fields,ResourceExternalNameWriter host) on cpu requires nonzero(host.library) requires nonzero(host.symbol) {
 host.library(fields.library,output);
 host.symbol(fields.symbol,output);
}

extern "C" ReadStatus resource_read_external_name(address context,ResourceExternalNameFields fields,ResourceExternalNameReader host) on cpu requires nonzero(host.library) requires nonzero(host.symbol) {
 ReadStatus s0=host.library(context,fields.library);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.symbol(context,fields.symbol);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_module_code(address output,ResourceModuleCodeFields fields,ResourceModuleCodeWriter host) on cpu requires nonzero(host.sections) requires nonzero(host.symbols) {
 host.sections(fields.sections,output);
 host.symbols(fields.symbols,output);
}

extern "C" ReadStatus resource_read_module_code(address context,ResourceModuleCodeFields fields,ResourceModuleCodeReader host) on cpu requires nonzero(host.sections) requires nonzero(host.symbols) {
 ReadStatus s0=host.sections(context,fields.sections);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.symbols(context,fields.symbols);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_module_section(address output,ResourceModuleSectionFields fields,ResourceModuleSectionWriter host) on cpu requires nonzero(host.name) requires nonzero(host.bytes) requires nonzero(host.flags) requires nonzero(host.relocations) {
 host.name(fields.name,output);
 host.bytes(fields.bytes,output);
 host.flags(fields.flags,output);
 host.relocations(fields.relocations,output);
}

extern "C" ReadStatus resource_read_module_section(address context,ResourceModuleSectionFields fields,ResourceModuleSectionReader host) on cpu requires nonzero(host.name) requires nonzero(host.bytes) requires nonzero(host.flags) requires nonzero(host.relocations) {
 ReadStatus s0=host.name(context,fields.name);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.bytes(context,fields.bytes);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.flags(context,fields.flags);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.relocations(context,fields.relocations);if(s3!=ReadStatus::Success){return s3;}
 return ReadStatus::Success;
}

extern "C" void resource_write_module_relocation(address output,ResourceModuleRelocationFields fields,ResourceModuleRelocationWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.kind) requires nonzero(host.target) {
 host.offset(fields.offset,output);
 host.kind(fields.kind,output);
 host.target(fields.target,output);
}

extern "C" ReadStatus resource_read_module_relocation(address context,ResourceModuleRelocationFields fields,ResourceModuleRelocationReader host) on cpu requires nonzero(host.offset) requires nonzero(host.kind) requires nonzero(host.target) {
 ReadStatus s0=host.offset(context,fields.offset);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.kind(context,fields.kind);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.target(context,fields.target);if(s2!=ReadStatus::Success){return s2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_native_relocation(address output,ResourceNativeRelocationFields fields,ResourceNativeRelocationWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.target) {
 host.offset(fields.offset,output);
 host.target(fields.target,output);
}

extern "C" ReadStatus resource_read_native_relocation(address context,ResourceNativeRelocationFields fields,ResourceNativeRelocationReader host) on cpu requires nonzero(host.offset) requires nonzero(host.target) {
 ReadStatus s0=host.offset(context,fields.offset);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.target(context,fields.target);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_native_code(address output,ResourceNativeCodeFields fields,ResourceNativeCodeWriter host) on cpu requires nonzero(host.bytes) requires nonzero(host.relocations) requires nonzero(host.stack_bytes) requires nonzero(host.calls) requires nonzero(host.function_end) requires nonzero(host.unwind) {
 host.bytes(fields.bytes,output);
 host.relocations(fields.relocations,output);
 host.stack_bytes(fields.stack_bytes,output);
 host.calls(fields.calls,output);
 host.function_end(fields.function_end,output);
 host.unwind(fields.unwind,output);
}

extern "C" ReadStatus resource_read_native_code(address context,ResourceNativeCodeFields fields,ResourceNativeCodeReader host) on cpu requires nonzero(host.bytes) requires nonzero(host.relocations) requires nonzero(host.stack_bytes) requires nonzero(host.calls) requires nonzero(host.function_end) requires nonzero(host.unwind) {
 ReadStatus s0=host.bytes(context,fields.bytes);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.relocations(context,fields.relocations);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.stack_bytes(context,fields.stack_bytes);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.calls(context,fields.calls);if(s3!=ReadStatus::Success){return s3;}
 ReadStatus s4=host.function_end(context,fields.function_end);if(s4!=ReadStatus::Success){return s4;}
 ReadStatus s5=host.unwind(context,fields.unwind);if(s5!=ReadStatus::Success){return s5;}
 return ReadStatus::Success;
}

extern "C" void resource_write_sort_code(address output,ResourceSortCodeFields fields,ResourceSortCodeWriter host) on cpu requires nonzero(host.source) requires nonzero(host.target) requires nonzero(host.initialize) requires nonzero(host.histogram) requires nonzero(host.scatter) requires nonzero(host.gather) {
 host.source(fields.source,output);
 host.target(fields.target,output);
 host.initialize(fields.initialize,output);
 host.histogram(fields.histogram,output);
 host.scatter(fields.scatter,output);
 host.gather(fields.gather,output);
}

extern "C" ReadStatus resource_read_sort_code(address context,ResourceSortCodeFields fields,ResourceSortCodeReader host) on cpu requires nonzero(host.source) requires nonzero(host.target) requires nonzero(host.initialize) requires nonzero(host.histogram) requires nonzero(host.scatter) requires nonzero(host.gather) {
 ReadStatus s0=host.source(context,fields.source);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.target(context,fields.target);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.initialize(context,fields.initialize);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.histogram(context,fields.histogram);if(s3!=ReadStatus::Success){return s3;}
 ReadStatus s4=host.scatter(context,fields.scatter);if(s4!=ReadStatus::Success){return s4;}
 ReadStatus s5=host.gather(context,fields.gather);if(s5!=ReadStatus::Success){return s5;}
 return ReadStatus::Success;
}

extern "C" void resource_write_shader_code(address output,ResourceShaderCodeFields fields,ResourceShaderCodeWriter host) on cpu requires nonzero(host.words) requires nonzero(host.ranks) {
 host.words(fields.words,output);
 host.ranks(fields.ranks,output);
}

extern "C" ReadStatus resource_read_shader_code(address context,ResourceShaderCodeFields fields,ResourceShaderCodeReader host) on cpu requires nonzero(host.words) requires nonzero(host.ranks) {
 ReadStatus s0=host.words(context,fields.words);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.ranks(context,fields.ranks);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_gpu_stage_code(address output,ResourceGpuStageCodeFields fields,ResourceGpuStageCodeWriter host) on cpu requires nonzero(host.main) requires nonzero(host.tail) requires nonzero(host.scan_add) requires nonzero(host.finish) requires nonzero(host.dispatch) requires nonzero(host.selector) requires nonzero(host.additional) {
 host.main(fields.main,output);
 host.tail(fields.tail,output);
 host.scan_add(fields.scan_add,output);
 host.finish(fields.finish,output);
 host.dispatch(fields.dispatch,output);
 host.selector(fields.selector,output);
 host.additional(fields.additional,output);
}

extern "C" ReadStatus resource_read_gpu_stage_code(address context,ResourceGpuStageCodeFields fields,ResourceGpuStageCodeReader host) on cpu requires nonzero(host.main) requires nonzero(host.tail) requires nonzero(host.scan_add) requires nonzero(host.finish) requires nonzero(host.dispatch) requires nonzero(host.selector) requires nonzero(host.additional) {
 ReadStatus s0=host.main(context,fields.main);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.tail(context,fields.tail);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.scan_add(context,fields.scan_add);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.finish(context,fields.finish);if(s3!=ReadStatus::Success){return s3;}
 ReadStatus s4=host.dispatch(context,fields.dispatch);if(s4!=ReadStatus::Success){return s4;}
 ReadStatus s5=host.selector(context,fields.selector);if(s5!=ReadStatus::Success){return s5;}
 ReadStatus s6=host.additional(context,fields.additional);if(s6!=ReadStatus::Success){return s6;}
 return ReadStatus::Success;
}

extern "C" void resource_write_module(address output,ResourceModuleFields fields,ResourceModuleWriter host) on cpu requires nonzero(host.types) requires nonzero(host.images) {
 host.types(fields.types,output);
 host.images(fields.images,output);
}

extern "C" ReadStatus resource_read_module(address context,ResourceModuleFields fields,ResourceModuleReader host) on cpu requires nonzero(host.types) requires nonzero(host.images) {
 ReadStatus s0=host.types(context,fields.types);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.images(context,fields.images);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" ReadStatus resource_read_image(Bytes input,inout u64 position,address context,address output,ResourceImageVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.cpu(context,output);}
 if(tag.value==1u64){return host.gpu(context,output);}
 return ReadStatus::ResourceImage;
}

extern "C" void resource_write_image_cpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(0,output,append);payload(value,output);}

extern "C" void resource_write_image_gpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" ReadStatus resource_read_linked_image(Bytes input,inout u64 position,address context,address output,ResourceImageVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==2u64){return host.cpu(context,output);}
 if(tag.value==1u64){return host.gpu(context,output);}
 return ReadStatus::ResourceLinkedImage;
}

extern "C" void resource_write_linked_image_cpu(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(2,output,append);payload(value,output);}

extern "C" ReadStatus resource_read_module_target(Bytes input,inout u64 position,address context,address output,ResourceModuleTargetVariants host) on cpu requires nonzero(host.section) requires nonzero(host.import) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.section(context,output);}
 if(tag.value==1u64){return host.import(context,output);}
 return ReadStatus::ResourceModuleTarget;
}

extern "C" void resource_write_module_target_section_fields(address output,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsWriter host) on cpu requires nonzero(host.section) requires nonzero(host.value) {
 host.section(fields.section,output);
 host.value(fields.value,output);
}

extern "C" ReadStatus resource_read_module_target_section_fields(address context,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsReader host) on cpu requires nonzero(host.section) requires nonzero(host.value) {
 ReadStatus s0=host.section(context,fields.section);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.value(context,fields.value);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}

extern "C" void resource_write_module_target_section(address output,ResourceModuleTargetSectionFieldsFields fields,ResourceModuleTargetSectionFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.section) requires nonzero(host.value) {word(0,output,append);resource_write_module_target_section_fields(output,fields,host);}

extern "C" void resource_write_module_target_import(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" ReadStatus resource_read_native_symbol(Bytes input,inout u64 position,address context,address output,ResourceNativeSymbolVariants host) on cpu requires nonzero(host.print) requires nonzero(host.external) requires nonzero(host.function) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.print(context,output);}
 if(tag.value==1u64){return host.external(context,output);}
 if(tag.value==2u64){return host.function(context,output);}
 return ReadStatus::ResourceNativeSymbol;
}

extern "C" void resource_write_native_symbol_print(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(0,output,append);payload(value,output);}

extern "C" void resource_write_native_symbol_external(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" void resource_write_native_symbol_function(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(2,output,append);payload(value,output);}

extern "C" ReadStatus resource_read_range_result(Bytes input,inout u64 position,address context,address output,ResourceRangeResultVariants host) on cpu requires nonzero(host.none) requires nonzero(host.values) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.none(context,output);}
 if(tag.value==1u64){return host.values(context,output);}
 return ReadStatus::ResourceRangeResult;
}

extern "C" void resource_write_range_result_none(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {word(0,output,append);}

extern "C" void resource_write_range_result_values(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" ReadStatus resource_read_continuation_code(Bytes input,inout u64 position,address context,address output,ResourceContinuationCodeVariants host) on cpu requires nonzero(host.reduction_tail) requires nonzero(host.scan_add) requires nonzero(host.compact_scatter) requires nonzero(host.segmented_finish) requires nonzero(host.scan) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.reduction_tail(context,output);}
 if(tag.value==1u64){return host.scan_add(context,output);}
 if(tag.value==2u64){return host.compact_scatter(context,output);}
 if(tag.value==3u64){return host.segmented_finish(context,output);}
 if(tag.value==4u64){return host.scan(context,output);}
 return ReadStatus::ResourceContinuationCode;
}

extern "C" void resource_write_continuation_code_reduction_tail(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(0,output,append);payload(value,output);}

extern "C" void resource_write_continuation_code_scan_add(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" void resource_write_continuation_code_compact_scatter(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(2,output,append);payload(value,output);}

extern "C" void resource_write_continuation_code_segmented_finish(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(3,output,append);payload(value,output);}

extern "C" void resource_write_continuation_code_scan_fields(address output,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsWriter host) on cpu requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target) {
 host.prefix(fields.prefix,output);
 host.apply(fields.apply,output);
 host.source(fields.source,output);
 host.target(fields.target,output);
}

extern "C" ReadStatus resource_read_continuation_code_scan_fields(address context,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsReader host) on cpu requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target) {
 ReadStatus s0=host.prefix(context,fields.prefix);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.apply(context,fields.apply);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.source(context,fields.source);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.target(context,fields.target);if(s3!=ReadStatus::Success){return s3;}
 return ReadStatus::Success;
}

extern "C" void resource_write_continuation_code_scan(address output,ResourceContinuationCodeScanFieldsFields fields,ResourceContinuationCodeScanFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.prefix) requires nonzero(host.apply) requires nonzero(host.source) requires nonzero(host.target) {word(4,output,append);resource_write_continuation_code_scan_fields(output,fields,host);}

extern "C" ReadStatus resource_read_cpu_stage_code(Bytes input,inout u64 position,address context,address output,ResourceCpuStageCodeVariants host) on cpu requires nonzero(host.element) requires nonzero(host.sort) {
 ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
 if(tag.value==0u64){return host.element(context,output);}
 if(tag.value==1u64){return host.sort(context,output);}
 return ReadStatus::ResourceCpuStageCode;
}

extern "C" void resource_write_cpu_stage_code_element_fields(address output,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsWriter host) on cpu requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result) {
 host.main(fields.main,output);
 host.continuation(fields.continuation,output);
 host.result(fields.result,output);
}

extern "C" ReadStatus resource_read_cpu_stage_code_element_fields(address context,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsReader host) on cpu requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result) {
 ReadStatus s0=host.main(context,fields.main);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.continuation(context,fields.continuation);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.result(context,fields.result);if(s2!=ReadStatus::Success){return s2;}
 return ReadStatus::Success;
}

extern "C" void resource_write_cpu_stage_code_element(address output,ResourceCpuStageCodeElementFieldsFields fields,ResourceCpuStageCodeElementFieldsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(append) requires nonzero(host.main) requires nonzero(host.continuation) requires nonzero(host.result) {word(0,output,append);resource_write_cpu_stage_code_element_fields(output,fields,host);}

extern "C" void resource_write_cpu_stage_code_sort(address value,address output,cfn<void,address,address> payload,cfn<void,address,address,u64> append) on cpu requires nonzero(payload) requires nonzero(append) {word(1,output,append);payload(value,output);}

extern "C" void resource_write_native_image(address output,ResourceNativeImageFields fields,ResourceNativeImageWriter host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) requires nonzero(host.functions) requires nonzero(host.resources) requires nonzero(host.module_code) {
 host.plan(fields.plan,output);
 host.stages(fields.stages,output);
 host.functions(fields.functions,output);
 host.resources(fields.resources,output);
 host.module_code(fields.module_code,output);
}

extern "C" ReadStatus resource_read_native_image(address context,ResourceNativeImageFields fields,ResourceNativeImageReader host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) requires nonzero(host.functions) requires nonzero(host.resources) requires nonzero(host.module_code) {
 ReadStatus s0=host.plan(context,fields.plan);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.stages(context,fields.stages);if(s1!=ReadStatus::Success){return s1;}
 ReadStatus s2=host.functions(context,fields.functions);if(s2!=ReadStatus::Success){return s2;}
 ReadStatus s3=host.resources(context,fields.resources);if(s3!=ReadStatus::Success){return s3;}
 ReadStatus s4=host.module_code(context,fields.module_code);if(s4!=ReadStatus::Success){return s4;}
 return ReadStatus::Success;
}

extern "C" void resource_write_gpu_image(address output,ResourceGpuImageFields fields,ResourceGpuImageWriter host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) {
 host.plan(fields.plan,output);
 host.stages(fields.stages,output);
}

extern "C" ReadStatus resource_read_gpu_image(address context,ResourceGpuImageFields fields,ResourceGpuImageReader host) on cpu requires nonzero(host.plan) requires nonzero(host.stages) {
 ReadStatus s0=host.plan(context,fields.plan);if(s0!=ReadStatus::Success){return s0;}
 ReadStatus s1=host.stages(context,fields.stages);if(s1!=ReadStatus::Success){return s1;}
 return ReadStatus::Success;
}
