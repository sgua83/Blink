module bootstrap.function_inputs;
import bootstrap.call_execution;

extern "C" ResourceStep resource_step(u32 stage,Facts facts) on cpu {
    u32 next = stage;
    if (next == 0) {
        if (facts.dataset != 0) { return ResourceStep(Resource::Buffer, 1); }
        next = 1;
    }
    if (next == 1) {
        if (facts.acceleration != 0) { return ResourceStep(Resource::Acceleration, 2); }
        next = 2;
    }
    if (next == 2) {
        if (facts.texture != 0) { return ResourceStep(Resource::Texture, 3); }
        next = 3;
    }
    if (next == 3) { return ResourceStep(Resource::Keep, 4); }
    return ResourceStep(Resource::Done, 4);
}

// Stage length and partition start precede the logical dataset lengths.
extern "C" u32 uniform_bytes(u32 buffers) on cpu {
    return 8 + 4 * buffers;
}

u32 align_uniform(u32 bytes, u32 alignment) on cpu {
    return (bytes + alignment - 1) & ~(alignment - 1);
}

extern "C" u32 layout_uniforms(u32 buffers, usize count, address context,
    cfn<UniformField,address,usize> ensures memory(read) field,
    cfn<void,address,usize,u32> place) on cpu
    requires nonzero(field) requires nonzero(place) {
    u32 bytes = uniform_bytes(buffers);
    usize index = 0u64;
    while (index < count) {
        UniformField value = field(context, index);
        u32 offset = align_uniform(bytes, value.alignment);
        place(context, index, offset);
        bytes = offset + value.size;
        index += 1u64;
    }
    return align_uniform(bytes, 4);
}

extern "C" Parameter parameter_form(Facts facts) on cpu {
    if (facts.acceleration != 0) { return Parameter::Acceleration; }
    if (facts.texture != 0) { return Parameter::Texture; }
    if (facts.dataset != 0) { return Parameter::Dataset; }
    if (facts.tile != 0) { return Parameter::Tile; }
    return Parameter::Ordinary;
}

extern "C" bool domain_result(u32 unit, usize arguments, u32 dataset, u32 tile, u32 view) on cpu {
    return unit != 0 && arguments == 0u64 && (dataset | tile | view) == 0;
}

extern "C" DomainParameter domain_parameter(Facts facts, CallAccess access) on cpu {
    Parameter form = parameter_form(facts);
    if (form == Parameter::Dataset) {
        return DomainParameter(form, access != CallAccess::Value ? DomainLayout::Accept : DomainLayout::Reject);
    }
    if (form == Parameter::Texture || form == Parameter::Acceleration) {
        return DomainParameter(form, DomainLayout::Accept);
    }
    return DomainParameter(form, access == CallAccess::Value ? DomainLayout::PlainLayout : DomainLayout::Reject);
}

extern "C" DomainBinding domain_binding(Parameter form, CallAccess access, u32 gpu, u32 qualified) on cpu {
    if (form == Parameter::Texture) {
        return DomainBinding(access == CallAccess::Read ? DomainAccessError::None : DomainAccessError::Texture, 0);
    }
    if (form == Parameter::Acceleration) {
        return DomainBinding(access == CallAccess::Read && gpu != 0 ?
            DomainAccessError::None : DomainAccessError::Acceleration, 0);
    }
    DomainAccessError error = DomainAccessError::None;
    if (form == Parameter::Dataset) {
        if (qualified == 0 && access == CallAccess::Value) { error = DomainAccessError::Dataset; }
        u32 view_check = access != CallAccess::Read && access != CallAccess::InOut ? 1 : 0;
        return DomainBinding(error, view_check);
    }
    if (qualified == 0 && access != CallAccess::Value) { error = DomainAccessError::Uniform; }
    return DomainBinding(error, 1);
}

extern "C" Dataset dataset_form(u32 ordinary_cpu,u32 by_value) on cpu {
    if (ordinary_cpu == 0) { return Dataset::Device; }
    if (by_value != 0) { return Dataset::Owned; }
    return Dataset::Borrowed;
}

