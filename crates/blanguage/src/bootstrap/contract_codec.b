module bootstrap.contract_codec;
import codec.builtin_type;

void numeric_write_scalar(BuiltinScalar value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    word(u32(value),output,append);
}

ReadStatus numeric_read_scalar(Bytes input,inout u64 position,inout BuiltinScalar output) on cpu {ReadScalar value=read_scalar(input,position);if(value.status!=ReadStatus::Success){return value.status;}output=value.value;return ReadStatus::Success;}

void numeric_write_identity(NumericIdentity value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
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

ReadStatus numeric_read_identity(Bytes input,inout u64 position,inout NumericIdentity output) on cpu {
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

void numeric_write_operand(NumericOperand value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
if(value.kind==OperandKind::Value){word(0,output,append);
numeric_write_identity(value.identity,output,append);
}
else if(value.kind==OperandKind::Constant){word(1,output,append);
numeric_write_scalar(value.scalar,output,append);
wide(value.bits,output,append);
}
}

ReadStatus numeric_read_operand_payload(u64 tag,Bytes input,inout u64 position,inout NumericOperand output) on cpu {
if(tag==0u64){
output.kind=OperandKind::Value;
ReadStatus status_26=numeric_read_identity(input,position,output.identity);if(status_26!=ReadStatus::Success){return status_26;}
return ReadStatus::Success;}
else if(tag==1u64){
output.kind=OperandKind::Constant;
ReadStatus status_27=numeric_read_scalar(input,position,output.scalar);if(status_27!=ReadStatus::Success){return status_27;}
ReadValue part_28=fixed_value(input,position,8);if(part_28.status!=ReadStatus::Success){return part_28.status;}output.bits=part_28.value;
return ReadStatus::Success;}
return ReadStatus::Operand;
}

ReadStatus numeric_read_operand(Bytes input,inout u64 position,inout NumericOperand output) on cpu {
 ReadValue tag=fixed_value(input,position,4);
 if(tag.status!=ReadStatus::Success){return tag.status;}
 return numeric_read_operand_payload(tag.value,input,position,output);
}

struct OperandSelector { u32 kind; OperandKey operand; };
struct AffineSelector { u32 kind; IdentityKey base; u64 padding; array<u64,4> coefficients; };
struct BinaryExpression { u32 form; u8 operation; u8 scalar; OperandKey left; OperandKey right; };
struct SelectionExpression { u32 form; array<u64,10> selection; };
struct FloatingKey { IdentityKey source; array<u64,5> bounds; u8 scalar; u8 form; };

NumericOperand empty_operand() on cpu {
    return NumericOperand(OperandKind::Value,empty_key_identity(),BuiltinScalar::F32,0u64);
}

ReadValue read_numeric_tag(Bytes input,inout u64 position,u64 limit,ReadStatus invalid) on cpu {
    ReadValue tag = fixed_value(input,position,4);
    if (tag.status == ReadStatus::Success && tag.value > limit) { tag.status = invalid; }
    return tag;
}

extern "C" void numeric_write_identity_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_identity(capture_key_identity(source),output,append);
}

extern "C" ReadStatus numeric_read_identity_key(Bytes input,inout u64 position,address output) on cpu {
    return read_domain_identity(input,position,output);
}

extern "C" void numeric_write_operand_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_operand(capture_key_operand(source),output,append);
}

extern "C" ReadStatus numeric_read_operand_key(Bytes input,inout u64 position,address output) on cpu {
    NumericOperand value = empty_operand();
    ReadStatus status = numeric_read_operand(input,position,value);
    if (status == ReadStatus::Success) {
        OperandKey key = make_key_operand(value);
        RtlMoveMemory(output,address_of(key),40u64);
    }
    return status;
}

extern "C" void numeric_write_selection_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    u32 kind = unsafe_position<u32>(source);
    address operand = domain_at(source,8u64);
    if (kind == 0) {
        numeric_write_operand_key(operand,output,append);
        return;
    }
    word(kind+1,output,append);
    if (kind == 2) {
        NumericOperand value = capture_key_operand(operand);
        numeric_write_identity(value.identity,output,append);
        wide(value.kind == OperandKind::Value ? 1u64 : 0u64,output,append);
        wide(0u64,output,append);
        wide(value.bits,output,append);
        wide(0u64,output,append);
        return;
    }
    numeric_write_identity_key(operand,output,append);
    wide(unsafe_position<u64>(domain_at(source,48u64)),output,append);
    wide(unsafe_position<u64>(domain_at(source,56u64)),output,append);
    wide(unsafe_position<u64>(domain_at(source,64u64)),output,append);
    wide(unsafe_position<u64>(domain_at(source,72u64)),output,append);
}

