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


// The complete recursive public contract. Native callbacks project or construct
// one nominal Rust field; they never call the former Rust byte codecs.
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
extern "C" void contract_write_requirement(address key,address condition,address output,cfn<void,address,address> key_write,cfn<void,address,address> condition_write,cfn<void,address,address,u64> append) on cpu requires nonzero(key_write) requires nonzero(condition_write) requires nonzero(append){key_write(key,output);wide(1u64,output,append);condition_write(condition,output);}
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

extern "C" void contract_write_value(address output,ContractValueFields fields,ContractValueWriter host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant){host.proof(fields.proof,output);host.literal(fields.literal,output);host.ty(fields.ty,output);host.regs(fields.regs,output);host.uniform(fields.uniform,output);host.constant(fields.constant,output);}
extern "C" ReadStatus contract_read_value(address context,ContractValueFields fields,ContractValueReader host) on cpu requires nonzero(host.proof) requires nonzero(host.literal) requires nonzero(host.ty) requires nonzero(host.regs) requires nonzero(host.uniform) requires nonzero(host.constant){ReadStatus s0=host.proof(context,fields.proof);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.literal(context,fields.literal);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.ty(context,fields.ty);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.regs(context,fields.regs);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.uniform(context,fields.uniform);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.constant(context,fields.constant);if(s5!=ReadStatus::Success){return s5;}return ReadStatus::Success;}
extern "C" void contract_write_address_layout(address output,ContractAddressLayoutFields fields,ContractAddressLayoutWriter host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts){host.size(fields.size,output);host.align(fields.align,output);host.parts(fields.parts,output);}
extern "C" ReadStatus contract_read_address_layout(address context,ContractAddressLayoutFields fields,ContractAddressLayoutReader host) on cpu requires nonzero(host.size) requires nonzero(host.align) requires nonzero(host.parts){ReadStatus s0=host.size(context,fields.size);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.align(context,fields.align);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.parts(context,fields.parts);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_local(address output,ContractAddressRootLocalFields fields,ContractAddressRootLocalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.base) requires nonzero(host.len) requires nonzero(append){word(0,output,append);host.base(fields.base,output);host.len(fields.len,output);}
extern "C" ReadStatus contract_read_address_root_local(address context,ContractAddressRootLocalFields fields,ContractAddressRootLocalReader host) on cpu requires nonzero(host.base) requires nonzero(host.len){ReadStatus s0=host.base(context,fields.base);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.len(context,fields.len);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_parameter(address output,ContractAddressRootParameterFields fields,ContractAddressRootParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append){word(1,output,append);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_address_root_parameter(address context,ContractAddressRootParameterFields fields,ContractAddressRootParameterReader host) on cpu requires nonzero(host.index){ReadStatus s0=host.index(context,fields.index);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_resource(address output,ContractAddressRootResourceFields fields,ContractAddressRootResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append){word(2,output,append);host.borrow(fields.borrow,output);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_address_root_resource(address context,ContractAddressRootResourceFields fields,ContractAddressRootResourceReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index){ReadStatus s0=host.borrow(context,fields.borrow);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.index(context,fields.index);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_buffer(address output,ContractAddressRootBufferFields fields,ContractAddressRootBufferWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.binding) requires nonzero(append){word(3,output,append);host.binding(fields.binding,output);}
extern "C" ReadStatus contract_read_address_root_buffer(address context,ContractAddressRootBufferFields fields,ContractAddressRootBufferReader host) on cpu requires nonzero(host.binding){ReadStatus s0=host.binding(context,fields.binding);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_derived(address output,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append){word(4,output,append);host.borrow(fields.borrow,output);}
extern "C" ReadStatus contract_read_address_root_derived(address context,ContractAddressRootDerivedFields fields,ContractAddressRootDerivedReader host) on cpu requires nonzero(host.borrow){ReadStatus s0=host.borrow(context,fields.borrow);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_external(address output,ContractAddressRootExternalFields fields,ContractAddressRootExternalWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.identity) requires nonzero(append){word(5,output,append);host.identity(fields.identity,output);}
extern "C" ReadStatus contract_read_address_root_external(address context,ContractAddressRootExternalFields fields,ContractAddressRootExternalReader host) on cpu requires nonzero(host.identity){ReadStatus s0=host.identity(context,fields.identity);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_root_dataset(address output,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(host.index) requires nonzero(append){word(6,output,append);host.borrow(fields.borrow,output);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_address_root_dataset(address context,ContractAddressRootDatasetFields fields,ContractAddressRootDatasetReader host) on cpu requires nonzero(host.borrow) requires nonzero(host.index){ReadStatus s0=host.borrow(context,fields.borrow);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.index(context,fields.index);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_initialization_source_position(address output,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append){word(0,output,append);host.origin(fields.origin,output);}
extern "C" ReadStatus contract_read_initialization_source_position(address context,ContractInitializationSourcePositionFields fields,ContractInitializationSourcePositionReader host) on cpu requires nonzero(host.origin){ReadStatus s0=host.origin(context,fields.origin);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_initialization_source_task(address output,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append){word(1,output,append);host.borrow(fields.borrow,output);}
extern "C" ReadStatus contract_read_initialization_source_task(address context,ContractInitializationSourceTaskFields fields,ContractInitializationSourceTaskReader host) on cpu requires nonzero(host.borrow){ReadStatus s0=host.borrow(context,fields.borrow);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_initialization(address output,ContractInitializationFields fields,ContractInitializationWriter host) on cpu requires nonzero(host.source) requires nonzero(host.when){host.source(fields.source,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_initialization(address context,ContractInitializationFields fields,ContractInitializationReader host) on cpu requires nonzero(host.source) requires nonzero(host.when){ReadStatus s0=host.source(context,fields.source);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.when(context,fields.when);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_data_payload(address output,ContractDataPayloadFields fields,ContractDataPayloadWriter host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes){host.root(fields.root,output);host.ty(fields.ty,output);host.length(fields.length,output);host.ownership(fields.ownership,output);host.access(fields.access,output);host.count(fields.count,output);host.residency(fields.residency,output);host.layout(fields.layout,output);host.elements(fields.elements,output);host.prefixes(fields.prefixes,output);}
extern "C" ReadStatus contract_read_data_payload(address context,ContractDataPayloadFields fields,ContractDataPayloadReader host) on cpu requires nonzero(host.root) requires nonzero(host.ty) requires nonzero(host.length) requires nonzero(host.ownership) requires nonzero(host.access) requires nonzero(host.count) requires nonzero(host.residency) requires nonzero(host.layout) requires nonzero(host.elements) requires nonzero(host.prefixes){ReadStatus s0=host.root(context,fields.root);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.ty(context,fields.ty);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.length(context,fields.length);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.ownership(context,fields.ownership);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.access(context,fields.access);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.count(context,fields.count);if(s5!=ReadStatus::Success){return s5;}ReadStatus s6=host.residency(context,fields.residency);if(s6!=ReadStatus::Success){return s6;}ReadStatus s7=host.layout(context,fields.layout);if(s7!=ReadStatus::Success){return s7;}ReadStatus s8=host.elements(context,fields.elements);if(s8!=ReadStatus::Success){return s8;}ReadStatus s9=host.prefixes(context,fields.prefixes);if(s9!=ReadStatus::Success){return s9;}return ReadStatus::Success;}
extern "C" void contract_write_element_facts_unknown(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_element_facts_known(address output,ContractElementFactsKnownFields fields,ContractElementFactsKnownWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append){word(1,output,append);host.value(fields.value,output);}
extern "C" ReadStatus contract_read_element_facts_known(address context,ContractElementFactsKnownFields fields,ContractElementFactsKnownReader host) on cpu requires nonzero(host.value){ReadStatus s0=host.value(context,fields.value);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_element_facts_cleanup(address output,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append){word(2,output,append);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_element_facts_cleanup(address context,ContractElementFactsCleanupFields fields,ContractElementFactsCleanupReader host) on cpu requires nonzero(host.when){ReadStatus s0=host.when(context,fields.when);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_element_facts_common(address output,ContractElementFactsCommonFields fields,ContractElementFactsCommonWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append){word(3,output,append);host.a(fields.a,output);host.b(fields.b,output);}
extern "C" ReadStatus contract_read_element_facts_common(address context,ContractElementFactsCommonFields fields,ContractElementFactsCommonReader host) on cpu requires nonzero(host.a) requires nonzero(host.b){ReadStatus s0=host.a(context,fields.a);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.b(context,fields.b);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_element_facts_selected(address output,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append){word(4,output,append);host.when(fields.when,output);host.a(fields.a,output);host.b(fields.b,output);}
extern "C" ReadStatus contract_read_element_facts_selected(address context,ContractElementFactsSelectedFields fields,ContractElementFactsSelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b){ReadStatus s0=host.when(context,fields.when);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.a(context,fields.a);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.b(context,fields.b);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_element_facts_input(address output,ContractElementFactsInputFields fields,ContractElementFactsInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(append){word(5,output,append);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_element_facts_input(address context,ContractElementFactsInputFields fields,ContractElementFactsInputReader host) on cpu requires nonzero(host.when){ReadStatus s0=host.when(context,fields.when);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_data_layout_declared(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_data_layout_parameter(address output,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset) requires nonzero(append){word(1,output,append);host.parameter(fields.parameter,output);host.stride(fields.stride,output);host.offset(fields.offset,output);}
extern "C" ReadStatus contract_read_data_layout_parameter(address context,ContractDataLayoutParameterFields fields,ContractDataLayoutParameterReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.stride) requires nonzero(host.offset){ReadStatus s0=host.parameter(context,fields.parameter);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.stride(context,fields.stride);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.offset(context,fields.offset);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_returned_unit(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_returned_value(address output,ContractReturnedValueFields fields,ContractReturnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append){word(1,output,append);host.value(fields.value,output);}
extern "C" ReadStatus contract_read_returned_value(address context,ContractReturnedValueFields fields,ContractReturnedValueReader host) on cpu requires nonzero(host.value){ReadStatus s0=host.value(context,fields.value);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_returned_dataset(address output,ContractReturnedDatasetFields fields,ContractReturnedDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.data) requires nonzero(append){word(2,output,append);host.data(fields.data,output);}
extern "C" ReadStatus contract_read_returned_dataset(address context,ContractReturnedDatasetFields fields,ContractReturnedDatasetReader host) on cpu requires nonzero(host.data){ReadStatus s0=host.data(context,fields.data);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_returned_owned_value(address output,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.value) requires nonzero(append){word(3,output,append);host.value(fields.value,output);}
extern "C" ReadStatus contract_read_returned_owned_value(address context,ContractReturnedOwnedValueFields fields,ContractReturnedOwnedValueReader host) on cpu requires nonzero(host.value){ReadStatus s0=host.value(context,fields.value);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_owned_value_payload(address output,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadWriter host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership){host.value(fields.value,output);host.payloads(fields.payloads,output);host.ownership(fields.ownership,output);}
extern "C" ReadStatus contract_read_owned_value_payload(address context,ContractOwnedValuePayloadFields fields,ContractOwnedValuePayloadReader host) on cpu requires nonzero(host.value) requires nonzero(host.payloads) requires nonzero(host.ownership){ReadStatus s0=host.value(context,fields.value);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.payloads(context,fields.payloads);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.ownership(context,fields.ownership);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_task_guard(address output,ContractTaskGuardFields fields,ContractTaskGuardWriter host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs){host.target(fields.target,output);host.ty(fields.ty,output);host.clauses(fields.clauses,output);host.result(fields.result,output);host.model(fields.model,output);host.arguments(fields.arguments,output);host.returned(fields.returned,output);host.inputs(fields.inputs,output);}
extern "C" ReadStatus contract_read_task_guard(address context,ContractTaskGuardFields fields,ContractTaskGuardReader host) on cpu requires nonzero(host.target) requires nonzero(host.ty) requires nonzero(host.clauses) requires nonzero(host.result) requires nonzero(host.model) requires nonzero(host.arguments) requires nonzero(host.returned) requires nonzero(host.inputs){ReadStatus s0=host.target(context,fields.target);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.ty(context,fields.ty);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.clauses(context,fields.clauses);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.result(context,fields.result);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.model(context,fields.model);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.arguments(context,fields.arguments);if(s5!=ReadStatus::Success){return s5;}ReadStatus s6=host.returned(context,fields.returned);if(s6!=ReadStatus::Success){return s6;}ReadStatus s7=host.inputs(context,fields.inputs);if(s7!=ReadStatus::Success){return s7;}return ReadStatus::Success;}
extern "C" void contract_write_task_completion(address output,ContractTaskCompletionFields fields,ContractTaskCompletionWriter host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent){host.offset(fields.offset,output);host.result(fields.result,output);host.layout(fields.layout,output);host.task(fields.task,output);host.parent(fields.parent,output);}
extern "C" ReadStatus contract_read_task_completion(address context,ContractTaskCompletionFields fields,ContractTaskCompletionReader host) on cpu requires nonzero(host.offset) requires nonzero(host.result) requires nonzero(host.layout) requires nonzero(host.task) requires nonzero(host.parent){ReadStatus s0=host.offset(context,fields.offset);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.result(context,fields.result);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.layout(context,fields.layout);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.task(context,fields.task);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.parent(context,fields.parent);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}
extern "C" void contract_write_task_projection_returned(address output,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.component) requires nonzero(host.source) requires nonzero(append){word(0,output,append);host.component(fields.component,output);host.source(fields.source,output);}
extern "C" ReadStatus contract_read_task_projection_returned(address context,ContractTaskProjectionReturnedFields fields,ContractTaskProjectionReturnedReader host) on cpu requires nonzero(host.component) requires nonzero(host.source){ReadStatus s0=host.component(context,fields.component);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.source(context,fields.source);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_task_projection_writes(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}
extern "C" void contract_write_task_projection_places(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}
extern "C" void contract_write_task_projection_initialized(address output,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.returned) requires nonzero(append){word(3,output,append);host.returned(fields.returned,output);}
extern "C" ReadStatus contract_read_task_projection_initialized(address context,ContractTaskProjectionInitializedFields fields,ContractTaskProjectionInitializedReader host) on cpu requires nonzero(host.returned){ReadStatus s0=host.returned(context,fields.returned);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_exclusion_resource(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_exclusion_ownership(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}
extern "C" void contract_write_exclusion_position(address output,ContractExclusionPositionFields fields,ContractExclusionPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.access) requires nonzero(append){word(2,output,append);host.access(fields.access,output);}
extern "C" ReadStatus contract_read_exclusion_position(address context,ContractExclusionPositionFields fields,ContractExclusionPositionReader host) on cpu requires nonzero(host.access){ReadStatus s0=host.access(context,fields.access);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_exclusion_view(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}
extern "C" void contract_write_position_access(address output,ContractPositionAccessFields fields,ContractPositionAccessWriter host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized){host.layout(fields.layout,output);host.written(fields.written,output);host.writable(fields.writable,output);host.initialized(fields.initialized,output);}
extern "C" ReadStatus contract_read_position_access(address context,ContractPositionAccessFields fields,ContractPositionAccessReader host) on cpu requires nonzero(host.layout) requires nonzero(host.written) requires nonzero(host.writable) requires nonzero(host.initialized){ReadStatus s0=host.layout(context,fields.layout);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.written(context,fields.written);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.writable(context,fields.writable);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.initialized(context,fields.initialized);if(s3!=ReadStatus::Success){return s3;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_resource(address output,ContractViewAccessResourceFields fields,ContractViewAccessResourceWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when) requires nonzero(append){word(0,output,append);host.resources(fields.resources,output);host.borrows(fields.borrows,output);host.kind(fields.kind,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_view_access_resource(address context,ContractViewAccessResourceFields fields,ContractViewAccessResourceReader host) on cpu requires nonzero(host.resources) requires nonzero(host.borrows) requires nonzero(host.kind) requires nonzero(host.when){ReadStatus s0=host.resources(context,fields.resources);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.borrows(context,fields.borrows);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.kind(context,fields.kind);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.when(context,fields.when);if(s3!=ReadStatus::Success){return s3;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_position(address output,ContractViewAccessPositionFields fields,ContractViewAccessPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position) requires nonzero(append){word(1,output,append);host.borrows(fields.borrows,output);host.when(fields.when,output);host.position(fields.position,output);}
extern "C" ReadStatus contract_read_view_access_position(address context,ContractViewAccessPositionFields fields,ContractViewAccessPositionReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position){ReadStatus s0=host.borrows(context,fields.borrows);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.when(context,fields.when);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.position(context,fields.position);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_memory(address output,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access) requires nonzero(append){word(2,output,append);host.borrows(fields.borrows,output);host.when(fields.when,output);host.access(fields.access,output);}
extern "C" ReadStatus contract_read_view_access_memory(address context,ContractViewAccessMemoryFields fields,ContractViewAccessMemoryReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.access){ReadStatus s0=host.borrows(context,fields.borrows);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.when(context,fields.when);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.access(context,fields.access);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_dataset(address output,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic) requires nonzero(append){word(3,output,append);host.borrows(fields.borrows,output);host.when(fields.when,output);host.index(fields.index,output);host.access(fields.access,output);host.atomic(fields.atomic,output);}
extern "C" ReadStatus contract_read_view_access_dataset(address context,ContractViewAccessDatasetFields fields,ContractViewAccessDatasetReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.index) requires nonzero(host.access) requires nonzero(host.atomic){ReadStatus s0=host.borrows(context,fields.borrows);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.when(context,fields.when);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.index(context,fields.index);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.access(context,fields.access);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.atomic(context,fields.atomic);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_initialized(address output,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when) requires nonzero(append){word(4,output,append);host.borrows(fields.borrows,output);host.initialized(fields.initialized,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_view_access_initialized(address context,ContractViewAccessInitializedFields fields,ContractViewAccessInitializedReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.initialized) requires nonzero(host.when){ReadStatus s0=host.borrows(context,fields.borrows);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.initialized(context,fields.initialized);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.when(context,fields.when);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_view_access_stored(address output,ContractViewAccessStoredFields fields,ContractViewAccessStoredWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when) requires nonzero(append){word(5,output,append);host.borrows(fields.borrows,output);host.word(fields.word,output);host.value(fields.value,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_view_access_stored(address context,ContractViewAccessStoredFields fields,ContractViewAccessStoredReader host) on cpu requires nonzero(host.borrows) requires nonzero(host.word) requires nonzero(host.value) requires nonzero(host.when){ReadStatus s0=host.borrows(context,fields.borrows);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.word(context,fields.word);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.value(context,fields.value);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.when(context,fields.when);if(s3!=ReadStatus::Success){return s3;}return ReadStatus::Success;}
extern "C" void contract_write_residency_part_field(address output,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.field) requires nonzero(append){word(0,output,append);host.field(fields.field,output);}
extern "C" ReadStatus contract_read_residency_part_field(address context,ContractResidencyPartFieldFields fields,ContractResidencyPartFieldReader host) on cpu requires nonzero(host.field){ReadStatus s0=host.field(context,fields.field);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_part_elements(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}
extern "C" void contract_write_residency_part_selected(address output,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.index) requires nonzero(append){word(2,output,append);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_residency_part_selected(address context,ContractResidencyPartSelectedFields fields,ContractResidencyPartSelectedReader host) on cpu requires nonzero(host.index){ReadStatus s0=host.index(context,fields.index);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_input(address output,ContractResidencyInputFields fields,ContractResidencyInputWriter host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields){host.parameter(fields.parameter,output);host.fields(fields.fields,output);}
extern "C" ReadStatus contract_read_residency_input(address context,ContractResidencyInputFields fields,ContractResidencyInputReader host) on cpu requires nonzero(host.parameter) requires nonzero(host.fields){ReadStatus s0=host.parameter(context,fields.parameter);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.fields(context,fields.fields);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_residency_origin_cpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_residency_origin_gpu(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}
extern "C" void contract_write_residency_origin_input(address output,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.input) requires nonzero(append){word(2,output,append);host.input(fields.input,output);}
extern "C" ReadStatus contract_read_residency_origin_input(address context,ContractResidencyOriginInputFields fields,ContractResidencyOriginInputReader host) on cpu requires nonzero(host.input){ReadStatus s0=host.input(context,fields.input);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_origin_output(address output,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.output) requires nonzero(append){word(3,output,append);host.output(fields.output,output);}
extern "C" ReadStatus contract_read_residency_origin_output(address context,ContractResidencyOriginOutputFields fields,ContractResidencyOriginOutputReader host) on cpu requires nonzero(host.output){ReadStatus s0=host.output(context,fields.output);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_origin_task_transfer(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(4,output,append);}
extern "C" void contract_write_residency_absent(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_residency_origins(address output,ContractResidencyOriginsFields fields,ContractResidencyOriginsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origins) requires nonzero(append){word(1,output,append);host.origins(fields.origins,output);}
extern "C" ReadStatus contract_read_residency_origins(address context,ContractResidencyOriginsFields fields,ContractResidencyOriginsReader host) on cpu requires nonzero(host.origins){ReadStatus s0=host.origins(context,fields.origins);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_selected(address output,ContractResidencySelectedFields fields,ContractResidencySelectedWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b) requires nonzero(append){word(2,output,append);host.when(fields.when,output);host.a(fields.a,output);host.b(fields.b,output);}
extern "C" ReadStatus contract_read_residency_selected(address context,ContractResidencySelectedFields fields,ContractResidencySelectedReader host) on cpu requires nonzero(host.when) requires nonzero(host.a) requires nonzero(host.b){ReadStatus s0=host.when(context,fields.when);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.a(context,fields.a);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.b(context,fields.b);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_residency_output_kind_returned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_residency_output_kind_written(address output,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parameter) requires nonzero(append){word(1,output,append);host.parameter(fields.parameter,output);}
extern "C" ReadStatus contract_read_residency_output_kind_written(address context,ContractResidencyOutputKindWrittenFields fields,ContractResidencyOutputKindWrittenReader host) on cpu requires nonzero(host.parameter){ReadStatus s0=host.parameter(context,fields.parameter);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_residency_output(address output,ContractResidencyOutputFields fields,ContractResidencyOutputWriter host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings){host.function(fields.function,output);host.kind(fields.kind,output);host.fields(fields.fields,output);host.arguments(fields.arguments,output);host.bindings(fields.bindings,output);}
extern "C" ReadStatus contract_read_residency_output(address context,ContractResidencyOutputFields fields,ContractResidencyOutputReader host) on cpu requires nonzero(host.function) requires nonzero(host.kind) requires nonzero(host.fields) requires nonzero(host.arguments) requires nonzero(host.bindings){ReadStatus s0=host.function(context,fields.function);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.kind(context,fields.kind);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.fields(context,fields.fields);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.arguments(context,fields.arguments);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.bindings(context,fields.bindings);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}
extern "C" void contract_write_domain_output(address output,ContractDomainOutputFields fields,ContractDomainOutputWriter host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words){host.binding(fields.binding,output);host.domain(fields.domain,output);host.words(fields.words,output);}
extern "C" ReadStatus contract_read_domain_output(address context,ContractDomainOutputFields fields,ContractDomainOutputReader host) on cpu requires nonzero(host.binding) requires nonzero(host.domain) requires nonzero(host.words){ReadStatus s0=host.binding(context,fields.binding);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.domain(context,fields.domain);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.words(context,fields.words);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_domain_interface(address output,ContractDomainInterfaceFields fields,ContractDomainInterfaceWriter host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements){host.instance(fields.instance,output);host.target(fields.target,output);host.buffers(fields.buffers,output);host.textures(fields.textures,output);host.write_regions(fields.write_regions,output);host.uniforms(fields.uniforms,output);host.length_requirements(fields.length_requirements,output);host.element_requirements(fields.element_requirements,output);host.prefix_guarantees(fields.prefix_guarantees,output);host.outputs(fields.outputs,output);host.elements(fields.elements,output);}
extern "C" ReadStatus contract_read_domain_interface(address context,ContractDomainInterfaceFields fields,ContractDomainInterfaceReader host) on cpu requires nonzero(host.instance) requires nonzero(host.target) requires nonzero(host.buffers) requires nonzero(host.textures) requires nonzero(host.write_regions) requires nonzero(host.uniforms) requires nonzero(host.length_requirements) requires nonzero(host.element_requirements) requires nonzero(host.prefix_guarantees) requires nonzero(host.outputs) requires nonzero(host.elements){ReadStatus s0=host.instance(context,fields.instance);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.target(context,fields.target);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.buffers(context,fields.buffers);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.textures(context,fields.textures);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.write_regions(context,fields.write_regions);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.uniforms(context,fields.uniforms);if(s5!=ReadStatus::Success){return s5;}ReadStatus s6=host.length_requirements(context,fields.length_requirements);if(s6!=ReadStatus::Success){return s6;}ReadStatus s7=host.element_requirements(context,fields.element_requirements);if(s7!=ReadStatus::Success){return s7;}ReadStatus s8=host.prefix_guarantees(context,fields.prefix_guarantees);if(s8!=ReadStatus::Success){return s8;}ReadStatus s9=host.outputs(context,fields.outputs);if(s9!=ReadStatus::Success){return s9;}ReadStatus s10=host.elements(context,fields.elements);if(s10!=ReadStatus::Success){return s10;}return ReadStatus::Success;}
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
extern "C" void contract_write_relation_op_identity(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_relation_op_position(address output,ContractRelationOpPositionFields fields,ContractRelationOpPositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append){word(1,output,append);host.origin(fields.origin,output);}
extern "C" ReadStatus contract_read_relation_op_position(address context,ContractRelationOpPositionFields fields,ContractRelationOpPositionReader host) on cpu requires nonzero(host.origin){ReadStatus s0=host.origin(context,fields.origin);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_relation_op_read(address output,ContractRelationOpReadFields fields,ContractRelationOpReadWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.word) requires nonzero(host.frame) requires nonzero(append){word(2,output,append);host.word(fields.word,output);host.frame(fields.frame,output);}
extern "C" ReadStatus contract_read_relation_op_read(address context,ContractRelationOpReadFields fields,ContractRelationOpReadReader host) on cpu requires nonzero(host.word) requires nonzero(host.frame){ReadStatus s0=host.word(context,fields.word);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.frame(context,fields.frame);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_relation_op_element(address output,ContractRelationOpElementFields fields,ContractRelationOpElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append){word(3,output,append);host.fields(fields.fields,output);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_relation_op_element(address context,ContractRelationOpElementFields fields,ContractRelationOpElementReader host) on cpu requires nonzero(host.fields) requires nonzero(host.index){ReadStatus s0=host.fields(context,fields.fields);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.index(context,fields.index);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_relation_op_projection(address output,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.fields) requires nonzero(append){word(4,output,append);host.fields(fields.fields,output);}
extern "C" ReadStatus contract_read_relation_op_projection(address context,ContractRelationOpProjectionFields fields,ContractRelationOpProjectionReader host) on cpu requires nonzero(host.fields){ReadStatus s0=host.fields(context,fields.fields);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_relation_op_resources(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(5,output,append);}
extern "C" void contract_write_relation_op_owners(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(6,output,append);}
extern "C" void contract_write_relation_op_select(address output,ContractRelationOpSelectFields fields,ContractRelationOpSelectWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.indices) requires nonzero(append){word(7,output,append);host.indices(fields.indices,output);}
extern "C" ReadStatus contract_read_relation_op_select(address context,ContractRelationOpSelectFields fields,ContractRelationOpSelectReader host) on cpu requires nonzero(host.indices){ReadStatus s0=host.indices(context,fields.indices);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_relation_op_initial(address output,ContractRelationOpInitialFields fields,ContractRelationOpInitialWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.frame) requires nonzero(append){word(8,output,append);host.origin(fields.origin,output);host.frame(fields.frame,output);}
extern "C" ReadStatus contract_read_relation_op_initial(address context,ContractRelationOpInitialFields fields,ContractRelationOpInitialReader host) on cpu requires nonzero(host.origin) requires nonzero(host.frame){ReadStatus s0=host.origin(context,fields.origin);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.frame(context,fields.frame);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_relation_edge(address output,ContractRelationEdgeFields fields,ContractRelationEdgeWriter host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when){host.from(fields.from,output);host.op(fields.op,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_relation_edge(address context,ContractRelationEdgeFields fields,ContractRelationEdgeReader host) on cpu requires nonzero(host.from) requires nonzero(host.op) requires nonzero(host.when){ReadStatus s0=host.from(context,fields.from);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.op(context,fields.op);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.when(context,fields.when);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_relation_node(address output,ContractRelationNodeFields fields,ContractRelationNodeWriter host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges){host.seeds(fields.seeds,output);host.edges(fields.edges,output);}
extern "C" ReadStatus contract_read_relation_node(address context,ContractRelationNodeFields fields,ContractRelationNodeReader host) on cpu requires nonzero(host.seeds) requires nonzero(host.edges){ReadStatus s0=host.seeds(context,fields.seeds);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.edges(context,fields.edges);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_relation_write(address output,ContractRelationWriteFields fields,ContractRelationWriteWriter host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when){host.target(fields.target,output);host.fields(fields.fields,output);host.when(fields.when,output);}
extern "C" ReadStatus contract_read_relation_write(address context,ContractRelationWriteFields fields,ContractRelationWriteReader host) on cpu requires nonzero(host.target) requires nonzero(host.fields) requires nonzero(host.when){ReadStatus s0=host.target(context,fields.target);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.fields(context,fields.fields);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.when(context,fields.when);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_relation_frame(address output,ContractRelationFrameFields fields,ContractRelationFrameWriter host) on cpu requires nonzero(host.parent) requires nonzero(host.writes){host.parent(fields.parent,output);host.writes(fields.writes,output);}
extern "C" ReadStatus contract_read_relation_frame(address context,ContractRelationFrameFields fields,ContractRelationFrameReader host) on cpu requires nonzero(host.parent) requires nonzero(host.writes){ReadStatus s0=host.parent(context,fields.parent);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.writes(context,fields.writes);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_relation_graph(address output,ContractRelationGraphFields fields,ContractRelationGraphWriter host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames){host.nodes(fields.nodes,output);host.variables(fields.variables,output);host.frames(fields.frames,output);}
extern "C" ReadStatus contract_read_relation_graph(address context,ContractRelationGraphFields fields,ContractRelationGraphReader host) on cpu requires nonzero(host.nodes) requires nonzero(host.variables) requires nonzero(host.frames){ReadStatus s0=host.nodes(context,fields.nodes);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.variables(context,fields.variables);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.frames(context,fields.frames);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
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
extern "C" void contract_write_ownership_owned(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(0,output,append);}
extern "C" void contract_write_ownership_borrowed(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(1,output,append);}
extern "C" void contract_write_ownership_moved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}
extern "C" void contract_write_address_identity_constant(address output,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.bits) requires nonzero(append){byte(u8(0),output,append);host.bits(fields.bits,output);}
extern "C" ReadStatus contract_read_address_identity_constant(address context,ContractAddressIdentityConstantFields fields,ContractAddressIdentityConstantReader host) on cpu requires nonzero(host.bits){ReadStatus s0=host.bits(context,fields.bits);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_identity_value(address output,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.ids) requires nonzero(append){byte(u8(1),output,append);host.ids(fields.ids,output);}
extern "C" ReadStatus contract_read_address_identity_value(address context,ContractAddressIdentityValueFields fields,ContractAddressIdentityValueReader host) on cpu requires nonzero(host.ids){ReadStatus s0=host.ids(context,fields.ids);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_identity_storage(address output,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.borrow) requires nonzero(append){byte(u8(2),output,append);host.borrow(fields.borrow,output);}
extern "C" ReadStatus contract_read_address_identity_storage(address context,ContractAddressIdentityStorageFields fields,ContractAddressIdentityStorageReader host) on cpu requires nonzero(host.borrow){ReadStatus s0=host.borrow(context,fields.borrow);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_address_origin(address output,ContractAddressOriginFields fields,ContractAddressOriginWriter host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized){host.root(fields.root,output);host.layout(fields.layout,output);host.base(fields.base,output);host.words(fields.words,output);host.writable(fields.writable,output);host.initialized(fields.initialized,output);}
extern "C" ReadStatus contract_read_address_origin(address context,ContractAddressOriginFields fields,ContractAddressOriginReader host) on cpu requires nonzero(host.root) requires nonzero(host.layout) requires nonzero(host.base) requires nonzero(host.words) requires nonzero(host.writable) requires nonzero(host.initialized){ReadStatus s0=host.root(context,fields.root);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.layout(context,fields.layout);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.base(context,fields.base);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.words(context,fields.words);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.writable(context,fields.writable);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.initialized(context,fields.initialized);if(s5!=ReadStatus::Success){return s5;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_data(address output,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(append){word(0,output,append);host.id(fields.id,output);}
extern "C" ReadStatus contract_read_borrow_source_data(address context,ContractBorrowSourceDataFields fields,ContractBorrowSourceDataReader host) on cpu requires nonzero(host.id){ReadStatus s0=host.id(context,fields.id);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_parameter(address output,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.id) requires nonzero(host.indices) requires nonzero(append){word(1,output,append);host.id(fields.id,output);host.indices(fields.indices,output);}
extern "C" ReadStatus contract_read_borrow_source_parameter(address context,ContractBorrowSourceParameterFields fields,ContractBorrowSourceParameterReader host) on cpu requires nonzero(host.id) requires nonzero(host.indices){ReadStatus s0=host.id(context,fields.id);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.indices(context,fields.indices);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_unresolved(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(2,output,append);}
extern "C" void contract_write_borrow_source_inactive(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(3,output,append);}
extern "C" void contract_write_borrow_source_external(address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append){word(4,output,append);}
extern "C" void contract_write_borrow_source_position(address output,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(append){word(5,output,append);host.origin(fields.origin,output);}
extern "C" ReadStatus contract_read_borrow_source_position(address context,ContractBorrowSourcePositionFields fields,ContractBorrowSourcePositionReader host) on cpu requires nonzero(host.origin){ReadStatus s0=host.origin(context,fields.origin);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_element(address output,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index) requires nonzero(append){word(6,output,append);host.parent(fields.parent,output);host.fields(fields.fields,output);host.index(fields.index,output);}
extern "C" ReadStatus contract_read_borrow_source_element(address context,ContractBorrowSourceElementFields fields,ContractBorrowSourceElementReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(host.index){ReadStatus s0=host.parent(context,fields.parent);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.fields(context,fields.fields);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.index(context,fields.index);if(s2!=ReadStatus::Success){return s2;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_projection(address output,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.parent) requires nonzero(host.fields) requires nonzero(append){word(7,output,append);host.parent(fields.parent,output);host.fields(fields.fields,output);}
extern "C" ReadStatus contract_read_borrow_source_projection(address context,ContractBorrowSourceProjectionFields fields,ContractBorrowSourceProjectionReader host) on cpu requires nonzero(host.parent) requires nonzero(host.fields){ReadStatus s0=host.parent(context,fields.parent);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.fields(context,fields.fields);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_contents(address output,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.origin) requires nonzero(host.word) requires nonzero(append){word(8,output,append);host.origin(fields.origin,output);host.word(fields.word,output);}
extern "C" ReadStatus contract_read_borrow_source_contents(address context,ContractBorrowSourceContentsFields fields,ContractBorrowSourceContentsReader host) on cpu requires nonzero(host.origin) requires nonzero(host.word){ReadStatus s0=host.origin(context,fields.origin);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.word(context,fields.word);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_relation(address output,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.graph) requires nonzero(append){word(9,output,append);host.graph(fields.graph,output);}
extern "C" ReadStatus contract_read_borrow_source_relation(address context,ContractBorrowSourceRelationFields fields,ContractBorrowSourceRelationReader host) on cpu requires nonzero(host.graph){ReadStatus s0=host.graph(context,fields.graph);if(s0!=ReadStatus::Success){return s0;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_task(address output,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions) requires nonzero(append){word(10,output,append);host.captures(fields.captures,output);host.outputs(fields.outputs,output);host.initialized(fields.initialized,output);host.guards(fields.guards,output);host.completions(fields.completions,output);}
extern "C" ReadStatus contract_read_borrow_source_task(address context,ContractBorrowSourceTaskFields fields,ContractBorrowSourceTaskReader host) on cpu requires nonzero(host.captures) requires nonzero(host.outputs) requires nonzero(host.initialized) requires nonzero(host.guards) requires nonzero(host.completions){ReadStatus s0=host.captures(context,fields.captures);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.outputs(context,fields.outputs);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.initialized(context,fields.initialized);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.guards(context,fields.guards);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.completions(context,fields.completions);if(s4!=ReadStatus::Success){return s4;}return ReadStatus::Success;}
extern "C" void contract_write_borrow_source_task_output(address output,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputWriter host,cfn<void,address,address,u64> append) on cpu requires nonzero(host.task) requires nonzero(host.projection) requires nonzero(append){word(11,output,append);host.task(fields.task,output);host.projection(fields.projection,output);}
extern "C" ReadStatus contract_read_borrow_source_task_output(address context,ContractBorrowSourceTaskOutputFields fields,ContractBorrowSourceTaskOutputReader host) on cpu requires nonzero(host.task) requires nonzero(host.projection){ReadStatus s0=host.task(context,fields.task);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.projection(context,fields.projection);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_residency_contract(address output,ContractResidencyContractFields fields,ContractResidencyContractWriter host) on cpu requires nonzero(host.required) requires nonzero(host.calls){host.required(fields.required,output);host.calls(fields.calls,output);}
extern "C" ReadStatus contract_read_residency_contract(address context,ContractResidencyContractFields fields,ContractResidencyContractReader host) on cpu requires nonzero(host.required) requires nonzero(host.calls){ReadStatus s0=host.required(context,fields.required);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.calls(context,fields.calls);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" void contract_write_residency_call(address output,ContractResidencyCallFields fields,ContractResidencyCallWriter host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path){host.function(fields.function,output);host.arguments(fields.arguments,output);host.bindings(fields.bindings,output);host.path(fields.path,output);}
extern "C" ReadStatus contract_read_residency_call(address context,ContractResidencyCallFields fields,ContractResidencyCallReader host) on cpu requires nonzero(host.function) requires nonzero(host.arguments) requires nonzero(host.bindings) requires nonzero(host.path){ReadStatus s0=host.function(context,fields.function);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.arguments(context,fields.arguments);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.bindings(context,fields.bindings);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.path(context,fields.path);if(s3!=ReadStatus::Success){return s3;}return ReadStatus::Success;}
extern "C" void contract_write_ordinary_contract(address output,ContractOrdinaryContractFields fields,ContractOrdinaryContractWriter host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization){host.parameters(fields.parameters,output);host.result(fields.result,output);host.inputs(fields.inputs,output);host.element_inputs(fields.element_inputs,output);host.returned(fields.returned,output);host.written(fields.written,output);host.requires(fields.requires,output);host.target(fields.target,output);host.frame(fields.frame,output);host.fallible(fields.fallible,output);host.domain(fields.domain,output);host.residency(fields.residency,output);host.view_access(fields.view_access,output);host.initialization(fields.initialization,output);}
extern "C" ReadStatus contract_read_ordinary_contract(address context,ContractOrdinaryContractFields fields,ContractOrdinaryContractReader host) on cpu requires nonzero(host.parameters) requires nonzero(host.result) requires nonzero(host.inputs) requires nonzero(host.element_inputs) requires nonzero(host.returned) requires nonzero(host.written) requires nonzero(host.requires) requires nonzero(host.target) requires nonzero(host.frame) requires nonzero(host.fallible) requires nonzero(host.domain) requires nonzero(host.residency) requires nonzero(host.view_access) requires nonzero(host.initialization){ReadStatus s0=host.parameters(context,fields.parameters);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.result(context,fields.result);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.inputs(context,fields.inputs);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.element_inputs(context,fields.element_inputs);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.returned(context,fields.returned);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.written(context,fields.written);if(s5!=ReadStatus::Success){return s5;}ReadStatus s6=host.requires(context,fields.requires);if(s6!=ReadStatus::Success){return s6;}ReadStatus s7=host.target(context,fields.target);if(s7!=ReadStatus::Success){return s7;}ReadStatus s8=host.frame(context,fields.frame);if(s8!=ReadStatus::Success){return s8;}ReadStatus s9=host.fallible(context,fields.fallible);if(s9!=ReadStatus::Success){return s9;}ReadStatus s10=host.domain(context,fields.domain);if(s10!=ReadStatus::Success){return s10;}ReadStatus s11=host.residency(context,fields.residency);if(s11!=ReadStatus::Success){return s11;}ReadStatus s12=host.view_access(context,fields.view_access);if(s12!=ReadStatus::Success){return s12;}ReadStatus s13=host.initialization(context,fields.initialization);if(s13!=ReadStatus::Success){return s13;}return ReadStatus::Success;}
extern "C" void contract_write_proof(address output,ContractProofFields fields,ContractProofWriter host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses){host.origins(fields.origins,output);host.conditions(fields.conditions,output);host.ranges(fields.ranges,output);host.nonzero(fields.nonzero,output);host.domains(fields.domains,output);host.views(fields.views,output);host.addresses(fields.addresses,output);}
extern "C" ReadStatus contract_read_proof(address context,ContractProofFields fields,ContractProofReader host) on cpu requires nonzero(host.origins) requires nonzero(host.conditions) requires nonzero(host.ranges) requires nonzero(host.nonzero) requires nonzero(host.domains) requires nonzero(host.views) requires nonzero(host.addresses){ReadStatus s0=host.origins(context,fields.origins);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.conditions(context,fields.conditions);if(s1!=ReadStatus::Success){return s1;}ReadStatus s2=host.ranges(context,fields.ranges);if(s2!=ReadStatus::Success){return s2;}ReadStatus s3=host.nonzero(context,fields.nonzero);if(s3!=ReadStatus::Success){return s3;}ReadStatus s4=host.domains(context,fields.domains);if(s4!=ReadStatus::Success){return s4;}ReadStatus s5=host.views(context,fields.views);if(s5!=ReadStatus::Success){return s5;}ReadStatus s6=host.addresses(context,fields.addresses);if(s6!=ReadStatus::Success){return s6;}return ReadStatus::Success;}
extern "C" void contract_write_pair(address output,ContractPairFields fields,ContractPairWriter host) on cpu requires nonzero(host.first) requires nonzero(host.second){host.first(fields.first,output);host.second(fields.second,output);}
extern "C" ReadStatus contract_read_pair(address context,ContractPairFields fields,ContractPairReader host) on cpu requires nonzero(host.first) requires nonzero(host.second){ReadStatus s0=host.first(context,fields.first);if(s0!=ReadStatus::Success){return s0;}ReadStatus s1=host.second(context,fields.second);if(s1!=ReadStatus::Success){return s1;}return ReadStatus::Success;}
extern "C" ReadStatus contract_read_address_root(Bytes input,inout u64 position,address context,address output,ContractAddressRootVariants host) on cpu requires nonzero(host.local) requires nonzero(host.parameter) requires nonzero(host.resource) requires nonzero(host.buffer) requires nonzero(host.derived) requires nonzero(host.external) requires nonzero(host.dataset){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.local(context,output);}if(tag.value==1u64){return host.parameter(context,output);}if(tag.value==2u64){return host.resource(context,output);}if(tag.value==3u64){return host.buffer(context,output);}if(tag.value==4u64){return host.derived(context,output);}if(tag.value==5u64){return host.external(context,output);}if(tag.value==6u64){return host.dataset(context,output);}return ReadStatus::ContractAddressRoot;}
extern "C" ReadStatus contract_read_initialization_source(Bytes input,inout u64 position,address context,address output,ContractInitializationSourceVariants host) on cpu requires nonzero(host.position) requires nonzero(host.task){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.position(context,output);}if(tag.value==1u64){return host.task(context,output);}return ReadStatus::ContractInitializationSource;}
extern "C" ReadStatus contract_read_element_facts(Bytes input,inout u64 position,address context,address output,ContractElementFactsVariants host) on cpu requires nonzero(host.unknown) requires nonzero(host.known) requires nonzero(host.cleanup) requires nonzero(host.common) requires nonzero(host.selected) requires nonzero(host.input){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.unknown(context,output);}if(tag.value==1u64){return host.known(context,output);}if(tag.value==2u64){return host.cleanup(context,output);}if(tag.value==3u64){return host.common(context,output);}if(tag.value==4u64){return host.selected(context,output);}if(tag.value==5u64){return host.input(context,output);}return ReadStatus::ContractElementFacts;}
extern "C" ReadStatus contract_read_data_layout(Bytes input,inout u64 position,address context,address output,ContractDataLayoutVariants host) on cpu requires nonzero(host.declared) requires nonzero(host.parameter){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.declared(context,output);}if(tag.value==1u64){return host.parameter(context,output);}return ReadStatus::ContractDataLayout;}
extern "C" ReadStatus contract_read_returned(Bytes input,inout u64 position,address context,address output,ContractReturnedVariants host) on cpu requires nonzero(host.unit) requires nonzero(host.value) requires nonzero(host.dataset) requires nonzero(host.owned_value){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.unit(context,output);}if(tag.value==1u64){return host.value(context,output);}if(tag.value==2u64){return host.dataset(context,output);}if(tag.value==3u64){return host.owned_value(context,output);}return ReadStatus::ContractReturned;}
extern "C" ReadStatus contract_read_task_projection(Bytes input,inout u64 position,address context,address output,ContractTaskProjectionVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.writes) requires nonzero(host.places) requires nonzero(host.initialized){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.returned(context,output);}if(tag.value==1u64){return host.writes(context,output);}if(tag.value==2u64){return host.places(context,output);}if(tag.value==3u64){return host.initialized(context,output);}return ReadStatus::ContractTaskProjection;}
extern "C" ReadStatus contract_read_exclusion(Bytes input,inout u64 position,address context,address output,ContractExclusionVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.ownership) requires nonzero(host.position) requires nonzero(host.view){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.resource(context,output);}if(tag.value==1u64){return host.ownership(context,output);}if(tag.value==2u64){return host.position(context,output);}if(tag.value==3u64){return host.view(context,output);}return ReadStatus::ContractExclusion;}
extern "C" ReadStatus contract_read_view_access(Bytes input,inout u64 position,address context,address output,ContractViewAccessVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.position) requires nonzero(host.memory) requires nonzero(host.dataset) requires nonzero(host.initialized) requires nonzero(host.stored){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.resource(context,output);}if(tag.value==1u64){return host.position(context,output);}if(tag.value==2u64){return host.memory(context,output);}if(tag.value==3u64){return host.dataset(context,output);}if(tag.value==4u64){return host.initialized(context,output);}if(tag.value==5u64){return host.stored(context,output);}return ReadStatus::ContractViewAccess;}
extern "C" ReadStatus contract_read_residency_part(Bytes input,inout u64 position,address context,address output,ContractResidencyPartVariants host) on cpu requires nonzero(host.field) requires nonzero(host.elements) requires nonzero(host.selected){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.field(context,output);}if(tag.value==1u64){return host.elements(context,output);}if(tag.value==2u64){return host.selected(context,output);}return ReadStatus::ContractResidencyPart;}
extern "C" ReadStatus contract_read_residency_origin(Bytes input,inout u64 position,address context,address output,ContractResidencyOriginVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) requires nonzero(host.input) requires nonzero(host.output) requires nonzero(host.task_transfer){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.cpu(context,output);}if(tag.value==1u64){return host.gpu(context,output);}if(tag.value==2u64){return host.input(context,output);}if(tag.value==3u64){return host.output(context,output);}if(tag.value==4u64){return host.task_transfer(context,output);}return ReadStatus::ContractResidencyOrigin;}
extern "C" ReadStatus contract_read_residency(Bytes input,inout u64 position,address context,address output,ContractResidencyVariants host) on cpu requires nonzero(host.absent) requires nonzero(host.origins) requires nonzero(host.selected){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.absent(context,output);}if(tag.value==1u64){return host.origins(context,output);}if(tag.value==2u64){return host.selected(context,output);}return ReadStatus::ContractResidency;}
extern "C" ReadStatus contract_read_residency_output_kind(Bytes input,inout u64 position,address context,address output,ContractResidencyOutputKindVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.written){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.returned(context,output);}if(tag.value==1u64){return host.written(context,output);}return ReadStatus::ContractResidencyOutputKind;}
extern "C" ReadStatus contract_read_texture_dimension(Bytes input,inout u64 position,address context,address output,ContractTextureDimensionVariants host) on cpu requires nonzero(host.d2) requires nonzero(host.d3){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==2u64){return host.d2(context,output);}if(tag.value==3u64){return host.d3(context,output);}return ReadStatus::ContractTextureDimension;}
extern "C" ReadStatus contract_read_texture_format(Bytes input,inout u64 position,address context,address output,ContractTextureFormatVariants host) on cpu requires nonzero(host.rgba32float) requires nonzero(host.rgba8unorm) requires nonzero(host.rgba8srgb) requires nonzero(host.r8unorm) requires nonzero(host.rg16unorm) requires nonzero(host.rgb10a2unorm) requires nonzero(host.r11g11b10float){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.rgba32float(context,output);}if(tag.value==1u64){return host.rgba8unorm(context,output);}if(tag.value==2u64){return host.rgba8srgb(context,output);}if(tag.value==3u64){return host.r8unorm(context,output);}if(tag.value==4u64){return host.rg16unorm(context,output);}if(tag.value==5u64){return host.rgb10a2unorm(context,output);}if(tag.value==6u64){return host.r11g11b10float(context,output);}return ReadStatus::ContractTextureFormat;}
extern "C" ReadStatus contract_read_element_fact(Bytes input,inout u64 position,address context,address output,ContractElementFactVariants host) on cpu requires nonzero(host.range) requires nonzero(host.index){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.range(context,output);}if(tag.value==1u64){return host.index(context,output);}return ReadStatus::ContractElementFact;}
extern "C" ReadStatus contract_read_access(Bytes input,inout u64 position,address context,address output,ContractAccessVariants host) on cpu requires nonzero(host.value) requires nonzero(host.read) requires nonzero(host.write) requires nonzero(host.inout) requires nonzero(host.atomic){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.value(context,output);}if(tag.value==1u64){return host.read(context,output);}if(tag.value==2u64){return host.write(context,output);}if(tag.value==3u64){return host.inout(context,output);}if(tag.value==4u64){return host.atomic(context,output);}return ReadStatus::Access;}
extern "C" ReadStatus contract_read_target(Bytes input,inout u64 position,address context,address output,ContractTargetVariants host) on cpu requires nonzero(host.compute) requires nonzero(host.cpu) requires nonzero(host.gpu){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.compute(context,output);}if(tag.value==1u64){return host.cpu(context,output);}if(tag.value==2u64){return host.gpu(context,output);}return ReadStatus::Target;}
extern "C" ReadStatus contract_read_call_parameter_kind(Bytes input,inout u64 position,address context,address output,ContractCallParameterKindVariants host) on cpu requires nonzero(host.value) requires nonzero(host.reference){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.value(context,output);}if(tag.value==1u64){return host.reference(context,output);}return ReadStatus::ContractCallParameterKind;}
extern "C" ReadStatus contract_read_contract_kind(Bytes input,inout u64 position,address context,address output,ContractContractKindVariants host) on cpu requires nonzero(host.requires) requires nonzero(host.ensures){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.requires(context,output);}if(tag.value==1u64){return host.ensures(context,output);}return ReadStatus::ContractContractKind;}
extern "C" ReadStatus contract_read_relation_op(Bytes input,inout u64 position,address context,address output,ContractRelationOpVariants host) on cpu requires nonzero(host.identity) requires nonzero(host.position) requires nonzero(host.read) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.resources) requires nonzero(host.owners) requires nonzero(host.select) requires nonzero(host.initial){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.identity(context,output);}if(tag.value==1u64){return host.position(context,output);}if(tag.value==2u64){return host.read(context,output);}if(tag.value==3u64){return host.element(context,output);}if(tag.value==4u64){return host.projection(context,output);}if(tag.value==5u64){return host.resources(context,output);}if(tag.value==6u64){return host.owners(context,output);}if(tag.value==7u64){return host.select(context,output);}if(tag.value==8u64){return host.initial(context,output);}return ReadStatus::ContractRelationOp;}
extern "C" ReadStatus contract_read_scalar(Bytes input,inout u64 position,address context,address output,ContractScalarVariants host) on cpu requires nonzero(host.f32) requires nonzero(host.u32) requires nonzero(host.i32) requires nonzero(host.bool) requires nonzero(host.u64) requires nonzero(host.i64) requires nonzero(host.f64) requires nonzero(host.u8) requires nonzero(host.i8) requires nonzero(host.u16) requires nonzero(host.i16){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.f32(context,output);}if(tag.value==1u64){return host.u32(context,output);}if(tag.value==2u64){return host.i32(context,output);}if(tag.value==3u64){return host.bool(context,output);}if(tag.value==4u64){return host.u64(context,output);}if(tag.value==5u64){return host.i64(context,output);}if(tag.value==6u64){return host.f64(context,output);}if(tag.value==7u64){return host.u8(context,output);}if(tag.value==8u64){return host.i8(context,output);}if(tag.value==9u64){return host.u16(context,output);}if(tag.value==10u64){return host.i16(context,output);}return ReadStatus::Scalar;}
extern "C" ReadStatus contract_read_ownership(Bytes input,inout u64 position,address context,address output,ContractOwnershipVariants host) on cpu requires nonzero(host.owned) requires nonzero(host.borrowed) requires nonzero(host.moved){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.owned(context,output);}if(tag.value==1u64){return host.borrowed(context,output);}if(tag.value==2u64){return host.moved(context,output);}return ReadStatus::ContractOwnership;}
extern "C" ReadStatus contract_read_address_identity(Bytes input,inout u64 position,address context,address output,ContractAddressIdentityVariants host) on cpu requires nonzero(host.constant) requires nonzero(host.value) requires nonzero(host.storage){ReadValue tag=fixed_value(input,position,1);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.constant(context,output);}if(tag.value==1u64){return host.value(context,output);}if(tag.value==2u64){return host.storage(context,output);}return ReadStatus::ContractAddressIdentity;}
extern "C" ReadStatus contract_read_borrow_source(Bytes input,inout u64 position,address context,address output,ContractBorrowSourceVariants host) on cpu requires nonzero(host.data) requires nonzero(host.parameter) requires nonzero(host.unresolved) requires nonzero(host.inactive) requires nonzero(host.external) requires nonzero(host.position) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.contents) requires nonzero(host.relation) requires nonzero(host.task) requires nonzero(host.task_output){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.data(context,output);}if(tag.value==1u64){return host.parameter(context,output);}if(tag.value==2u64){return host.unresolved(context,output);}if(tag.value==3u64){return host.inactive(context,output);}if(tag.value==4u64){return host.external(context,output);}if(tag.value==5u64){return host.position(context,output);}if(tag.value==6u64){return host.element(context,output);}if(tag.value==7u64){return host.projection(context,output);}if(tag.value==8u64){return host.contents(context,output);}if(tag.value==9u64){return host.relation(context,output);}if(tag.value==10u64){return host.task(context,output);}if(tag.value==11u64){return host.task_output(context,output);}return ReadStatus::ContractBorrowSource;}
extern "C" void contract_write_ast_type(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){write_type(source,output,host);}
extern "C" void contract_write_ast_predicate(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){predicate(source,output,host);}
extern "C" void contract_write_ast_condition(address source,address output,Host host) on cpu requires nonzero(host.append) requires nonzero(host.ty) requires nonzero(host.component) requires nonzero(host.field) requires nonzero(host.variant) requires nonzero(host.function) requires nonzero(host.parameter) requires nonzero(host.type_name) requires nonzero(host.argument) requires nonzero(host.shape) requires nonzero(host.expression) requires nonzero(host.predicate) requires nonzero(host.interval) requires nonzero(host.condition) requires nonzero(host.constant){condition(source,output,host);}
extern "C" ReadStatus contract_read_ast_condition(Bytes input,inout u64 position,address output,ReadHost host) on cpu requires nonzero(host.type_fields) requires nonzero(host.name_fields) requires nonzero(host.function_fields) requires nonzero(host.constant_fields) requires nonzero(host.text) requires nonzero(host.string_option) requires nonzero(host.push_u32) requires nonzero(host.push_u64) requires nonzero(host.push_component) requires nonzero(host.push_type) requires nonzero(host.push_field) requires nonzero(host.push_variant) requires nonzero(host.push_parameter) requires nonzero(host.push_interval) requires nonzero(host.push_argument) requires nonzero(host.push_expression) requires nonzero(host.push_predicate) requires nonzero(host.type_box) requires nonzero(host.type_name_box) requires nonzero(host.function_box) requires nonzero(host.shape_option) requires nonzero(host.predicates_option) requires nonzero(host.arguments_option) requires nonzero(host.shape_expression) requires nonzero(host.shape_literal) requires nonzero(host.shape_name) requires nonzero(host.expression) requires nonzero(host.condition) requires nonzero(host.predicate) requires nonzero(host.access) requires nonzero(host.target) requires nonzero(host.optional_access) requires nonzero(host.expression_span){return read_condition(input,position,output,host);}

// Same resource bytes; field order and boolean decoding belong to this B format.
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