extern "C" ValueForm value_form(Mode mode,u32 read,u32 atomic,u32 by_value,u32 owned) on cpu {
    if (read != 0 && owned != 0) { return ValueForm::OwnedRead; }
    if (atomic != 0 && mode.application == 0) { return ValueForm::AtomicParameter; }
    if (by_value == 0) { return ValueForm::Reference; }
    return ValueForm::Value;
}

extern "C" PlaceForm place_form(Mode mode,u32 read) on cpu {
    if (mode.application != 0) {
        if (mode.c_abi != 0) { return PlaceForm::CAbi; }
        return PlaceForm::Native;
    }
    if (read != 0) { return PlaceForm::LocalRead; }
    return PlaceForm::LocalWrite;
}

extern "C" Reference reference_body(Mode mode,u32 reads,u32 atomic,u32 inout,u32 owns) on cpu {
    if (atomic != 0) { return Reference(reads, RefBody::Atomic); }
    if (owns != 0) {
        if (inout != 0) { return Reference(reads, RefBody::OwnedInOut); }
        return Reference(reads, RefBody::OwnedOutput);
    }
    if (reads != 0 && mode.application != 0) { return Reference(reads, RefBody::ReadContent); }
    return Reference(reads, RefBody::Empty);
}

extern "C" u32 start_actions(Mode mode) on cpu {
    return mode.application | (mode.cooperative << 1);
}

extern "C" Prepare prepare(u32 stage,Mode mode) on cpu {
    if (stage == 0) { return Prepare::Elements; }
    if (stage == 1) { return Prepare::BindElements; }
    if (stage == 2) { return Prepare::Resources; }
    if (stage == 3) { return Prepare::Resolve; }
    if (stage == 4) { return Prepare::Assume; }
    if (stage == 5) { return Prepare::Arrays; }
    if (stage == 6) { return Prepare::Outputs; }
    if (stage == 7 && mode.application != 0 && mode.c_abi == 0) { return Prepare::Completions; }
    return Prepare::Done;
}

extern "C" u32 return_task_layout(u32 application_entry,u32 c_abi,u32 has_result) on cpu {
    return application_entry & (1 ^ c_abi) & has_result;
}

extern "C" ReferenceLayout reference_layout(ReferenceFacts facts,usize components,u32 c_abi) on cpu {
    if (c_abi != 0) { return ReferenceLayout(ReferenceForm::Whole,1u64); }
    if (components == 0u64) { return ReferenceLayout(ReferenceForm::Empty,0u64); }
    if (components > 1u64 && facts.array == 0 && facts.record == 0
        && facts.view == 0 && facts.dataset == 0 && facts.variant == 0 && facts.enumeration == 0) {
        return ReferenceLayout(ReferenceForm::Split,components);
    }
    return ReferenceLayout(ReferenceForm::Whole,1u64);
}

extern "C" ProgramStep program_step(ProgramCursor cursor,ProgramFacts facts) on cpu {
 ProgramCursor c=cursor;
 if(c.phase==0){return ProgramStep(ProgramCursor(1,0),ProgramNeed::Next,0);}
 if(facts.present==0){return ProgramStep(c,ProgramNeed::Finish,uniform_bytes(facts.buffers));}
 ResourceStep step=resource_step(c.resource,facts.shape);c.resource=step.next;
 if(step.need==Resource::Buffer){return ProgramStep(c,ProgramNeed::Buffer,0);}
 if(step.need==Resource::Acceleration){return ProgramStep(c,ProgramNeed::Acceleration,0);}
 if(step.need==Resource::Texture){return ProgramStep(c,ProgramNeed::Texture,0);}
 if(step.need==Resource::Keep){return ProgramStep(ProgramCursor(0,0),ProgramNeed::Keep,0);}
 return ProgramStep(ProgramCursor(1,0),ProgramNeed::Next,0);
}