extern "C" ReadStatus numeric_read_selection_key(Bytes input,inout u64 position,address output) on cpu {
    ReadValue tag = fixed_value(input,position,4);
    if (tag.status != ReadStatus::Success) { return tag.status; }
    if ((tag.value >> 1u64) != 1u64) {
        NumericOperand value = empty_operand();
        ReadStatus status = numeric_read_operand_payload(tag.value,input,position,value);
        if (status == ReadStatus::Success) {
            OperandSelector key = OperandSelector(0,make_key_operand(value));
            RtlMoveMemory(output,address_of(key),48u64);
        }
        return status;
    }
    NumericIdentity base = empty_key_identity();
    ReadStatus status = numeric_read_identity(input,position,base);
    if (status != ReadStatus::Success) { return status; }
    array<u64,4> terms = array<u64,4>(0u64,0u64,0u64,0u64);
    u32 index = 0;
    while (index < 4) {
        ReadValue part = fixed_value(input,position,8);
        if (part.status != ReadStatus::Success) { return part.status; }
        terms[index] = part.value;
        index += 1;
    }
    if (tag.value == 3u64) {
        NumericOperand value = empty_operand();
        if (terms[0] == 0u64 && terms[1] == 0u64) {
            value.kind = OperandKind::Constant;
            value.scalar = BuiltinScalar::U32;
            value.bits = terms[2];
        } else {
            value.identity = base;
        }
        OperandSelector key = OperandSelector(2,make_key_operand(value));
        RtlMoveMemory(output,address_of(key),48u64);
    } else {
        AffineSelector key = AffineSelector(1,make_key_identity(base),0u64,terms);
        RtlMoveMemory(output,address_of(key),80u64);
    }
    return ReadStatus::Success;
}

extern "C" void numeric_write_integer_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    word(u32(unsafe_position<u8>(source)),output,append);
}

ReadStatus read_numeric_byte(Bytes input,inout u64 position,address output,u64 limit,ReadStatus invalid) on cpu {
    ReadValue tag = read_numeric_tag(input,position,limit,invalid);
    if (tag.status == ReadStatus::Success) { unsafe_position<u8>(output) = u8(tag.value); }
    return tag.status;
}

extern "C" ReadStatus numeric_read_integer_key(Bytes input,inout u64 position,address output) on cpu {
    return read_numeric_byte(input,position,output,9u64,ReadStatus::IntegerOperation);
}

extern "C" void numeric_write_expression_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    u32 form = unsafe_position<u32>(source);
    word(form,output,append);
    if (form == 1) {
        numeric_write_selection_key(domain_at(source,8u64),output,append);
        return;
    }
    numeric_write_integer_key(domain_at(source,4u64),output,append);
    numeric_write_scalar(fact_scalar_tag(domain_at(source,5u64)),output,append);
    numeric_write_operand_key(domain_at(source,8u64),output,append);
    numeric_write_operand_key(domain_at(source,48u64),output,append);
}

extern "C" ReadStatus numeric_read_expression_key(Bytes input,inout u64 position,address output) on cpu {
    ReadValue form = read_numeric_tag(input,position,1u64,ReadStatus::IntegerExpression);
    if (form.status != ReadStatus::Success) { return form.status; }
    if (form.value == 1u64) {
        SelectionExpression key = SelectionExpression(1,
            array<u64,10>(0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64));
        ReadStatus status = numeric_read_selection_key(input,position,address_of(key.selection[0]));
        if (status == ReadStatus::Success) { RtlMoveMemory(output,address_of(key),88u64); }
        return status;
    }
    ReadValue operation = read_numeric_tag(input,position,9u64,ReadStatus::IntegerOperation);
    if (operation.status != ReadStatus::Success) { return operation.status; }
    BuiltinScalar scalar = BuiltinScalar::F32;
    ReadStatus status = numeric_read_scalar(input,position,scalar);
    if (status != ReadStatus::Success) { return status; }
    NumericOperand left = empty_operand();
    status = numeric_read_operand(input,position,left);
    if (status != ReadStatus::Success) { return status; }
    NumericOperand right = empty_operand();
    status = numeric_read_operand(input,position,right);
    if (status != ReadStatus::Success) { return status; }
    BinaryExpression key = BinaryExpression(0,u8(operation.value),u8(u32(scalar)),make_key_operand(left),make_key_operand(right));
    RtlMoveMemory(output,address_of(key),88u64);
    return ReadStatus::Success;
}

extern "C" void numeric_write_comparison_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_integer_key(source,output,append);
}

extern "C" ReadStatus numeric_read_comparison_key(Bytes input,inout u64 position,address output) on cpu {
    return read_numeric_byte(input,position,output,5u64,ReadStatus::Comparison);
}

extern "C" void numeric_write_floating_form_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_integer_key(source,output,append);
}

extern "C" ReadStatus numeric_read_floating_form_key(Bytes input,inout u64 position,address output) on cpu {
    return read_numeric_byte(input,position,output,1u64,ReadStatus::FloatingForm);
}

extern "C" void numeric_write_floating_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_identity_key(source,output,append);
    u64 offset = 32u64;
    while (offset < 72u64) {
        wide(unsafe_position<u64>(domain_at(source,offset)),output,append);
        offset += 8u64;
    }
    numeric_write_scalar(fact_scalar_tag(domain_at(source,72u64)),output,append);
    numeric_write_floating_form_key(domain_at(source,73u64),output,append);
}

extern "C" ReadStatus numeric_read_floating_key(Bytes input,inout u64 position,address output) on cpu {
    NumericIdentity source = empty_key_identity();
    ReadStatus status = numeric_read_identity(input,position,source);
    if (status != ReadStatus::Success) { return status; }
    array<u64,5> bounds = array<u64,5>(0u64,0u64,0u64,0u64,0u64);
    u32 index = 0;
    while (index < 5) {
        ReadValue bits = fixed_value(input,position,8);
        if (bits.status != ReadStatus::Success) { return bits.status; }
        bounds[index] = bits.value;
        index += 1;
    }
    BuiltinScalar scalar = BuiltinScalar::F32;
    status = numeric_read_scalar(input,position,scalar);
    if (status != ReadStatus::Success) { return status; }
    ReadValue form = read_numeric_tag(input,position,1u64,ReadStatus::FloatingForm);
    if (form.status != ReadStatus::Success) { return form.status; }
    FloatingKey key = FloatingKey(make_key_identity(source),bounds,u8(u32(scalar)),u8(form.value));
    RtlMoveMemory(output,address_of(key),80u64);
    return ReadStatus::Success;
}

