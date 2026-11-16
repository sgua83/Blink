module bootstrap.contract_codec;

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

void numeric_write_scalar(ScalarTag value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value==ScalarTag::F32){word(0,output,append);}
else if(value==ScalarTag::U32){word(1,output,append);}
else if(value==ScalarTag::I32){word(2,output,append);}
else if(value==ScalarTag::Bool){word(3,output,append);}
else if(value==ScalarTag::U64){word(4,output,append);}
else if(value==ScalarTag::I64){word(5,output,append);}
else if(value==ScalarTag::F64){word(6,output,append);}
else if(value==ScalarTag::U8){word(7,output,append);}
else if(value==ScalarTag::I8){word(8,output,append);}
else if(value==ScalarTag::U16){word(9,output,append);}
else if(value==ScalarTag::I16){word(10,output,append);}
}
ReadStatus numeric_read_scalar(Bytes input,inout u64 position,inout ScalarTag output) on cpu {ReadScalar value=read_scalar(input,position);if(value.status!=ReadStatus::Success){return value.status;}output=value.value;return ReadStatus::Success;}
extern "C" void numeric_write_identity(NumericIdentity value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value.kind==IdentityKind::Register){word(0,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::DataLength){word(1,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::TileLength){word(2,output,append);
}
else if(value.kind==IdentityKind::ParameterContent){word(3,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::ResourceParameter){word(4,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::ParameterStorage){word(5,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::ElementComponent){word(6,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::ElementParent){word(7,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::DatasetContent){word(8,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
wide(value.third,output,append);
}
else if(value.kind==IdentityKind::MemoryValue){word(9,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::ElementIndex){word(10,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::LoopValue){word(11,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::TaskResult){word(12,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::TaskCompletion){word(13,output,append);
wide(value.first,output,append);
wide(value.second,output,append);
}
else if(value.kind==IdentityKind::TaskControl){word(14,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::TaskReturnSource){word(15,output,append);
wide(value.first,output,append);
}
else if(value.kind==IdentityKind::TaskWorkCompletion){word(16,output,append);
word(u32(value.first),output,append);
word(u32(value.second),output,append);
}
}
extern "C" ReadStatus numeric_read_identity(Bytes input,inout u64 position,inout NumericIdentity output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
output.kind=IdentityKind::Register;
ReadValue part_1=fixed_value(input,position,8);if(part_1.status!=ReadStatus::Success){return part_1.status;}output.first=part_1.value;
return ReadStatus::Success;}
else if(tag.value==1u64){
output.kind=IdentityKind::DataLength;
ReadValue part_2=fixed_value(input,position,8);if(part_2.status!=ReadStatus::Success){return part_2.status;}output.first=part_2.value;
return ReadStatus::Success;}
else if(tag.value==2u64){
output.kind=IdentityKind::TileLength;
return ReadStatus::Success;}
else if(tag.value==3u64){
output.kind=IdentityKind::ParameterContent;
ReadValue part_3=fixed_value(input,position,8);if(part_3.status!=ReadStatus::Success){return part_3.status;}output.first=part_3.value;
ReadValue part_4=fixed_value(input,position,8);if(part_4.status!=ReadStatus::Success){return part_4.status;}output.second=part_4.value;
return ReadStatus::Success;}
else if(tag.value==4u64){
output.kind=IdentityKind::ResourceParameter;
ReadValue part_5=fixed_value(input,position,8);if(part_5.status!=ReadStatus::Success){return part_5.status;}output.first=part_5.value;
ReadValue part_6=fixed_value(input,position,8);if(part_6.status!=ReadStatus::Success){return part_6.status;}output.second=part_6.value;
return ReadStatus::Success;}
else if(tag.value==5u64){
output.kind=IdentityKind::ParameterStorage;
ReadValue part_7=fixed_value(input,position,8);if(part_7.status!=ReadStatus::Success){return part_7.status;}output.first=part_7.value;
return ReadStatus::Success;}
else if(tag.value==6u64){
output.kind=IdentityKind::ElementComponent;
ReadValue part_8=fixed_value(input,position,8);if(part_8.status!=ReadStatus::Success){return part_8.status;}output.first=part_8.value;
return ReadStatus::Success;}
else if(tag.value==7u64){
output.kind=IdentityKind::ElementParent;
ReadValue part_9=fixed_value(input,position,8);if(part_9.status!=ReadStatus::Success){return part_9.status;}output.first=part_9.value;
ReadValue part_10=fixed_value(input,position,8);if(part_10.status!=ReadStatus::Success){return part_10.status;}output.second=part_10.value;
return ReadStatus::Success;}
else if(tag.value==8u64){
output.kind=IdentityKind::DatasetContent;
ReadValue part_11=fixed_value(input,position,8);if(part_11.status!=ReadStatus::Success){return part_11.status;}output.first=part_11.value;
ReadValue part_12=fixed_value(input,position,8);if(part_12.status!=ReadStatus::Success){return part_12.status;}output.second=part_12.value;
ReadValue part_13=fixed_value(input,position,8);if(part_13.status!=ReadStatus::Success){return part_13.status;}output.third=part_13.value;
return ReadStatus::Success;}
else if(tag.value==9u64){
output.kind=IdentityKind::MemoryValue;
ReadValue part_14=fixed_value(input,position,8);if(part_14.status!=ReadStatus::Success){return part_14.status;}output.first=part_14.value;
ReadValue part_15=fixed_value(input,position,8);if(part_15.status!=ReadStatus::Success){return part_15.status;}output.second=part_15.value;
return ReadStatus::Success;}
else if(tag.value==10u64){
output.kind=IdentityKind::ElementIndex;
ReadValue part_16=fixed_value(input,position,8);if(part_16.status!=ReadStatus::Success){return part_16.status;}output.first=part_16.value;
return ReadStatus::Success;}
else if(tag.value==11u64){
output.kind=IdentityKind::LoopValue;
ReadValue part_17=fixed_value(input,position,8);if(part_17.status!=ReadStatus::Success){return part_17.status;}output.first=part_17.value;
ReadValue part_18=fixed_value(input,position,8);if(part_18.status!=ReadStatus::Success){return part_18.status;}output.second=part_18.value;
return ReadStatus::Success;}
else if(tag.value==12u64){
output.kind=IdentityKind::TaskResult;
ReadValue part_19=fixed_value(input,position,8);if(part_19.status!=ReadStatus::Success){return part_19.status;}output.first=part_19.value;
return ReadStatus::Success;}
else if(tag.value==13u64){
output.kind=IdentityKind::TaskCompletion;
ReadValue part_20=fixed_value(input,position,8);if(part_20.status!=ReadStatus::Success){return part_20.status;}output.first=part_20.value;
ReadValue part_21=fixed_value(input,position,8);if(part_21.status!=ReadStatus::Success){return part_21.status;}output.second=part_21.value;
return ReadStatus::Success;}
else if(tag.value==14u64){
output.kind=IdentityKind::TaskControl;
ReadValue part_22=fixed_value(input,position,8);if(part_22.status!=ReadStatus::Success){return part_22.status;}output.first=part_22.value;
return ReadStatus::Success;}
else if(tag.value==15u64){
output.kind=IdentityKind::TaskReturnSource;
ReadValue part_23=fixed_value(input,position,8);if(part_23.status!=ReadStatus::Success){return part_23.status;}output.first=part_23.value;
return ReadStatus::Success;}
else if(tag.value==16u64){
output.kind=IdentityKind::TaskWorkCompletion;
ReadValue part_24=fixed_value(input,position,4);if(part_24.status!=ReadStatus::Success){return part_24.status;}output.first=u64(u32(part_24.value));
ReadValue part_25=fixed_value(input,position,4);if(part_25.status!=ReadStatus::Success){return part_25.status;}output.second=u64(u32(part_25.value));
return ReadStatus::Success;}
return ReadStatus::Identity;
}
extern "C" void numeric_write_operand(NumericOperand value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value.kind==OperandKind::Value){word(0,output,append);
numeric_write_identity(value.identity,output,append);
}
else if(value.kind==OperandKind::Constant){word(1,output,append);
numeric_write_scalar(value.scalar,output,append);
wide(value.bits,output,append);
}
}
extern "C" ReadStatus numeric_read_operand(Bytes input,inout u64 position,inout NumericOperand output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
output.kind=OperandKind::Value;
ReadStatus status_26=numeric_read_identity(input,position,output.identity);if(status_26!=ReadStatus::Success){return status_26;}
return ReadStatus::Success;}
else if(tag.value==1u64){
output.kind=OperandKind::Constant;
ReadStatus status_27=numeric_read_scalar(input,position,output.scalar);if(status_27!=ReadStatus::Success){return status_27;}
ReadValue part_28=fixed_value(input,position,8);if(part_28.status!=ReadStatus::Success){return part_28.status;}output.bits=part_28.value;
return ReadStatus::Success;}
return ReadStatus::Operand;
}
extern "C" void numeric_write_integer(IntegerKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value==IntegerKind::Add){word(0,output,append);
}
else if(value==IntegerKind::Subtract){word(1,output,append);
}
else if(value==IntegerKind::Multiply){word(2,output,append);
}
else if(value==IntegerKind::Divide){word(3,output,append);
}
else if(value==IntegerKind::Remainder){word(4,output,append);
}
else if(value==IntegerKind::And){word(5,output,append);
}
else if(value==IntegerKind::Or){word(6,output,append);
}
else if(value==IntegerKind::Xor){word(7,output,append);
}
else if(value==IntegerKind::ShiftLeft){word(8,output,append);
}
else if(value==IntegerKind::ShiftRight){word(9,output,append);
}
}
extern "C" ReadStatus numeric_read_integer(Bytes input,inout u64 position,inout IntegerKind output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
output=IntegerKind::Add;
return ReadStatus::Success;}
else if(tag.value==1u64){
output=IntegerKind::Subtract;
return ReadStatus::Success;}
else if(tag.value==2u64){
output=IntegerKind::Multiply;
return ReadStatus::Success;}
else if(tag.value==3u64){
output=IntegerKind::Divide;
return ReadStatus::Success;}
else if(tag.value==4u64){
output=IntegerKind::Remainder;
return ReadStatus::Success;}
else if(tag.value==5u64){
output=IntegerKind::And;
return ReadStatus::Success;}
else if(tag.value==6u64){
output=IntegerKind::Or;
return ReadStatus::Success;}
else if(tag.value==7u64){
output=IntegerKind::Xor;
return ReadStatus::Success;}
else if(tag.value==8u64){
output=IntegerKind::ShiftLeft;
return ReadStatus::Success;}
else if(tag.value==9u64){
output=IntegerKind::ShiftRight;
return ReadStatus::Success;}
return ReadStatus::IntegerOperation;
}
extern "C" void numeric_write_expression(NumericExpression value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(true){word(0,output,append);
numeric_write_integer(value.operation,output,append);
numeric_write_scalar(value.scalar,output,append);
numeric_write_operand(value.left,output,append);
numeric_write_operand(value.right,output,append);
}
}
extern "C" ReadStatus numeric_read_expression(Bytes input,inout u64 position,inout NumericExpression output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
ReadStatus status_29=numeric_read_integer(input,position,output.operation);if(status_29!=ReadStatus::Success){return status_29;}
ReadStatus status_30=numeric_read_scalar(input,position,output.scalar);if(status_30!=ReadStatus::Success){return status_30;}
ReadStatus status_31=numeric_read_operand(input,position,output.left);if(status_31!=ReadStatus::Success){return status_31;}
ReadStatus status_32=numeric_read_operand(input,position,output.right);if(status_32!=ReadStatus::Success){return status_32;}
return ReadStatus::Success;}
return ReadStatus::IntegerExpression;
}
extern "C" void numeric_write_comparison(ComparisonKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value==ComparisonKind::Equal){word(0,output,append);
}
else if(value==ComparisonKind::NotEqual){word(1,output,append);
}
else if(value==ComparisonKind::Less){word(2,output,append);
}
else if(value==ComparisonKind::Greater){word(3,output,append);
}
else if(value==ComparisonKind::LessEqual){word(4,output,append);
}
else if(value==ComparisonKind::GreaterEqual){word(5,output,append);
}
}
extern "C" ReadStatus numeric_read_comparison(Bytes input,inout u64 position,inout ComparisonKind output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
output=ComparisonKind::Equal;
return ReadStatus::Success;}
else if(tag.value==1u64){
output=ComparisonKind::NotEqual;
return ReadStatus::Success;}
else if(tag.value==2u64){
output=ComparisonKind::Less;
return ReadStatus::Success;}
else if(tag.value==3u64){
output=ComparisonKind::Greater;
return ReadStatus::Success;}
else if(tag.value==4u64){
output=ComparisonKind::LessEqual;
return ReadStatus::Success;}
else if(tag.value==5u64){
output=ComparisonKind::GreaterEqual;
return ReadStatus::Success;}
return ReadStatus::Comparison;
}
extern "C" void numeric_write_floating_form(FloatingKind value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value==FloatingKind::Cast){word(0,output,append);
}
else if(value==FloatingKind::Upper){word(1,output,append);
}
}
extern "C" ReadStatus numeric_read_floating_form(Bytes input,inout u64 position,inout FloatingKind output) on cpu {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){
output=FloatingKind::Cast;
return ReadStatus::Success;}
else if(tag.value==1u64){
output=FloatingKind::Upper;
return ReadStatus::Success;}
return ReadStatus::FloatingForm;
}
extern "C" void numeric_write_floating(NumericFloating value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
numeric_write_identity(value.source,output,append);
wide(value.lower,output,append);
wide(value.upper,output,append);
wide(value.terms[0],output,append);
wide(value.terms[1],output,append);
wide(value.terms[2],output,append);
numeric_write_scalar(value.scalar,output,append);
numeric_write_floating_form(value.form,output,append);
}
extern "C" ReadStatus numeric_read_floating(Bytes input,inout u64 position,inout NumericFloating output) on cpu {
ReadStatus status_33=numeric_read_identity(input,position,output.source);if(status_33!=ReadStatus::Success){return status_33;}
ReadValue part_34=fixed_value(input,position,8);if(part_34.status!=ReadStatus::Success){return part_34.status;}output.lower=part_34.value;
ReadValue part_35=fixed_value(input,position,8);if(part_35.status!=ReadStatus::Success){return part_35.status;}output.upper=part_35.value;
ReadValue part_36=fixed_value(input,position,8);if(part_36.status!=ReadStatus::Success){return part_36.status;}output.terms[0]=part_36.value;
ReadValue part_37=fixed_value(input,position,8);if(part_37.status!=ReadStatus::Success){return part_37.status;}output.terms[1]=part_37.value;
ReadValue part_38=fixed_value(input,position,8);if(part_38.status!=ReadStatus::Success){return part_38.status;}output.terms[2]=part_38.value;
ReadStatus status_39=numeric_read_scalar(input,position,output.scalar);if(status_39!=ReadStatus::Success){return status_39;}
ReadStatus status_40=numeric_read_floating_form(input,position,output.form);if(status_40!=ReadStatus::Success){return status_40;}
return ReadStatus::Success;
}
extern "C" void numeric_write_domain(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) {
DomainFields value=host.domain(source);
if(value.kind==DomainKind::Data){word(0,output,host.append);
wide(value.ordinal,output,host.append);
}
else if(value.kind==DomainKind::Fixed){word(1,output,host.append);
word(u32(value.distance),output,host.append);
}
else if(value.kind==DomainKind::Tile){word(2,output,host.append);
}
else if(value.kind==DomainKind::DataLength){word(3,output,host.append);
wide(value.ordinal,output,host.append);
}
else if(value.kind==DomainKind::TileLength){word(4,output,host.append);
}
else if(value.kind==DomainKind::View){word(5,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
numeric_write_identity(host.identity(value.third),output,host.append);
}
else if(value.kind==DomainKind::ViewLength){word(6,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
numeric_write_identity(host.identity(value.third),output,host.append);
}
else if(value.kind==DomainKind::ApplicationData){word(7,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
}
else if(value.kind==DomainKind::ApplicationLength){word(8,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
}
else if(value.kind==DomainKind::Upper){word(9,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
word(u32(value.distance),output,host.append);
}
else if(value.kind==DomainKind::Room){word(10,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
}
else if(value.kind==DomainKind::Sum){word(11,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
}
else if(value.kind==DomainKind::Shifted){word(12,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
word(u32(value.distance),output,host.append);
}
else if(value.kind==DomainKind::Offset){word(13,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
word(u32(value.distance),output,host.append);
}
else if(value.kind==DomainKind::Computed){word(14,output,host.append);
numeric_write_expression(host.expression(value.expression),output,host.append);
}
else if(value.kind==DomainKind::Lower){word(15,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
word(u32(value.distance),output,host.append);
}
else if(value.kind==DomainKind::CommonRange){word(16,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_scalar(value.scalars[0],output,host.append);
numeric_write_scalar(value.scalars[1],output,host.append);
numeric_write_scalar(value.scalars[2],output,host.append);
numeric_write_scalar(value.scalars[3],output,host.append);
}
else if(value.kind==DomainKind::KnownBits){word(17,output,host.append);
numeric_write_scalar(value.scalars[0],output,host.append);
wide(value.mask,output,host.append);
wide(value.bits,output,host.append);
}
else if(value.kind==DomainKind::Floating){word(18,output,host.append);
numeric_write_floating(host.floating(value.floating),output,host.append);
}
}

extern "C" ReadStatus numeric_read_domain(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.domain) {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){return host.domain(host.context,output,DomainKind::Data);}
else if(tag.value==1u64){return host.domain(host.context,output,DomainKind::Fixed);}
else if(tag.value==2u64){return host.domain(host.context,output,DomainKind::Tile);}
else if(tag.value==3u64){return host.domain(host.context,output,DomainKind::DataLength);}
else if(tag.value==4u64){return host.domain(host.context,output,DomainKind::TileLength);}
else if(tag.value==5u64){return host.domain(host.context,output,DomainKind::View);}
else if(tag.value==6u64){return host.domain(host.context,output,DomainKind::ViewLength);}
else if(tag.value==7u64){return host.domain(host.context,output,DomainKind::ApplicationData);}
else if(tag.value==8u64){return host.domain(host.context,output,DomainKind::ApplicationLength);}
else if(tag.value==9u64){return host.domain(host.context,output,DomainKind::Upper);}
else if(tag.value==10u64){return host.domain(host.context,output,DomainKind::Room);}
else if(tag.value==11u64){return host.domain(host.context,output,DomainKind::Sum);}
else if(tag.value==12u64){return host.domain(host.context,output,DomainKind::Shifted);}
else if(tag.value==13u64){return host.domain(host.context,output,DomainKind::Offset);}
else if(tag.value==14u64){return host.domain(host.context,output,DomainKind::Computed);}
else if(tag.value==15u64){return host.domain(host.context,output,DomainKind::Lower);}
else if(tag.value==16u64){return host.domain(host.context,output,DomainKind::CommonRange);}
else if(tag.value==17u64){return host.domain(host.context,output,DomainKind::KnownBits);}
else if(tag.value==18u64){return host.domain(host.context,output,DomainKind::Floating);}
return ReadStatus::IndexDomain;
}

extern "C" void numeric_write_predicate(NumericPredicate value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
numeric_write_comparison(value.operation,output,append);
numeric_write_scalar(value.scalar,output,append);
numeric_write_operand(value.left,output,append);
numeric_write_operand(value.right,output,append);
}
extern "C" ReadStatus numeric_read_predicate(Bytes input,inout u64 position,inout NumericPredicate output) on cpu {
ReadStatus status_74=numeric_read_comparison(input,position,output.operation);if(status_74!=ReadStatus::Success){return status_74;}
ReadStatus status_75=numeric_read_scalar(input,position,output.scalar);if(status_75!=ReadStatus::Success){return status_75;}
ReadStatus status_76=numeric_read_operand(input,position,output.left);if(status_76!=ReadStatus::Success){return status_76;}
ReadStatus status_77=numeric_read_operand(input,position,output.right);if(status_77!=ReadStatus::Success){return status_77;}
return ReadStatus::Success;
}
extern "C" void numeric_write_fact(address source,address output,NumericWriter host) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate) {
FactFields value=host.fact(source);
if(value.kind==FactKind::Equal){word(0,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
}
else if(value.kind==FactKind::Index){word(1,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_domain(value.domain,output,host);
}
else if(value.kind==FactKind::Minimum){word(2,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_scalar(value.scalar,output,host.append);
wide(value.bits,output,host.append);
}
else if(value.kind==FactKind::Maximum){word(3,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_scalar(value.scalar,output,host.append);
wide(value.bits,output,host.append);
}
else if(value.kind==FactKind::NonZero){word(4,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
}
else if(value.kind==FactKind::Boolean){word(5,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
byte(u8(value.truth),output,host.append);
}
else if(value.kind==FactKind::Below){word(6,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
byte(u8(value.truth),output,host.append);
numeric_write_scalar(value.scalar,output,host.append);
}
else if(value.kind==FactKind::Outside){word(7,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_domain(value.domain,output,host);
}
else if(value.kind==FactKind::Zero){word(8,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
}
else if(value.kind==FactKind::NotEqual){word(9,output,host.append);
numeric_write_identity(host.identity(value.first),output,host.append);
numeric_write_identity(host.identity(value.second),output,host.append);
}
else if(value.kind==FactKind::Predicate){word(10,output,host.append);
numeric_write_predicate(host.predicate(value.predicate),output,host.append);
byte(u8(value.truth),output,host.append);
}
}

extern "C" ReadStatus numeric_read_fact(Bytes input,inout u64 position,address output,NumericReader host) on cpu requires nonzero(host.fact) {
ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
if(tag.value==0u64){return host.fact(host.context,output,FactKind::Equal);}
else if(tag.value==1u64){return host.fact(host.context,output,FactKind::Index);}
else if(tag.value==2u64){return host.fact(host.context,output,FactKind::Minimum);}
else if(tag.value==3u64){return host.fact(host.context,output,FactKind::Maximum);}
else if(tag.value==4u64){return host.fact(host.context,output,FactKind::NonZero);}
else if(tag.value==5u64){return host.fact(host.context,output,FactKind::Boolean);}
else if(tag.value==6u64){return host.fact(host.context,output,FactKind::Below);}
else if(tag.value==7u64){return host.fact(host.context,output,FactKind::Outside);}
else if(tag.value==8u64){return host.fact(host.context,output,FactKind::Zero);}
else if(tag.value==9u64){return host.fact(host.context,output,FactKind::NotEqual);}
else if(tag.value==10u64){return host.fact(host.context,output,FactKind::Predicate);}
return ReadStatus::Fact;
}

extern "C" void numeric_write_ranges(Span values,address output,RangeWriter host) on cpu requires nonzero(host.append) requires nonzero(host.interval) {
 wide(values.count,output,host.append);u64 index=0u64;
 while(index<values.count){NumericInterval value=host.interval(item(values,index));wide(value.lower,output,host.append);wide(value.upper,output,host.append);index+=1u64;}
}
extern "C" ReadStatus numeric_read_ranges(Bytes input,inout u64 position,address output,cfn<void,address,u64,u64> push) on cpu requires nonzero(push) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;while(index<count.value){
  ReadValue lower=fixed_value(input,position,8);if(lower.status!=ReadStatus::Success){return lower.status;}
  ReadValue upper=fixed_value(input,position,8);if(upper.status!=ReadStatus::Success){return upper.status;}
  push(output,lower.value,upper.value);index+=1u64;
 }return ReadStatus::Success;
}





extern "C" ReadStatus numeric_read_payload_ordinal(Bytes input,inout u64 position,inout u64 field0) on cpu {
ReadValue part_157=fixed_value(input,position,8);if(part_157.status!=ReadStatus::Success){return part_157.status;}field0=part_157.value;
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_count(Bytes input,inout u64 position,inout u32 field0) on cpu {
ReadValue part_158=fixed_value(input,position,4);if(part_158.status!=ReadStatus::Success){return part_158.status;}field0=u32(part_158.value);
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_view_identities(Bytes input,inout u64 position,inout array<NumericIdentity,3> field0) on cpu {
ReadStatus status_159=numeric_read_identity(input,position,field0[0]);if(status_159!=ReadStatus::Success){return status_159;}
ReadStatus status_160=numeric_read_identity(input,position,field0[1]);if(status_160!=ReadStatus::Success){return status_160;}
ReadStatus status_161=numeric_read_identity(input,position,field0[2]);if(status_161!=ReadStatus::Success){return status_161;}
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_identity_distance(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu {
ReadStatus status_162=numeric_read_identity(input,position,field0);if(status_162!=ReadStatus::Success){return status_162;}
ReadValue part_163=fixed_value(input,position,4);if(part_163.status!=ReadStatus::Success){return part_163.status;}field1=u32(part_163.value);
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_identity_pair(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1) on cpu {
ReadStatus status_164=numeric_read_identity(input,position,field0);if(status_164!=ReadStatus::Success){return status_164;}
ReadStatus status_165=numeric_read_identity(input,position,field1);if(status_165!=ReadStatus::Success){return status_165;}
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_shifted_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2) on cpu {
ReadStatus status_166=numeric_read_identity(input,position,field0);if(status_166!=ReadStatus::Success){return status_166;}
ReadStatus status_167=numeric_read_identity(input,position,field1);if(status_167!=ReadStatus::Success){return status_167;}
ReadValue part_168=fixed_value(input,position,4);if(part_168.status!=ReadStatus::Success){return part_168.status;}field2=u32(part_168.value);
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_common_range(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout ScalarTag field2,inout ScalarTag field3,inout ScalarTag field4) on cpu {
ReadStatus status_169=numeric_read_identity(input,position,field0);if(status_169!=ReadStatus::Success){return status_169;}
ReadStatus status_170=numeric_read_scalar(input,position,field1);if(status_170!=ReadStatus::Success){return status_170;}
ReadStatus status_171=numeric_read_scalar(input,position,field2);if(status_171!=ReadStatus::Success){return status_171;}
ReadStatus status_172=numeric_read_scalar(input,position,field3);if(status_172!=ReadStatus::Success){return status_172;}
ReadStatus status_173=numeric_read_scalar(input,position,field4);if(status_173!=ReadStatus::Success){return status_173;}
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_known_bits(Bytes input,inout u64 position,inout ScalarTag field0,inout u64 field1,inout u64 field2) on cpu {
ReadStatus status_174=numeric_read_scalar(input,position,field0);if(status_174!=ReadStatus::Success){return status_174;}
ReadValue part_175=fixed_value(input,position,8);if(part_175.status!=ReadStatus::Success){return part_175.status;}field1=part_175.value;
ReadValue part_176=fixed_value(input,position,8);if(part_176.status!=ReadStatus::Success){return part_176.status;}field2=part_176.value;
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_identity_domain(Bytes input,inout u64 position,inout NumericIdentity field0,address field1,NumericReader host) on cpu requires nonzero(host.domain) {
ReadStatus status_177=numeric_read_identity(input,position,field0);if(status_177!=ReadStatus::Success){return status_177;}
ReadStatus domain_status=numeric_read_domain(input,position,field1,host);if(domain_status!=ReadStatus::Success){return domain_status;}
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_identity_scalar_bits(Bytes input,inout u64 position,inout NumericIdentity field0,inout ScalarTag field1,inout u64 field2) on cpu {
ReadStatus status_178=numeric_read_identity(input,position,field0);if(status_178!=ReadStatus::Success){return status_178;}
ReadStatus status_179=numeric_read_scalar(input,position,field1);if(status_179!=ReadStatus::Success){return status_179;}
ReadValue part_180=fixed_value(input,position,8);if(part_180.status!=ReadStatus::Success){return part_180.status;}field2=part_180.value;
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_identity_truth(Bytes input,inout u64 position,inout NumericIdentity field0,inout u32 field1) on cpu {
ReadStatus status_181=numeric_read_identity(input,position,field0);if(status_181!=ReadStatus::Success){return status_181;}
ReadValue part_182=read_boolean(input,position);if(part_182.status!=ReadStatus::Success){return part_182.status;}field1=u32(part_182.value);
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_ordered_identities(Bytes input,inout u64 position,inout NumericIdentity field0,inout NumericIdentity field1,inout u32 field2,inout ScalarTag field3) on cpu {
ReadStatus status_183=numeric_read_identity(input,position,field0);if(status_183!=ReadStatus::Success){return status_183;}
ReadStatus status_184=numeric_read_identity(input,position,field1);if(status_184!=ReadStatus::Success){return status_184;}
ReadValue part_185=read_boolean(input,position);if(part_185.status!=ReadStatus::Success){return part_185.status;}field2=u32(part_185.value);
ReadStatus status_186=numeric_read_scalar(input,position,field3);if(status_186!=ReadStatus::Success){return status_186;}
return ReadStatus::Success;
}
extern "C" ReadStatus numeric_read_payload_predicate_truth(Bytes input,inout u64 position,inout NumericPredicate field0,inout u32 field1) on cpu {
ReadStatus status_187=numeric_read_predicate(input,position,field0);if(status_187!=ReadStatus::Success){return status_187;}
ReadValue part_188=read_boolean(input,position);if(part_188.status!=ReadStatus::Success){return part_188.status;}field1=u32(part_188.value);
return ReadStatus::Success;
}

extern "C" void condition_node_ids(address context,CanonicalHost host) on cpu requires nonzero(host.next) requires nonzero(host.index) requires nonzero(host.len) requires nonzero(host.intern) requires nonzero(host.record) requires nonzero(host.bind) {
 ClauseFields value=host.next(context);
 while(value.present!=0){
  NodeKind kind=NodeKind::Impossible;u64 a=0u64;u64 b=0u64;
  if(value.kind==ClauseKind::Facts){kind=NodeKind::Facts;}
  else if(value.kind==ClauseKind::Both||value.kind==ClauseKind::Branch){kind=NodeKind::Both;a=host.index(context,value.first);b=host.index(context,value.second);}
  else if(value.kind==ClauseKind::Either){kind=NodeKind::Either;a=host.index(context,value.first);b=host.index(context,value.second);}
  u64 index=0u64;
  if((kind==NodeKind::Both||kind==NodeKind::Either)&&a==b){index=a;}
  else{
   u64 next=host.len(context);Inserted found=host.intern(context,kind,value.facts,a,b,next);index=found.index;
   if(found.inserted!=0){host.record(context,kind,value.facts,a,b);}
  }
  host.bind(context,value.key,index);value=host.next(context);
 }
}
extern "C" void write_proof_conditions(address context,u64 length,u64 count,address output,ConditionWriter host) on cpu requires nonzero(host.append) requires nonzero(host.condition) requires nonzero(host.node) requires nonzero(host.index) requires nonzero(host.facts) {
 u64 width=length;
 while(width!=0u64){RootFields last=host.condition(context,width-1u64);if(last.present!=0){break;}width-=1u64;}
 byte(width!=0u64?u8(1):u8(0),output,host.append);
 if(width==0u64){return;}
 byte(u8(3),output,host.append);wide(count,output,host.append);
 u64 index=0u64;
 while(index<count){
  NodeFields value=host.node(context,index);
  if(value.kind==NodeKind::Impossible){word(0,output,host.append);}
  else if(value.kind==NodeKind::Facts){word(1,output,host.append);host.facts(value.facts,output);}
  else if(value.kind==NodeKind::Both){word(2,output,host.append);wide(value.first,output,host.append);wide(value.second,output,host.append);}
  else{word(3,output,host.append);wide(value.first,output,host.append);wide(value.second,output,host.append);}
  index+=1u64;
 }
 wide(width,output,host.append);index=0u64;
 while(index<width){
  RootFields value=host.condition(context,index);byte(value.present!=0?u8(1):u8(0),output,host.append);
  if(value.present!=0){wide(host.index(context,value.first),output,host.append);wide(host.index(context,value.second),output,host.append);}
  index+=1u64;
 }
}
extern "C" void write_fact_sequence(u64 count,address iterator,address output,NumericWriter host,cfn<address,address> next) on cpu requires nonzero(host.append) requires nonzero(host.identity) requires nonzero(host.expression) requires nonzero(host.floating) requires nonzero(host.domain) requires nonzero(host.fact) requires nonzero(host.predicate) requires nonzero(next) {
 wide(count,output,host.append);u64 index=0u64;
 while(index<count){numeric_write_fact(next(iterator),output,host);index+=1u64;}
}
extern "C" ReadStatus read_fact_sequence(Bytes input,inout u64 position,address context,address output,cfn<ReadStatus,address,address> push) on cpu requires nonzero(push) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){ReadStatus status=push(context,output);if(status!=ReadStatus::Success){return status;}index+=1u64;}
 return ReadStatus::Success;
}
extern "C" ReadStatus read_condition_nodes(Bytes input,inout u64 position,inout u32 present,address output,ConditionReader host) on cpu requires nonzero(host.impossible) requires nonzero(host.facts) requires nonzero(host.both) requires nonzero(host.either) {
 ReadValue active=read_boolean(input,position);if(active.status!=ReadStatus::Success){return active.status;}
 present=u32(active.value);if(present==0){return ReadStatus::Success;}
 ReadValue version=fixed_value(input,position,1);if(version.status!=ReadStatus::Success){return version.status;}
 if(version.value!=3u64){return ReadStatus::ConditionVersion;}
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){
  ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}
  if(tag.value==0u64){host.impossible(output);}
  else if(tag.value==1u64){ReadStatus status=host.facts(host.context,output);if(status!=ReadStatus::Success){return status;}}
  else if(tag.value==2u64||tag.value==3u64){
   ReadValue a=fixed_value(input,position,8);if(a.status!=ReadStatus::Success){return a.status;}
   ReadValue b=fixed_value(input,position,8);if(b.status!=ReadStatus::Success){return b.status;}
   if(tag.value==2u64){host.both(output,a.value,b.value);}else{host.either(output,a.value,b.value);}
  }else{return ReadStatus::ConditionNode;}
  index+=1u64;
 }
 return ReadStatus::Success;
}
extern "C" ReadStatus read_condition_roots(Bytes input,inout u64 position,address output,cfn<void,address,u32,u64,u64> push) on cpu requires nonzero(push) {
 ReadValue count=fixed_value(input,position,8);if(count.status!=ReadStatus::Success){return count.status;}
 u64 index=0u64;
 while(index<count.value){
  ReadValue active=read_boolean(input,position);if(active.status!=ReadStatus::Success){return active.status;}
  u64 a=0u64;u64 b=0u64;
  if(active.value!=0u64){
   ReadValue first=fixed_value(input,position,8);if(first.status!=ReadStatus::Success){return first.status;}a=first.value;
   ReadValue second=fixed_value(input,position,8);if(second.status!=ReadStatus::Success){return second.status;}b=second.value;
  }
  push(output,u32(active.value),a,b);index+=1u64;
 }
 return ReadStatus::Success;
}