extern "C" BindingStep binding_step(BindingCursor cursor,Mode mode,BindingFacts facts) on cpu {
 BindingCursor c=cursor;
 if(c.phase==0){c.phase=1;if((start_actions(mode)&1)!=0){return BindingStep(c,BindingNeed::Frame);}}
 if(c.phase==1){c.phase=2;return BindingStep(c,BindingNeed::Calls);}
 if(c.phase==2){c.phase=3;if((start_actions(mode)&2)!=0){return BindingStep(c,BindingNeed::Cooperative);}}
 if(c.phase==3){c.phase=4;return BindingStep(c,BindingNeed::Next);}
 if(c.phase==4){
  if(facts.present==0){c.phase=10;c.prepare=0;}
  else{
   Parameter form=parameter_form(facts.shape);c.phase=5;
   if(form==Parameter::Acceleration){return BindingStep(c,BindingNeed::Acceleration);}
   if(form==Parameter::Texture){return BindingStep(c,BindingNeed::Texture);}
   if(form==Parameter::Dataset){return BindingStep(c,BindingNeed::Dataset);}
   c.phase=6;return BindingStep(c,BindingNeed::Type);
  }
 }
 if(c.phase==5){c.phase=7;return BindingStep(c,BindingNeed::Bind);}
 if(c.phase==6){c.phase=7;return BindingStep(c,parameter_form(facts.shape)==Parameter::Tile?BindingNeed::Tile:BindingNeed::Ordinary);}
 if(c.phase==7){
  Parameter form=parameter_form(facts.shape);
  if(form==Parameter::Acceleration){c.acceleration+=1u64;}
  if(form==Parameter::Texture){c.texture+=1u64;}
  if(form==Parameter::Dataset){c.data+=1u64;}
  c.index+=1u64;c.phase=4;return BindingStep(c,BindingNeed::Next);
 }
 Prepare action=prepare(c.prepare,mode);c.prepare+=1;
 if(action==Prepare::Elements){return BindingStep(c,BindingNeed::Elements);}
 if(action==Prepare::BindElements){return BindingStep(c,BindingNeed::BindElements);}
 if(action==Prepare::Resources){return BindingStep(c,BindingNeed::Resources);}
 if(action==Prepare::Resolve){return BindingStep(c,BindingNeed::Resolve);}
 if(action==Prepare::Assume){return BindingStep(c,BindingNeed::Assume);}
 if(action==Prepare::Arrays){return BindingStep(c,BindingNeed::Arrays);}
 if(action==Prepare::Outputs){return BindingStep(c,BindingNeed::Outputs);}
 if(action==Prepare::Completions){return BindingStep(c,BindingNeed::Completions);}
 return BindingStep(c,BindingNeed::Finish);
}
extern "C" void resource_parameters(u64 count,address context,cfn<u32,address,u64> inspect,cfn<void,address,u64> publish) on cpu requires nonzero(inspect) requires nonzero(publish) {
 u64 index=0u64;while(index<count){if(inspect(context,index)!=0){publish(context,index);}index+=1u64;}
}

extern "C" DatasetStep dataset_step(InputCursor cursor,DatasetFacts facts) on cpu {
 InputCursor c=cursor;
 if(c.phase==0){
  Dataset role=dataset_form(facts.ordinary_cpu,facts.by_value);
  if(role==Dataset::Device){c.phase=1;return DatasetStep(c,DatasetNeed::Access);}
  if(role==Dataset::Owned){c.phase=10;return DatasetStep(c,DatasetNeed::Application);}
  c.phase=20;return DatasetStep(c,DatasetNeed::Layout);
 }
 if(c.phase==1){if(facts.inout!=0){c.phase=2;c.index=0u64;return DatasetStep(c,DatasetNeed::Components);}return DatasetStep(c,DatasetNeed::Device);}
 if(c.phase==3){c.index+=1u64;c.phase=2;}
 if(c.phase==2){if(c.index<facts.components){c.phase=3;return DatasetStep(c,DatasetNeed::Initialize);}return DatasetStep(c,DatasetNeed::Device);}
 if(c.phase==10){c.phase=11;return DatasetStep(c,DatasetNeed::OwnedAbi);}
 if(c.phase==11){c.phase=12;return DatasetStep(c,DatasetNeed::Rebound);}
 if(c.phase==12){return DatasetStep(c,DatasetNeed::Owned);}
 if(c.phase==20){c.phase=21;return DatasetStep(c,DatasetNeed::Application);}
 if(c.phase==21){c.phase=22;return DatasetStep(c,DatasetNeed::Registers);}
 if(c.phase==22){c.phase=23;return DatasetStep(c,DatasetNeed::BorrowedAbi);}
 return DatasetStep(c,DatasetNeed::Borrowed);
}