struct DomainKey { DomainKind kind; u32 compact; array<u64,12> payload; };

extern "C" void RtlMoveMemory(address output, address input, u64 bytes) on cpu from "KERNEL32.dll";

address domain_at(address source, u64 offset) on cpu {
    return unsafe_address(address_bits(source) + offset);
}

u32 domain_wire_tag(DomainKind kind) on cpu {
    if (kind == DomainKind::Lower) { return 15; }
    if (kind == DomainKind::Room) { return 10; }
    if (kind == DomainKind::Sum) { return 11; }
    if (kind == DomainKind::Shifted) { return 12; }
    if (kind == DomainKind::Computed) { return 14; }
    return u32(kind);
}

DomainKind domain_physical_tag(u32 tag) on cpu {
    if (tag == 15) { return DomainKind::Lower; }
    if (tag == 10) { return DomainKind::Room; }
    if (tag == 11) { return DomainKind::Sum; }
    if (tag == 12) { return DomainKind::Shifted; }
    if (tag == 14) { return DomainKind::Computed; }
    return unsafe_position<DomainKind>(address_of(tag));
}

extern "C" void numeric_write_domain(address source,address output,cfn<void,address,address,u64> append) on cpu
    requires nonzero(append) {
    DomainKind kind = unsafe_position<DomainKind>(source);
    word(domain_wire_tag(kind),output,append);
    address first = domain_at(source,8u64);
    if (kind == DomainKind::Tile || kind == DomainKind::TileLength) { return; }
    if (kind == DomainKind::Data || kind == DomainKind::DataLength) {
        wide(unsafe_position<u64>(first),output,append);
        return;
    }
    if (kind == DomainKind::Fixed) {
        word(unsafe_position<u32>(domain_at(source,4u64)),output,append);
        return;
    }
    if (kind == DomainKind::Computed) {
        numeric_write_expression_key(first,output,append);
        return;
    }
    if (kind == DomainKind::Floating) {
        numeric_write_floating_key(first,output,append);
        return;
    }
    if (kind == DomainKind::KnownBits) {
        numeric_write_scalar(fact_scalar_tag(domain_at(source,4u64)),output,append);
        wide(unsafe_position<u64>(first),output,append);
        wide(unsafe_position<u64>(domain_at(source,16u64)),output,append);
        return;
    }

    numeric_write_identity(capture_key_identity(first),output,append);
    if (kind == DomainKind::View || kind == DomainKind::ViewLength) {
        numeric_write_identity(capture_key_identity(domain_at(source,40u64)),output,append);
        numeric_write_identity(capture_key_identity(domain_at(source,72u64)),output,append);
    } else if (kind == DomainKind::Sum || kind == DomainKind::Shifted) {
        numeric_write_identity(capture_key_identity(domain_at(source,40u64)),output,append);
        if (kind == DomainKind::Shifted) {
            word(unsafe_position<u32>(domain_at(source,72u64)),output,append);
        }
    } else if (kind == DomainKind::Upper || kind == DomainKind::Lower || kind == DomainKind::Offset) {
        word(unsafe_position<u32>(domain_at(source,40u64)),output,append);
    } else if (kind == DomainKind::CommonRange) {
        numeric_write_scalar(fact_scalar_tag(domain_at(source,40u64)),output,append);
        numeric_write_scalar(fact_scalar_tag(domain_at(source,41u64)),output,append);
        numeric_write_scalar(fact_scalar_tag(domain_at(source,42u64)),output,append);
        numeric_write_scalar(fact_scalar_tag(domain_at(source,43u64)),output,append);
    }
}

NumericIdentity empty_key_identity() on cpu {
    return NumericIdentity(IdentityKind::Register,0u64,0u64,0u64);
}

DomainKey empty_domain_key() on cpu {
    return DomainKey(DomainKind::Tile,0,
        array<u64,12>(0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64,0u64));
}

ReadStatus read_domain_identity(Bytes input, inout u64 position, address output) on cpu {
    NumericIdentity value = empty_key_identity();
    ReadStatus status = numeric_read_identity(input,position,value);
    if (status == ReadStatus::Success) {
        IdentityKey key = make_key_identity(value);
        RtlMoveMemory(output,address_of(key),32u64);
    }
    return status;
}

