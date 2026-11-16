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

struct ReadBytes { Bytes value;ReadStatus status; };
struct ReadValue { u64 value;ReadStatus status; };
struct ReadAccess { AccessTag value;ReadStatus status; };
struct ReadTarget { TargetTag value;ReadStatus status; };
struct ReadScalar { ScalarTag value;ReadStatus status; };
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
ReadStatus read_text_field(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 ReadBytes value=counted_bytes(input,position);
 if(value.status!=ReadStatus::Success){return value.status;}
 return host.text(host.context,output,value.value);
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
 ReadStatus read_step_1=read_text_field(input,position,name,host);if(read_step_1!=ReadStatus::Success){return read_step_1;}
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
 ReadStatus read_step_4=read_text_field(input,position,fields.name,host);if(read_step_4!=ReadStatus::Success){return read_step_4;}
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
 ReadStatus read_step_18=read_text_field(input,position,fields.name,host);if(read_step_18!=ReadStatus::Success){return read_step_18;}
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
 if(kind==ExpressionTag::Number||kind==ExpressionTag::Name){return read_text_field(input,position,fields.text,host);}
 if(kind==ExpressionTag::StaticU32){return read_word_field(input,position,fields.literal);}
 if(kind==ExpressionTag::Bool){return read_bool_field(input,position,fields.literal);}
 if(kind==ExpressionTag::Member){ReadStatus read_step_30=read_expression(input,position,fields.a,host);if(read_step_30!=ReadStatus::Success){return read_step_30;}return read_text_field(input,position,fields.text,host);}
 if(kind==ExpressionTag::Index){ReadStatus read_step_31=read_expression(input,position,fields.a,host);if(read_step_31!=ReadStatus::Success){return read_step_31;}return read_expression(input,position,fields.b,host);}
 if(kind==ExpressionTag::Call){ReadStatus read_step_32=read_text_field(input,position,fields.text,host);if(read_step_32!=ReadStatus::Success){return read_step_32;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Invoke){ReadStatus read_step_33=read_expression(input,position,fields.a,host);if(read_step_33!=ReadStatus::Success){return read_step_33;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Construct){ReadStatus read_step_34=read_type_name(input,position,fields.ty,host);if(read_step_34!=ReadStatus::Success){return read_step_34;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Specialize){ReadStatus read_step_35=read_text_field(input,position,fields.text,host);if(read_step_35!=ReadStatus::Success){return read_step_35;}ReadStatus read_step_36=read_arguments(input,position,fields.types,host);if(read_step_36!=ReadStatus::Success){return read_step_36;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::FunctionRef){ReadStatus read_step_37=read_text_field(input,position,fields.text,host);if(read_step_37!=ReadStatus::Success){return read_step_37;}return read_arguments(input,position,fields.types,host);}
 if(kind==ExpressionTag::Cooperate){ReadStatus read_step_38=read_text_field(input,position,fields.text,host);if(read_step_38!=ReadStatus::Success){return read_step_38;}ReadStatus read_step_39=read_owned_option(input,position,fields.types,host.context,host.arguments_option);if(read_step_39!=ReadStatus::Success){return read_step_39;}return read_owned_values(input,position,fields.arguments,host.context,host.push_expression);}
 if(kind==ExpressionTag::Target){ReadTarget target=read_target(input,position);if(target.status!=ReadStatus::Success){return target.status;}host.target(fields.target,target.value);return read_expression(input,position,fields.a,host);}
 if(kind==ExpressionTag::Unary){ReadStatus read_step_40=read_text_field(input,position,fields.text,host);if(read_step_40!=ReadStatus::Success){return read_step_40;}return read_expression(input,position,fields.a,host);}
 if(kind==ExpressionTag::Binary){ReadStatus read_step_41=read_text_field(input,position,fields.text,host);if(read_step_41!=ReadStatus::Success){return read_step_41;}ReadStatus read_step_42=read_expression(input,position,fields.a,host);if(read_step_42!=ReadStatus::Success){return read_step_42;}return read_expression(input,position,fields.b,host);}
 ReadStatus read_step_43=read_expression(input,position,fields.a,host);if(read_step_43!=ReadStatus::Success){return read_step_43;}
 ReadStatus read_step_44=read_expression(input,position,fields.b,host);if(read_step_44!=ReadStatus::Success){return read_step_44;}
 return read_expression(input,position,fields.c,host);
}
extern "C" ReadStatus read_condition_payload(ConditionTag kind,ConditionFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 if(kind==ConditionTag::Name||kind==ConditionTag::Number){return read_text_field(input,position,fields.text,host);}
 if(kind==ConditionTag::Bool){return read_bool_field(input,position,fields.literal);}
 if(kind==ConditionTag::StaticU32){return read_word_field(input,position,fields.literal);}
 if(kind==ConditionTag::Member){ReadStatus read_step_45=read_condition(input,position,fields.a,host);if(read_step_45!=ReadStatus::Success){return read_step_45;}return read_text_field(input,position,fields.text,host);}
 if(kind==ConditionTag::Unary){ReadStatus read_step_46=read_text_field(input,position,fields.text,host);if(read_step_46!=ReadStatus::Success){return read_step_46;}return read_condition(input,position,fields.a,host);}
 if(kind==ConditionTag::Binary){ReadStatus read_step_47=read_text_field(input,position,fields.text,host);if(read_step_47!=ReadStatus::Success){return read_step_47;}ReadStatus read_step_48=read_condition(input,position,fields.a,host);if(read_step_48!=ReadStatus::Success){return read_step_48;}return read_condition(input,position,fields.b,host);}
 return read_constant(input,position,fields.constant,host);
}
extern "C" ReadStatus read_predicate_payload(PredicateTag kind,PredicateFields fields,Bytes input,inout u64 position,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span) {
 if(kind==PredicateTag::Memory){
  ReadValue present=read_boolean(input,position);if(present.status!=ReadStatus::Success){return present.status;}
  if(present.value!=0u64){ReadAccess access=read_access(input,position);if(access.status!=ReadStatus::Success){return access.status;}host.optional_access(fields.access,access.value);}
  return ReadStatus::Success;
 }
 ReadStatus read_step_49=read_text_field(input,position,fields.first,host);if(read_step_49!=ReadStatus::Success){return read_step_49;}
 if(kind==PredicateTag::Prefix){ReadStatus read_step_50=read_text_field(input,position,fields.second,host);if(read_step_50!=ReadStatus::Success){return read_step_50;}return read_text_field(input,position,fields.third,host);}
 if(kind==PredicateTag::Index||kind==PredicateTag::SameLength){return read_text_field(input,position,fields.second,host);}
 if(kind==PredicateTag::Initialized||kind==PredicateTag::Cleanup){return read_condition(input,position,fields.condition,host);}
 if(kind==PredicateTag::Range){return read_owned_values(input,position,fields.intervals,host.context,host.push_interval);}
 return ReadStatus::Success;
}
