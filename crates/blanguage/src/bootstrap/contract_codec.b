module bootstrap.contract_codec;

address item(Span values,u64 index) on cpu {
 return unsafe_address(address_bits(values.data)+index*values.stride);
}
void byte(u8 value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 host.append(output,address_of(value),1u64);
}
void word(u32 value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 array<u8,4> bytes;
 bytes[0]=u8(value);bytes[1]=u8(value>>8);bytes[2]=u8(value>>16);bytes[3]=u8(value>>24);
 host.append(output,address_of(bytes[0]),4u64);
}
void wide(u64 value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 array<u8,8> bytes;
 bytes[0]=u8(value);bytes[1]=u8(value>>8);bytes[2]=u8(value>>16);bytes[3]=u8(value>>24);
 bytes[4]=u8(value>>32);bytes[5]=u8(value>>40);bytes[6]=u8(value>>48);bytes[7]=u8(value>>56);
 host.append(output,address_of(bytes[0]),8u64);
}
void text(Bytes value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(value.count,output,host);host.append(output,value.data,value.count);
}
void type_option(address value,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 bool present=address_bits(value)!=0u64;byte(present?u8(1):u8(0),output,host);
 if(present){write_type(value,output,host);}
}
void arguments(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host);u64 index=0u64;
 while(index<values.count){
  Argument value=host.argument(item(values,index));
  if(value.kind==0){word(0,output,host);type_name(value.value,output,host);}
  else{word(1,output,host);shape(value.value,output,host);}
  index+=1u64;
 }
}
void predicates(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host);u64 index=0u64;
 while(index<values.count){predicate(item(values,index),output,host);index+=1u64;}
}
void type_name(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 TypeName value=host.type_name(source);text(value.name,output,host);byte(u8(value.canonical),output,host);
 arguments(value.arguments,output,host);byte(u8(value.dataset),output,host);
 byte(u8(value.view),output,host);byte(u8(value.tile),output,host);
 bool count=address_bits(value.count)!=0u64;byte(count?u8(1):u8(0),output,host);
 if(count){shape(value.count,output,host);}
 byte(u8(value.contracts_present),output,host);
 if(value.contracts_present!=0){predicates(value.contracts,output,host);}
}
void shape(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Shape value=host.shape(source);
 if(value.kind==0){word(0,output,host);word(value.literal,output,host);}
 else if(value.kind==1){word(1,output,host);text(value.name,output,host);}
 else{word(2,output,host);expression(value.expression,output,host);}
}
void expressions(Span values,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 wide(values.count,output,host);u64 index=0u64;
 while(index<values.count){expression(item(values,index),output,host);index+=1u64;}
}
void expression(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Expression value=host.expression(source);
 // Description kinds are actual syntax variants; the wire tags and payload
 // order below belong to this encoder, not the Rust field projection.
 if(value.kind==0){word(0,output,host);text(value.name,output,host);}
 else if(value.kind==1){word(1,output,host);word(value.literal,output,host);}
 else if(value.kind==2){word(2,output,host);byte(u8(value.literal),output,host);}
 else if(value.kind==3){word(3,output,host);text(value.name,output,host);}
 else if(value.kind==4){word(4,output,host);expression(value.a,output,host);text(value.name,output,host);}
 else if(value.kind==5){word(5,output,host);expression(value.a,output,host);expression(value.b,output,host);}
 else if(value.kind==6){word(6,output,host);text(value.name,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==7){word(7,output,host);expression(value.a,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==8){word(8,output,host);type_name(value.ty,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==9){word(9,output,host);text(value.name,output,host);arguments(value.types,output,host);expressions(value.arguments,output,host);}
 else if(value.kind==10){word(10,output,host);text(value.name,output,host);arguments(value.types,output,host);}
 else if(value.kind==11){
  word(11,output,host);text(value.name,output,host);byte(u8(value.types_present),output,host);
  if(value.types_present!=0){arguments(value.types,output,host);}expressions(value.arguments,output,host);
 }else if(value.kind==12){word(12,output,host);word(value.target,output,host);expression(value.a,output,host);}
 else if(value.kind==13){word(13,output,host);text(value.name,output,host);expression(value.a,output,host);}
 else if(value.kind==14){word(14,output,host);text(value.name,output,host);expression(value.a,output,host);expression(value.b,output,host);}
 else{word(15,output,host);expression(value.a,output,host);expression(value.b,output,host);expression(value.c,output,host);}
 wide(value.start,output,host);wide(value.end,output,host);
}
void constant(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Constant value=host.constant(source);write_type(value.ty,output,host);wide(value.bits.count,output,host);u64 index=0u64;
 while(index<value.bits.count){wide(unsafe_position<u64>(item(value.bits,index)),output,host);index+=1u64;}
}
void condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Condition value=host.condition(source);
 if(value.kind==0){word(0,output,host);text(value.name,output,host);}
 else if(value.kind==1){word(1,output,host);text(value.name,output,host);}
 else if(value.kind==2){word(2,output,host);byte(u8(value.literal),output,host);}
 else if(value.kind==3){word(3,output,host);condition(value.a,output,host);text(value.name,output,host);}
 else if(value.kind==4){word(4,output,host);text(value.name,output,host);condition(value.a,output,host);}
 else if(value.kind==5){word(5,output,host);text(value.name,output,host);condition(value.a,output,host);condition(value.b,output,host);}
 else if(value.kind==6){word(6,output,host);word(value.literal,output,host);}
 else{word(7,output,host);constant(value.constant,output,host);}
}
void predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Predicate value=host.predicate(source);
 if(value.kind==0){word(0,output,host);text(value.first,output,host);text(value.second,output,host);text(value.third,output,host);}
 else if(value.kind==1){word(1,output,host);text(value.first,output,host);}
 else if(value.kind==2){word(2,output,host);text(value.first,output,host);}
 else if(value.kind==3){word(4,output,host);text(value.first,output,host);}
 else if(value.kind==4){word(8,output,host);byte(u8(value.access_present),output,host);if(value.access_present!=0){word(value.access,output,host);}}
 else if(value.kind==5){word(3,output,host);text(value.first,output,host);text(value.second,output,host);}
 else if(value.kind==6){
  word(9,output,host);text(value.first,output,host);wide(value.intervals.count,output,host);u64 index=0u64;
  while(index<value.intervals.count){Interval bounds=host.interval(item(value.intervals,index));condition(bounds.lower,output,host);condition(bounds.upper,output,host);index+=1u64;}
 }else if(value.kind==7){word(6,output,host);text(value.first,output,host);text(value.second,output,host);}
 else if(value.kind==8){word(5,output,host);text(value.first,output,host);condition(value.condition,output,host);}
 else{word(7,output,host);text(value.first,output,host);condition(value.condition,output,host);}
}
extern "C" void write_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant) {
 Type value=host.ty(source);text(value.name,output,host);wide(value.arguments.count,output,host);u64 index=0u64;
 while(index<value.arguments.count){write_type(item(value.arguments,index),output,host);index+=1u64;}
 wide(value.values.count,output,host);index=0u64;
 while(index<value.values.count){word(unsafe_position<u32>(item(value.values,index)),output,host);index+=1u64;}
 word(value.size,output,host);word(value.alignment,output,host);wide(value.components.count,output,host);index=0u64;
 while(index<value.components.count){Component part=host.component(item(value.components,index));word(part.scalar,output,host);word(part.word,output,host);index+=1u64;}
 wide(value.fields.count,output,host);index=0u64;
 while(index<value.fields.count){Field field=host.field(item(value.fields,index));text(field.name,output,host);word(field.word,output,host);write_type(field.ty,output,host);index+=1u64;}
 type_option(value.view,output,host);type_option(value.array,output,host);
 byte(u8(value.enumeration),output,host);byte(u8(value.result),output,host);wide(value.variants.count,output,host);index=0u64;
 while(index<value.variants.count){Variant variant=host.variant(item(value.variants,index));text(variant.name,output,host);word(variant.discriminant,output,host);write_type(variant.ty,output,host);index+=1u64;}
 bool dataset=address_bits(value.dataset)!=0u64;byte(dataset?u8(1):u8(0),output,host);
 if(dataset){type_name(value.dataset,output,host);}
 byte(u8(value.cleanup_present),output,host);if(value.cleanup_present!=0){text(value.cleanup,output,host);}
 bool callable=address_bits(value.function)!=0u64;byte(callable?u8(1):u8(0),output,host);
 if(callable){
  Function function=host.function(value.function);type_name(function.result,output,host);wide(function.parameters.count,output,host);index=0u64;
  while(index<function.parameters.count){Parameter parameter=host.parameter(item(function.parameters,index));word(parameter.access,output,host);type_name(parameter.ty,output,host);index+=1u64;}
  predicates(function.contracts,output,host);
 }
}