ReadStatus read_domain_key(Bytes input,inout u64 position,inout DomainKey output) on cpu {
    ReadValue tag = fixed_value(input,position,4);
    if (tag.status != ReadStatus::Success) { return tag.status; }
    if (tag.value > 18u64) { return ReadStatus::IndexDomain; }

    DomainKind kind = domain_physical_tag(u32(tag.value));
    output.kind = kind;
    if (kind == DomainKind::Tile || kind == DomainKind::TileLength) { return ReadStatus::Success; }
    if (kind == DomainKind::Data || kind == DomainKind::DataLength) {
        ReadValue ordinal = fixed_value(input,position,8);
        if (ordinal.status != ReadStatus::Success) { return ordinal.status; }
        output.payload[0] = ordinal.value;
        return ReadStatus::Success;
    }
    if (kind == DomainKind::Fixed) {
        ReadValue count = fixed_value(input,position,4);
        if (count.status != ReadStatus::Success) { return count.status; }
        output.compact = u32(count.value);
        return ReadStatus::Success;
    }
    if (kind == DomainKind::Computed) {
        return numeric_read_expression_key(input,position,address_of(output.payload[0]));
    }
    if (kind == DomainKind::Floating) {
        return numeric_read_floating_key(input,position,address_of(output.payload[0]));
    }
    if (kind == DomainKind::KnownBits) {
        BuiltinScalar scalar = BuiltinScalar::F32;
        ReadStatus status = numeric_read_scalar(input,position,scalar);
        if (status != ReadStatus::Success) { return status; }
        ReadValue mask = fixed_value(input,position,8);
        if (mask.status != ReadStatus::Success) { return mask.status; }
        ReadValue bits = fixed_value(input,position,8);
        if (bits.status != ReadStatus::Success) { return bits.status; }
        output.compact = u32(scalar);
        output.payload[0] = mask.value;
        output.payload[1] = bits.value;
        return ReadStatus::Success;
    }

    ReadStatus status = read_domain_identity(input,position,address_of(output.payload[0]));
    if (status != ReadStatus::Success) { return status; }
    if (kind == DomainKind::View || kind == DomainKind::ViewLength) {
        status = read_domain_identity(input,position,address_of(output.payload[4]));
        if (status != ReadStatus::Success) { return status; }
        return read_domain_identity(input,position,address_of(output.payload[8]));
    }
    if (kind == DomainKind::Sum || kind == DomainKind::Shifted) {
        status = read_domain_identity(input,position,address_of(output.payload[4]));
        if (status != ReadStatus::Success) { return status; }
        if (kind == DomainKind::Shifted) {
            ReadValue distance = fixed_value(input,position,4);
            if (distance.status != ReadStatus::Success) { return distance.status; }
            output.payload[8] = distance.value;
        }
    } else if (kind == DomainKind::Upper || kind == DomainKind::Lower || kind == DomainKind::Offset) {
        ReadValue distance = fixed_value(input,position,4);
        if (distance.status != ReadStatus::Success) { return distance.status; }
        output.payload[4] = distance.value;
    } else if (kind == DomainKind::CommonRange) {
        u64 scalars = 0u64;
        u64 shift = 0u64;
        while (shift < 32u64) {
            BuiltinScalar scalar = BuiltinScalar::F32;
            status = numeric_read_scalar(input,position,scalar);
            if (status != ReadStatus::Success) { return status; }
            scalars |= u64(u32(scalar)) << shift;
            shift += 8u64;
        }
        output.payload[4] = scalars;
    }
    return ReadStatus::Success;
}

extern "C" ReadStatus numeric_read_domain(Bytes input,inout u64 position,address output) on cpu {
    DomainKey key = empty_domain_key();
    ReadStatus status = read_domain_key(input,position,key);
    if (status == ReadStatus::Success) { RtlMoveMemory(output,address_of(key),104u64); }
    return status;
}

void numeric_write_predicate(NumericPredicate value,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
word(u32(value.operation),output,append);
numeric_write_scalar(value.scalar,output,append);
numeric_write_operand(value.left,output,append);
numeric_write_operand(value.right,output,append);
}

ReadStatus numeric_read_predicate(Bytes input,inout u64 position,inout NumericPredicate output) on cpu {
ReadValue comparison = read_numeric_tag(input,position,5u64,ReadStatus::Comparison);
if (comparison.status != ReadStatus::Success) { return comparison.status; }
u32 op_code = u32(comparison.value);
output.operation = unsafe_position<ComparisonKind>(address_of(op_code));
ReadStatus status_75=numeric_read_scalar(input,position,output.scalar);if(status_75!=ReadStatus::Success){return status_75;}
ReadStatus status_76=numeric_read_operand(input,position,output.left);if(status_76!=ReadStatus::Success){return status_76;}
ReadStatus status_77=numeric_read_operand(input,position,output.right);if(status_77!=ReadStatus::Success){return status_77;}
return ReadStatus::Success;
}

extern "C" void numeric_write_predicate_key(address source,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    numeric_write_predicate(capture_key_predicate(source),output,append);
}

extern "C" ReadStatus numeric_read_predicate_key(Bytes input,inout u64 position,address output) on cpu {
    NumericOperand operand = empty_operand();
    NumericPredicate value = NumericPredicate(ComparisonKind::Equal,BuiltinScalar::F32,operand,operand);
    ReadStatus status = numeric_read_predicate(input,position,value);
    if (status == ReadStatus::Success) {
        PredicateKey key = make_key_predicate(value);
        RtlMoveMemory(output,address_of(key),88u64);
    }
    return status;
}