extern "C" OrdinaryStep ordinary_step(InputCursor cursor,Mode mode,CallAccess access,OrdinaryFacts facts) on cpu {
 InputCursor c=cursor;OrdinaryStep result=OrdinaryStep(c,OrdinaryNeed::PlainValue,ReferenceForm::Empty,access==CallAccess::InOut?1:0);
 ValueForm form=value_form(mode,access==CallAccess::Read?1:0,access==CallAccess::Atomic?1:0,access==CallAccess::Value?1:0,facts.owned);
 if(c.phase==0){
  if(form==ValueForm::AtomicParameter){result.need=OrdinaryNeed::AtomicParameter;return result;}
  if(form==ValueForm::Reference){c.phase=20;if(mode.application!=0){result.cursor=c;result.need=OrdinaryNeed::Origin;return result;}}
  else{c.phase=1;c.index=0u64;result.cursor=c;result.need=OrdinaryNeed::Components;return result;}
 }
 if(c.phase==2){c.index+=1u64;c.phase=1;}
 if(c.phase==1){
  if(c.index<facts.components){c.phase=2;result.cursor=c;result.need=OrdinaryNeed::Register;return result;}
  c.phase=3;if(form==ValueForm::OwnedRead||mode.application!=0){result.cursor=c;result.need=OrdinaryNeed::ValueAbi;return result;}
 }
 if(c.phase==3){c.phase=4;result.cursor=c;result.need=OrdinaryNeed::Value;return result;}
 if(c.phase==4){if(facts.owned==0){result.cursor=c;return result;}c.phase=5;result.cursor=c;result.need=OrdinaryNeed::Incoming;return result;}
 if(c.phase==5){c.phase=6;if(form==ValueForm::OwnedRead){result.cursor=c;result.need=OrdinaryNeed::Borrow;return result;}}
 if(c.phase==6){c.phase=7;result.cursor=c;result.need=OrdinaryNeed::Residency;return result;}
 if(c.phase==7){result.cursor=c;result.need=OrdinaryNeed::OwnedValue;return result;}
 if(c.phase==20){
  PlaceForm placement=place_form(mode,access==CallAccess::Read?1:0);c.phase=30;
  if(placement==PlaceForm::LocalRead){result.cursor=c;result.need=OrdinaryNeed::LocalRead;return result;}
  if(placement==PlaceForm::LocalWrite){result.cursor=c;result.need=OrdinaryNeed::LocalWrite;return result;}
  c.phase=21;result.cursor=c;result.need=OrdinaryNeed::ReferenceFacts;return result;
 }
 if(c.phase==21){
  ReferenceLayout layout=reference_layout(facts.reference,facts.components,mode.c_abi);
  if(layout.form==ReferenceForm::Whole){c.phase=22;result.cursor=c;result.need=OrdinaryNeed::ReferenceWhole;return result;}
  if(layout.form==ReferenceForm::Split){c.phase=24;c.index=0u64;}
  else{c.phase=30;result.cursor=c;result.need=OrdinaryNeed::ReferencePlace;return result;}
 }
 if(c.phase==22){c.phase=26;c.index=0u64;}
 if(c.phase==25){c.index+=1u64;c.phase=24;}
 if(c.phase==24){
  result.form=ReferenceForm::Split;
  if(c.index<facts.components){c.phase=25;result.cursor=c;result.need=OrdinaryNeed::ReferenceSplit;return result;}
  c.phase=30;result.cursor=c;result.need=OrdinaryNeed::ReferencePlace;return result;
 }
 if(c.phase==27){c.index+=1u64;c.phase=26;}
 if(c.phase==26){
  result.form=ReferenceForm::Whole;
  if(c.index<facts.components){c.phase=27;result.cursor=c;result.need=OrdinaryNeed::ReferenceWord;return result;}
  c.phase=30;result.cursor=c;result.need=OrdinaryNeed::ReferencePlace;return result;
 }
 Reference decision=reference_body(mode,(access==CallAccess::Read||access==CallAccess::InOut||access==CallAccess::Atomic)?1:0,access==CallAccess::Atomic?1:0,access==CallAccess::InOut?1:0,facts.owned);
 if(c.phase==30){c.phase=31;if(decision.initialize!=0){result.cursor=c;result.need=OrdinaryNeed::Initialize;return result;}}
 if(c.phase==31){
  if(decision.body==RefBody::Atomic){result.cursor=c;result.need=OrdinaryNeed::AtomicReference;return result;}
  if(decision.body==RefBody::OwnedInOut||decision.body==RefBody::OwnedOutput){c.phase=40;result.cursor=c;result.need=OrdinaryNeed::ReferenceType;return result;}
  c.phase=50;result.cursor=c;result.need=decision.body==RefBody::ReadContent?OrdinaryNeed::ContentProof:OrdinaryNeed::EmptyProof;return result;
 }
 if(c.phase==40){c.phase=41;c.index=0u64;result.cursor=c;result.need=OrdinaryNeed::Components;return result;}
 if(c.phase==42){c.index+=1u64;c.phase=41;}
 if(c.phase==41){
  if(c.index<facts.components){c.phase=42;result.cursor=c;result.need=OrdinaryNeed::Register;return result;}
  c.phase=43;result.cursor=c;result.need=decision.body==RefBody::OwnedInOut?OrdinaryNeed::ContentProof:OrdinaryNeed::EmptyProof;return result;
 }
 if(c.phase==43){c.phase=44;result.cursor=c;result.need=OrdinaryNeed::TemplateValue;return result;}
 if(c.phase==44){c.phase=45;result.cursor=c;result.need=OrdinaryNeed::TemplateReturned;return result;}
 if(c.phase==45){c.phase=46;if(decision.body==RefBody::OwnedInOut){result.cursor=c;result.need=OrdinaryNeed::Residency;return result;}}
 if(c.phase==46){result.cursor=c;result.need=OrdinaryNeed::OwnedReference;return result;}
 result.cursor=c;result.need=OrdinaryNeed::Reference;return result;
}

extern "C" CompletionStep completion_step(InputCursor cursor,u64 first,CompletionFacts facts) on cpu {
 CompletionStep result=CompletionStep(cursor,CompletionNeed::Finish,CompletionSource::Input,0u64,0u64);
 if(cursor.phase==0||cursor.phase==2){result.cursor.phase=1;result.need=CompletionNeed::Next;return result;}
 if(facts.present==0){return result;}
 result.cursor.phase=2;result.need=CompletionNeed::Publish;
 if(facts.nested!=0){result.source=CompletionSource::Returned;result.input=first+2u64*facts.parent;result.component=facts.component;}
 else{result.input=facts.input;}
 return result;
}
extern "C" ReturnStep return_step(InputCursor cursor,ReturnFacts facts) on cpu {
 if(cursor.phase==0){
  if(return_task_layout(facts.application,facts.c_abi,facts.result)==0){return ReturnStep(cursor,ReturnNeed::Finish);}
  return ReturnStep(InputCursor(1,0u64),ReturnNeed::Inspect);
 }
 return ReturnStep(cursor,facts.nonempty!=0?ReturnNeed::Publish:ReturnNeed::Finish);
}