extern "C" void numeric_write_fact(address source,address output,cfn<void,address,address,u64> append) on cpu
    requires nonzero(append) {
    FactView value = read_fact_fields(source);
    word(u32(value.kind),output,append);
    if (value.kind == FactKind::Predicate) {
        numeric_write_predicate(capture_key_predicate(value.payload),output,append);
        byte(u8(value.flag),output,append);
        return;
    }

    numeric_write_identity(capture_key_identity(value.first),output,append);
    if (value.kind == FactKind::Equal || value.kind == FactKind::NotEqual || value.kind == FactKind::Below) {
        numeric_write_identity(capture_key_identity(value.second),output,append);
        if (value.kind == FactKind::Below) {
            byte(u8(value.flag),output,append);
            numeric_write_scalar(fact_scalar_tag(value.scalar),output,append);
        }
    } else if (value.kind == FactKind::Index || value.kind == FactKind::Outside) {
        numeric_write_domain(value.payload,output,append);
    } else if (value.kind == FactKind::Minimum || value.kind == FactKind::Maximum) {
        numeric_write_scalar(fact_scalar_tag(value.scalar),output,append);
        wide(value.value,output,append);
    } else if (value.kind == FactKind::Boolean) {
        byte(u8(value.flag),output,append);
    }
}

ReadStatus read_fact_predicate(Bytes input, inout u64 position, address output) on cpu {
    NumericOperand empty = NumericOperand(OperandKind::Value,empty_key_identity(),BuiltinScalar::F32,0u64);
    NumericPredicate predicate = NumericPredicate(ComparisonKind::Equal,BuiltinScalar::F32,empty,empty);
    ReadStatus status = numeric_read_predicate(input,position,predicate);
    if (status != ReadStatus::Success) { return status; }
    ReadValue truth = read_boolean(input,position);
    if (truth.status != ReadStatus::Success) { return truth.status; }

    PredicateKey key = make_key_predicate(predicate);
    address payload = address_of(key);
    materialize_fact(FactView(1,FactKind::Predicate,address(),address(),payload,address(),
        0u64,0u64,u32(truth.value),0),output);
    return ReadStatus::Success;
}

extern "C" ReadStatus numeric_read_fact(Bytes input,inout u64 position,address output) on cpu {
    ReadValue tag = fixed_value(input,position,4);
    if (tag.status != ReadStatus::Success) { return tag.status; }
    if (tag.value > u64(u32(FactKind::Predicate))) { return ReadStatus::Fact; }
    u32 code = u32(tag.value);
    FactKind kind = unsafe_position<FactKind>(address_of(code));
    if (kind == FactKind::Predicate) { return read_fact_predicate(input,position,output); }

    NumericIdentity identity = empty_key_identity();
    ReadStatus status = numeric_read_identity(input,position,identity);
    if (status != ReadStatus::Success) { return status; }
    IdentityKey key = make_key_identity(identity);
    address first = address_of(key);
    FactView view = FactView(1,kind,first,address(),address(),address(),0u64,0u64,0,0);

    if (kind == FactKind::Index || kind == FactKind::Outside) {
        DomainKey domain = empty_domain_key();
        view.payload = address_of(domain);
        status = read_domain_key(input,position,domain);
        if (status != ReadStatus::Success) { return status; }
        materialize_fact(view,output);
        return ReadStatus::Success;
    }

    BuiltinScalar scalar = BuiltinScalar::F32;
    u8 scalar_code = u8(0);
    if (kind == FactKind::Equal || kind == FactKind::NotEqual || kind == FactKind::Below) {
        NumericIdentity second = empty_key_identity();
        status = numeric_read_identity(input,position,second);
        if (status != ReadStatus::Success) { return status; }
        if (kind == FactKind::Below) {
            ReadValue truth = read_boolean(input,position);
            if (truth.status != ReadStatus::Success) { return truth.status; }
            view.flag = u32(truth.value);
            status = numeric_read_scalar(input,position,scalar);
            if (status != ReadStatus::Success) { return status; }
            scalar_code = u8(u32(scalar));
            view.scalar = address_of(scalar_code);
        }
        IdentityKey other = make_key_identity(second);
        view.second = address_of(other);
        materialize_fact(view,output);
        return ReadStatus::Success;
    }

    if (kind == FactKind::Boolean) {
        ReadValue truth = read_boolean(input,position);
        if (truth.status != ReadStatus::Success) { return truth.status; }
        view.flag = u32(truth.value);
    } else if (kind == FactKind::Minimum || kind == FactKind::Maximum) {
        status = numeric_read_scalar(input,position,scalar);
        if (status != ReadStatus::Success) { return status; }
        ReadValue bits = fixed_value(input,position,8);
        if (bits.status != ReadStatus::Success) { return bits.status; }
        scalar_code = u8(u32(scalar));
        view.scalar = address_of(scalar_code);
        view.value = bits.value;
    }
    materialize_fact(view,output);
    return ReadStatus::Success;
}

extern "C" void numeric_write_ranges(address intervals,u64 count,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    wide(count,output,append);
    u64 index = 0u64;
    while (index < count) {
        BitRange value = proof_range_record(intervals,index);
        wide(value.lower,output,append);
        wide(value.upper,output,append);
        index += 1u64;
    }
}

extern "C" ReadStatus numeric_read_ranges(Bytes input,inout u64 position,address output) on cpu {
    ReadValue count = fixed_value(input,position,8);
    if (count.status != ReadStatus::Success) { return count.status; }
    u64 index = 0u64;
    while (index < count.value) {
        ReadValue lower = fixed_value(input,position,8);
        if (lower.status != ReadStatus::Success) { return lower.status; }
        ReadValue upper = fixed_value(input,position,8);
        if (upper.status != ReadStatus::Success) { return upper.status; }
        proof_range_push_ends(output,lower.value,upper.value);
        index += 1u64;
    }
    return ReadStatus::Success;
}

extern "C" void write_proof_conditions(address context,u64 length,address graph,address output,ConditionWriter host) on cpu
 requires nonzero(host.append) requires nonzero(host.condition) {
 u64 count=unsafe_position<CanonicalGraph>(graph).nodes.count;
 u64 width=length;
 while(width!=0u64){RootFields last=host.condition(context,width-1u64);if(last.present!=0){break;}width-=1u64;}
 byte(width!=0u64?u8(1):u8(0),output,host.append);
 if(width==0u64){return;}
 byte(u8(3),output,host.append);wide(count,output,host.append);
 u64 index=0u64;
 while(index<count){
  NodeFields value=condition_node_at(graph,index);
  word(u32(value.kind),output,host.append);
  if(value.kind==NodeKind::Facts){write_fact_sequence(value.facts,output,host.append);}
  else if(value.kind==NodeKind::Both||value.kind==NodeKind::Either){
   wide(value.first,output,host.append);wide(value.second,output,host.append);
  }
  index+=1u64;
 }
 wide(width,output,host.append);index=0u64;
 while(index<width){
  RootFields value=host.condition(context,index);byte(value.present!=0?u8(1):u8(0),output,host.append);
  if(value.present!=0){wide(condition_node_index(graph,value.first),output,host.append);wide(condition_node_index(graph,value.second),output,host.append);}
  index+=1u64;
 }
}

i32 compare_condition_index(u64 left,u64 right) on cpu {
 u64 a=left;u64 b=right;
 while(a!=b){
  u64 x=a&255u64;u64 y=b&255u64;
  if(x!=y){return x<y?-i32(1):i32(1);}
  a=a>>8;b=b>>8;
 }
 return i32(0);
}

extern "C" i32 compare_proof_condition(address left,address right,address left_graph,address right_graph,
 cfn<RootFields,address,u64> root,cfn<i32,address,address> compare_facts) on cpu
 requires nonzero(root) requires nonzero(compare_facts) {
 u64 left_count=unsafe_position<CanonicalGraph>(left_graph).nodes.count;
 u64 right_count=unsafe_position<CanonicalGraph>(right_graph).nodes.count;
 // One condition on each side. The presence, format, and root-count bytes match.
 i32 order=compare_condition_index(left_count,right_count);
 if(order!=i32(0)){return order;}
 u64 i=0u64;
 while(i<left_count){
  NodeFields a=condition_node_at(left_graph,i);NodeFields b=condition_node_at(right_graph,i);
  u32 a_kind=u32(a.kind);u32 b_kind=u32(b.kind);
  if(a_kind!=b_kind){return a_kind<b_kind?-i32(1):i32(1);}
  if(a.kind==NodeKind::Facts){
   order=compare_facts(a.facts,b.facts);
   if(order!=i32(0)){return order;}
  }else if(a.kind==NodeKind::Both||a.kind==NodeKind::Either){
   order=compare_condition_index(a.first,b.first);
   if(order!=i32(0)){return order;}
   order=compare_condition_index(a.second,b.second);
   if(order!=i32(0)){return order;}
  }
  i+=1u64;
 }
 RootFields a=root(left,0u64);RootFields b=root(right,0u64);
 order=compare_condition_index(condition_node_index(left_graph,a.first),condition_node_index(right_graph,b.first));
 if(order!=i32(0)){return order;}
 return compare_condition_index(condition_node_index(left_graph,a.second),condition_node_index(right_graph,b.second));
}

extern "C" void write_fact_sequence(address facts,address output,cfn<void,address,address,u64> append) on cpu requires nonzero(append) {
    u64 count = unsafe_position<OrderedTree>(facts).count;
    wide(count,output,append);
    FactCursor cursor = fact_tree_cursor(facts);
    u64 index = 0u64;
    while (index < count) {
        address key = fact_tree_next(facts,address_of(cursor),0);
        numeric_write_fact(key,output,append);
        index += 1u64;
    }
}

ReadStatus read_fact_sequence(Bytes input,inout u64 position,address output) on cpu {
    ReadValue count = fixed_value(input,position,8);
    if (count.status != ReadStatus::Success) { return count.status; }
    u64 index = 0u64;
    while (index < count.value) {
        address key = fact_tree_prepare_key(output);
        ReadStatus status = numeric_read_fact(input,position,key);
        if (status != ReadStatus::Success) { return status; }
        fact_tree_insert(output,key);
        index += 1u64;
    }
    return ReadStatus::Success;
}

ReadStatus read_condition_facts(Bytes input,inout u64 position,address output,
    cfn<void,address,address> take) on cpu requires nonzero(take) {
    OrderedTree facts = OrderedTree(address(),0u64,0u64,0u64);
    ReadStatus status = read_fact_sequence(input,position,address_of(facts));
    if (status != ReadStatus::Success) {
        fact_tree_clear(address_of(facts));
        return status;
    }
    // The receiving encoded node takes the populated tree.
    take(output,address_of(facts));
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
  else if(tag.value==1u64){ReadStatus status=read_condition_facts(input,position,output,host.facts);if(status!=ReadStatus::Success){return status;}}
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

extern "C" void contract_write_requirement(address key,address condition,address output,cfn<void,address,address> key_write,cfn<void,address,address> condition_write,cfn<void,address,address,u64> append) on cpu requires nonzero(key_write) requires nonzero(condition_write) requires nonzero(append){key_write(key,output);wide(1u64,output,append);condition_write(condition,output);}

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

extern "C" void contract_write_view_access_position(address output,ContractViewAccessPositionFields fields,ContractViewAccessPositionWriter host,cfn<void,address,address,u64> append,u32 external_atomic) on cpu requires nonzero(host.borrows) requires nonzero(host.when) requires nonzero(host.position) requires nonzero(append){word(external_atomic!=0?6:1,output,append);host.borrows(fields.borrows,output);host.when(fields.when,output);host.position(fields.position,output);}

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

extern "C" ReadStatus contract_read_address_root(Bytes input,inout u64 position,address context,address output,ContractAddressRootVariants host) on cpu requires nonzero(host.local) requires nonzero(host.parameter) requires nonzero(host.resource) requires nonzero(host.buffer) requires nonzero(host.derived) requires nonzero(host.external) requires nonzero(host.dataset){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.local(context,output);}if(tag.value==1u64){return host.parameter(context,output);}if(tag.value==2u64){return host.resource(context,output);}if(tag.value==3u64){return host.buffer(context,output);}if(tag.value==4u64){return host.derived(context,output);}if(tag.value==5u64){return host.external(context,output);}if(tag.value==6u64){return host.dataset(context,output);}return ReadStatus::ContractAddressRoot;}

extern "C" ReadStatus contract_read_initialization_source(Bytes input,inout u64 position,address context,address output,ContractInitializationSourceVariants host) on cpu requires nonzero(host.position) requires nonzero(host.task){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.position(context,output);}if(tag.value==1u64){return host.task(context,output);}return ReadStatus::ContractInitializationSource;}

extern "C" ReadStatus contract_read_element_facts(Bytes input,inout u64 position,address context,address output,ContractElementFactsVariants host) on cpu requires nonzero(host.unknown) requires nonzero(host.known) requires nonzero(host.cleanup) requires nonzero(host.common) requires nonzero(host.selected) requires nonzero(host.input){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.unknown(context,output);}if(tag.value==1u64){return host.known(context,output);}if(tag.value==2u64){return host.cleanup(context,output);}if(tag.value==3u64){return host.common(context,output);}if(tag.value==4u64){return host.selected(context,output);}if(tag.value==5u64){return host.input(context,output);}return ReadStatus::ContractElementFacts;}

extern "C" ReadStatus contract_read_data_layout(Bytes input,inout u64 position,address context,address output,ContractDataLayoutVariants host) on cpu requires nonzero(host.declared) requires nonzero(host.parameter){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.declared(context,output);}if(tag.value==1u64){return host.parameter(context,output);}return ReadStatus::ContractDataLayout;}

extern "C" ReadStatus contract_read_returned(Bytes input,inout u64 position,address context,address output,ContractReturnedVariants host) on cpu requires nonzero(host.unit) requires nonzero(host.value) requires nonzero(host.dataset) requires nonzero(host.owned_value){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.unit(context,output);}if(tag.value==1u64){return host.value(context,output);}if(tag.value==2u64){return host.dataset(context,output);}if(tag.value==3u64){return host.owned_value(context,output);}return ReadStatus::ContractReturned;}

extern "C" ReadStatus contract_read_task_projection(Bytes input,inout u64 position,address context,address output,ContractTaskProjectionVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.writes) requires nonzero(host.places) requires nonzero(host.initialized){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.returned(context,output);}if(tag.value==1u64){return host.writes(context,output);}if(tag.value==2u64){return host.places(context,output);}if(tag.value==3u64){return host.initialized(context,output);}return ReadStatus::ContractTaskProjection;}

extern "C" ReadStatus contract_read_exclusion(Bytes input,inout u64 position,address context,address output,ContractExclusionVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.ownership) requires nonzero(host.position) requires nonzero(host.view){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.resource(context,output);}if(tag.value==1u64){return host.ownership(context,output);}if(tag.value==2u64){return host.position(context,output);}if(tag.value==3u64){return host.view(context,output);}return ReadStatus::ContractExclusion;}

extern "C" ReadStatus contract_read_view_access(Bytes input,inout u64 position,address context,address output,ContractViewAccessVariants host) on cpu requires nonzero(host.resource) requires nonzero(host.position) requires nonzero(host.memory) requires nonzero(host.dataset) requires nonzero(host.initialized) requires nonzero(host.stored) requires nonzero(host.external_atomic){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.resource(context,output);}if(tag.value==1u64){return host.position(context,output);}if(tag.value==2u64){return host.memory(context,output);}if(tag.value==3u64){return host.dataset(context,output);}if(tag.value==4u64){return host.initialized(context,output);}if(tag.value==5u64){return host.stored(context,output);}if(tag.value==6u64){return host.external_atomic(context,output);}return ReadStatus::ContractViewAccess;}

extern "C" ReadStatus contract_read_residency_part(Bytes input,inout u64 position,address context,address output,ContractResidencyPartVariants host) on cpu requires nonzero(host.field) requires nonzero(host.elements) requires nonzero(host.selected){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.field(context,output);}if(tag.value==1u64){return host.elements(context,output);}if(tag.value==2u64){return host.selected(context,output);}return ReadStatus::ContractResidencyPart;}

extern "C" ReadStatus contract_read_residency_origin(Bytes input,inout u64 position,address context,address output,ContractResidencyOriginVariants host) on cpu requires nonzero(host.cpu) requires nonzero(host.gpu) requires nonzero(host.input) requires nonzero(host.output) requires nonzero(host.task_transfer){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.cpu(context,output);}if(tag.value==1u64){return host.gpu(context,output);}if(tag.value==2u64){return host.input(context,output);}if(tag.value==3u64){return host.output(context,output);}if(tag.value==4u64){return host.task_transfer(context,output);}return ReadStatus::ContractResidencyOrigin;}

extern "C" ReadStatus contract_read_residency(Bytes input,inout u64 position,address context,address output,ContractResidencyVariants host) on cpu requires nonzero(host.absent) requires nonzero(host.origins) requires nonzero(host.selected){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.absent(context,output);}if(tag.value==1u64){return host.origins(context,output);}if(tag.value==2u64){return host.selected(context,output);}return ReadStatus::ContractResidency;}

extern "C" ReadStatus contract_read_residency_output_kind(Bytes input,inout u64 position,address context,address output,ContractResidencyOutputKindVariants host) on cpu requires nonzero(host.returned) requires nonzero(host.written){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.returned(context,output);}if(tag.value==1u64){return host.written(context,output);}return ReadStatus::ContractResidencyOutputKind;}

extern "C" ReadStatus contract_read_relation_op(Bytes input,inout u64 position,address context,address output,ContractRelationOpVariants host) on cpu requires nonzero(host.identity) requires nonzero(host.position) requires nonzero(host.read) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.resources) requires nonzero(host.owners) requires nonzero(host.select) requires nonzero(host.initial){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.identity(context,output);}if(tag.value==1u64){return host.position(context,output);}if(tag.value==2u64){return host.read(context,output);}if(tag.value==3u64){return host.element(context,output);}if(tag.value==4u64){return host.projection(context,output);}if(tag.value==5u64){return host.resources(context,output);}if(tag.value==6u64){return host.owners(context,output);}if(tag.value==7u64){return host.select(context,output);}if(tag.value==8u64){return host.initial(context,output);}return ReadStatus::ContractRelationOp;}

extern "C" ReadStatus contract_read_ownership(Bytes input,inout u64 position,address context,address output,ContractOwnershipVariants host) on cpu requires nonzero(host.owned) requires nonzero(host.borrowed) requires nonzero(host.moved){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.owned(context,output);}if(tag.value==1u64){return host.borrowed(context,output);}if(tag.value==2u64){return host.moved(context,output);}return ReadStatus::ContractOwnership;}

extern "C" ReadStatus contract_read_address_identity(Bytes input,inout u64 position,address context,address output,ContractAddressIdentityVariants host) on cpu requires nonzero(host.constant) requires nonzero(host.value) requires nonzero(host.storage){ReadValue tag=fixed_value(input,position,1);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.constant(context,output);}if(tag.value==1u64){return host.value(context,output);}if(tag.value==2u64){return host.storage(context,output);}return ReadStatus::ContractAddressIdentity;}

extern "C" ReadStatus contract_read_borrow_source(Bytes input,inout u64 position,address context,address output,ContractBorrowSourceVariants host) on cpu requires nonzero(host.data) requires nonzero(host.parameter) requires nonzero(host.unresolved) requires nonzero(host.inactive) requires nonzero(host.external) requires nonzero(host.position) requires nonzero(host.element) requires nonzero(host.projection) requires nonzero(host.contents) requires nonzero(host.relation) requires nonzero(host.task) requires nonzero(host.task_output){ReadValue tag=fixed_value(input,position,4);if(tag.status!=ReadStatus::Success){return tag.status;}if(tag.value==0u64){return host.data(context,output);}if(tag.value==1u64){return host.parameter(context,output);}if(tag.value==2u64){return host.unresolved(context,output);}if(tag.value==3u64){return host.inactive(context,output);}if(tag.value==4u64){return host.external(context,output);}if(tag.value==5u64){return host.position(context,output);}if(tag.value==6u64){return host.element(context,output);}if(tag.value==7u64){return host.projection(context,output);}if(tag.value==8u64){return host.contents(context,output);}if(tag.value==9u64){return host.relation(context,output);}if(tag.value==10u64){return host.task(context,output);}if(tag.value==11u64){return host.task_output(context,output);}return ReadStatus::ContractBorrowSource;}

extern "C" void group_public_access_conditions(usize stride,PublicAccessGroupHost host) on cpu
    requires nonzero(stride) requires nonzero(host.next) requires nonzero(host.compare)
    requires nonzero(host.seed) requires nonzero(host.merge) requires nonzero(host.emit) {
    OrderedTree groups=OrderedTree(address(),0u64,0u64,0u64);
    while(true){
        let access=host.next(host.context);
        if(access==address()){break;}
        let place=ordered_tree_seek(address_of(groups),stride,access,host.compare);
        if(place.entry!=address()){
            host.merge(host.context,place.entry);
        }else{
            let slot=ordered_tree_insert_slot(address_of(groups),stride,place.parent,place.order);
            host.seed(host.context,slot);
        }
    }
    FactCursor cursor=ordered_tree_cursor(address_of(groups),stride);
    while(true){
        let slot=ordered_tree_step(address_of(groups),stride,address_of(cursor),0);
        if(slot==address()){break;}
        host.emit(host.context,slot);
    }
    fact_tree_clear(address_of(groups));
}
