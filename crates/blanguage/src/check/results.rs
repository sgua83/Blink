//! 结果使用一个判别字和重叠载荷；分支与传播沿原返回域和资源所有者。
use super::*;

impl Builder<'_> {
    #[inline(never)]
    pub(super) async fn variant_constructor_type(&self,name:&str,explicit:Option<&[GenericArgument]>,expected:Option<&Ty>,span:Span)->Result<Option<(Ty,usize)>> {
        unsafe{self.world.call(self.variant_constructor_type_with(name,expected,span,|parent|{
            explicit.map(|arguments|{let mut syntax=TypeName::named(parent);syntax.arguments=arguments.to_vec();syntax})
        })).await}
    }
    pub(super) async fn variant_constructor_type_with(&self,name:&str,expected:Option<&Ty>,span:Span,
        explicit:impl FnOnce(&str)->Option<TypeName>)->Result<Option<(Ty,usize)>> {
        let Some((parent,member))=name.rsplit_once("::") else{return Ok(None)};
        let Some(key)=self.world.resolve(parent,self.scope.module(),self.scope.imports(),false).unique() else{return Ok(None)};
        if self.world.record(&key).0.variants().is_empty(){return Ok(None);}
        let syntax=explicit(parent).unwrap_or_else(||{
            let mut syntax=TypeName::named(parent);
            if let Some(expected)=expected.filter(|ty|ty.name.split_once('<').map_or(ty.name.as_str(),|(root,_)|root)==key){
                syntax.arguments=expected.syntax().arguments;
            }
            syntax
        });
        let ty=unsafe { self.world.call(self.expected_storage(&syntax,span)).await }?;
        let Some(index)=ty.variants.iter().position(|variant|variant.name==member) else{return self.fail(span,"B269",format!("{}没有变体分支{member}",ty.name));};
        Ok(Some((ty,index)))
    }
    #[inline(never)]
    pub(super) async fn expr_expected(
        &mut self,
        e: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        unsafe{self.world.call(source_call::expected(self,e,expected)).await}
    }

    #[inline(never)]
    pub(super) async fn value_variant_constructor(&mut self,ty:Ty,index:usize,args:&Expressions,span:Span)->Result<Option<Value>> {
        unsafe { self.world.call(self.variant_values(ty,index,args.len(),span,async |builder,expected|unsafe { builder.world.call(builder.expr_expected(&args.at(0),Some(expected))).await })).await }
    }
    pub(super) async fn variant_values(&mut self,ty:Ty,index:usize,count:usize,span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self,&Ty)->Result<Option<Value>>)->Result<Option<Value>> {
        let mut cursor=super::flow::LocalCursor {phase:0};
        let mut facts=VariantBuildFacts {owned:0,cpu:0,application:0,unit:0,arguments:count,continuing:0,present:0};
        let mut argument=Some(argument);let mut value=None;
        loop {
            let step=unsafe {variant_build_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                VariantBuildNeed::Payload=>facts.unit=u32::from(ty.variants[index].ty.name=="void"),
                VariantBuildNeed::VoidArguments=>return self.fail(span,"B269","void 结果分支不接受载荷"),
                VariantBuildNeed::PayloadArguments=>return self.fail(span,"B269","结果分支需要一个匹配类型的载荷"),
                VariantBuildNeed::Evaluate=>{value=argument.take().unwrap()(self,&ty.variants[index].ty).await?;facts.present=u32::from(value.is_some());}
                VariantBuildNeed::Check=>self.require_same(&ty.variants[index].ty,&value.as_ref().unwrap().ty,span)?,
                VariantBuildNeed::Build=>return Ok(Some(self.variant_value(ty,index,value))),
                VariantBuildNeed::Complete=>return Ok(None),
                VariantBuildNeed::InvalidTarget|VariantBuildNeed::Unit|VariantBuildNeed::Missing|VariantBuildNeed::Storage=>unsafe {std::hint::unreachable_unchecked()},
            }
        }
    }
    fn result_component(&mut self,value:&Value,index:usize,scalar:Scalar)->Value {
        let source=value.ty.components[index].0;
        let mut values=WordValues {builder:self,input:value,offset:0,current:None,right:None,joined:None};
        unsafe {lower_variant_component(WordScalar::of(source),WordScalar::of(scalar),index,values.emitter())};
        values.current.unwrap()
    }
    fn result_word(&mut self,value:&Value,offset:u32,scalar:Scalar)->Value {
        let input=WordInput {components:value.ty.components.len(),offset,target:WordScalar::of(scalar)};
        let mut values=WordValues {builder:self,input:value,offset,current:None,right:None,joined:None};
        unsafe {lower_variant_word(input,values.emitter())};
        values.current.unwrap()
    }
    pub(super) fn result_value(&mut self, ty: Ty, success: bool, payload: Option<Value>) -> Value {
        self.variant_value(ty,usize::from(!success),payload)
    }
    pub(super) fn variant_value(&mut self, ty: Ty, index: usize, payload: Option<Value>) -> Value {
        let input=VariantInput::of(&ty,index,ty.components.len(),payload.is_some());
        let mut values=VariantValues {builder:self,mode:VariantMode::Build,input:None,ty:Some(ty),payload,output:None,
            index,offset:input.offset,word:None,expected:None,views:None,active:None,span:None,origin:None,test:None};
        unsafe {build_variant_value(input,values.emitter())};values.output.unwrap()
    }
    pub(super) fn result_payload(&mut self, value: &Value, success: bool) -> Option<Value> {
        self.variant_payload(value,usize::from(!success))
    }
    pub(super) fn variant_payload(&mut self, value: &Value, index: usize) -> Option<Value> {
        let input=VariantInput::of(&value.ty,index,value.ty.variants[index].ty.components.len(),false);
        let mut values=VariantValues {builder:self,mode:VariantMode::Read,input:Some(value),ty:None,payload:None,output:None,
            index,offset:input.offset,word:None,expected:None,views:None,active:None,span:None,origin:None,test:None};
        unsafe {read_variant_payload(input,values.emitter())};values.output
    }
    pub(super) fn result_condition(&mut self, value: &Value, span: Span) -> Result<Value> {
        self.variant_condition(value,0,span,None)
    }
    pub(super) fn variant_condition(&mut self,value:&Value,index:usize,span:Span,origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        let input=VariantInput::of(&value.ty,index,value.ty.components.len(),false);
        let mut values=VariantValues {builder:self,mode:VariantMode::Test,input:Some(value),ty:None,payload:None,output:None,
            index,offset:input.offset,word:None,expected:None,views:None,active:None,span:Some(span),origin,test:None};
        unsafe {test_variant_tag(input,values.emitter())};values.test.unwrap()
    }
    pub(super) async fn check_variant_patterns(&self,ty:&Ty,arms:&MatchArms,span:Span)->Result<Vec<MatchArm>> {
        let mut cursor=PatternCursor {phase:0,index:0};
        let mut facts=PatternFacts {eligible:u32::from(ty.has_variants()),owner:0,found:0,duplicate:0,unit:0,binding:0,named:0,present:0,arms:arms.len(),variants:ty.variants.len()};
        let mut selected=vec![None;ty.variants.len()];let mut arm=None;let mut label=PatternLabel {qualified:0,owner:0,start:0};let mut index=0;
        loop {let step=unsafe {pattern_step(cursor,facts)};cursor=step.cursor;match step.need {
            PatternNeed::InvalidType=>return self.fail(span,"B269","match需要带载荷变体或result值"),
            PatternNeed::Arm=>{let current=arms.at(cursor.index);let spelling=current.variant();label=unsafe {pattern_label(PatternName::of(spelling))};facts.owner=label.qualified;arm=Some(current);},
            PatternNeed::Owner=>{let arm=arm.as_ref().unwrap();let mut syntax=ty.syntax();syntax.name=arm.variant()[..label.owner].into();syntax.canonical=false;let expected=unsafe {self.world.call(self.expected_storage(&syntax,arm.span())).await}?;self.require_same(ty,&expected,arm.span())?;},
            PatternNeed::Find=>{let name=&arm.as_ref().unwrap().variant()[label.start..];index=unsafe {pattern_find(PatternName::of(name),ty.variants.len(),ty,pattern_variant_name)};facts.found=u32::from(index<ty.variants.len());},
            PatternNeed::Unknown=>return self.fail(arm.as_ref().unwrap().span(),"B269",format!("{}没有变体分支{}",ty.name,&arm.as_ref().unwrap().variant()[label.start..])),
            PatternNeed::Select=>{facts.duplicate=u32::from(selected[index].replace(arm.as_ref().unwrap().clone()).is_some());},
            PatternNeed::Duplicate=>return self.fail(arm.as_ref().unwrap().span(),"B269","同一变体分支不能重复"),
            PatternNeed::Inspect=>{let name=arm.as_ref().unwrap().binding();let spelling=name.unwrap_or("");facts.unit=u32::from(ty.variants[index].ty.name=="void");facts.binding=u32::from(name.is_some());facts.named=unsafe {pattern_binding(facts.binding,spelling.as_ptr(),spelling.len())};},
            PatternNeed::MissingBinding=>return self.fail(arm.as_ref().unwrap().span(),"B269","有载荷分支需要名称或_模式"),
            PatternNeed::UnitBinding=>return self.fail(arm.as_ref().unwrap().span(),"B269","无载荷分支没有可绑定的数据"),
            PatternNeed::Coverage=>facts.present=u32::from(selected[cursor.index].is_some()),
            PatternNeed::Incomplete=>return self.fail(span,"B269","match必须处理声明的全部变体分支"),
            PatternNeed::Complete=>return Ok(selected.into_iter().map(Option::unwrap).collect()),
        }}
    }

}


#[repr(C)]
struct WordScalar {code:u32,encoding:crate::builtin_types::ScalarEncoding}
impl WordScalar {fn of(scalar:Scalar)->Self {Self {code:scalar as u32,encoding:scalar.into()}}}
#[repr(C)]
struct WordComponent {scalar:WordScalar,offset:u32}
#[repr(C)]
struct WordConstants {left:Bits,right:Bits,left_present:u32,right_present:u32}
#[repr(C)]
struct WordInput {components:usize,offset:u32,target:WordScalar}
#[repr(u32)]
#[derive(Clone,Copy)]
enum WordOperation {ShiftRight,And,ShiftLeft,Or}
#[repr(u32)]
enum WordAction {BeginComponent,Bitcast,Cast,CastKeepProof,Project,RightConstant,TakeJoined,Emit,Accumulate,TakeAccumulated,Zero,FinalComponent,NonzeroBytes,MarkNonzero}
#[repr(C)]
struct WordStep {action:WordAction,index:usize,scalar:u32,operation:WordOperation,present:u32,bits:Bits}
#[repr(C)]
struct WordEmitter<'a,'w> {
    context:*mut WordValues<'a,'w>,
    component:unsafe extern "C" fn(*const WordValues<'a,'w>,usize)->WordComponent,
    known:unsafe extern "C" fn(*const WordValues<'a,'w>)->WordConstants,
    action:unsafe extern "C" fn(*mut WordValues<'a,'w>,WordStep)->u32,
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::lower_variant_component"]
    fn lower_variant_component(source:WordScalar,target:WordScalar,index:usize,emitter:WordEmitter<'_,'_>);
    #[link_name="bootstrap.operator_types::lower_variant_word"]
    fn lower_variant_word(input:WordInput,emitter:WordEmitter<'_,'_>);
}
struct WordValues<'a,'w> {builder:&'a mut Builder<'w>,input:&'a Value,offset:u32,current:Option<Value>,right:Option<Value>,joined:Option<Value>}
impl<'a,'w> WordValues<'a,'w> {
    fn emitter(&mut self)->WordEmitter<'a,'w> {WordEmitter {context:self,component:word_component,known:word_known,action:word_action}}
}
unsafe extern "C" fn word_component(context:*const WordValues<'_,'_>,index:usize)->WordComponent {
    let input=unsafe {&*context}.input;let(scalar,offset)=input.ty.components[index];WordComponent {scalar:WordScalar::of(scalar),offset}
}
unsafe extern "C" fn word_known(context:*const WordValues<'_,'_>)->WordConstants {
    let values=unsafe {&*context};
    let bits=|value:&Value|value.constant.as_ref().map_or((0,0),|bits|(bits[0],1));
    let(left,left_present)=bits(values.current.as_ref().unwrap());let(right,right_present)=bits(values.right.as_ref().unwrap());
    WordConstants {left,right,left_present,right_present}
}
unsafe extern "C" fn word_action(context:*mut WordValues<'_,'_>,step:WordStep)->u32 {
    let values=unsafe {&mut *context};let scalar=operators::SCALARS[step.scalar as usize];
    match step.action {
        WordAction::BeginComponent=>{
            let input=values.input;let index=step.index;
            values.current=Some(Value {ty:Ty::scalar(scalar),regs:vec![input.regs[index]],proof:input.projected_proof(&[index]),
                uniform:input.uniform,literal:false,constant:input.constant.as_ref().map(|bits|vec![bits[index]])});
        },
        WordAction::Bitcast=>{
            let value=values.current.as_mut().unwrap();let reg=values.builder.reg(scalar);
            values.builder.emit(InstKind::Unary {dst:reg,op:UnaryOp::Bitcast,a:value.regs[0]});
            value.ty=Ty::scalar(scalar);value.regs=vec![reg];value.proof=bounds::Proof::default();
        },
        WordAction::Cast=>values.current=Some(values.builder.cast(values.current.take().unwrap(),scalar)),
        WordAction::CastKeepProof=>{
            let value=values.current.take().unwrap();let proof=bounds::Proof::capture(&value);
            let mut value=values.builder.cast(value,scalar);value.proof=proof;values.current=Some(value);
        },
        WordAction::Project=>values.current=Some(values.builder.result_component(values.input,step.index,scalar)),
        WordAction::RightConstant=>values.right=Some(values.builder.constant(scalar,step.bits)),
        WordAction::TakeJoined=>{values.right=values.joined.take();return u32::from(values.right.is_some());},
        WordAction::Emit=>{
            let word=values.current.take().unwrap();let rhs=values.right.take().unwrap();
            let(name,op)=match step.operation {WordOperation::ShiftRight=>(">>",BinaryOp::ShiftRightLogical),WordOperation::And=>("&",BinaryOp::And),
                WordOperation::ShiftLeft=>("<<",BinaryOp::ShiftLeft),WordOperation::Or=>("|",BinaryOp::Or)};
            let proof=bounds::arithmetic(values.builder,name,&word,&rhs);let dst=values.builder.reg(scalar);
            values.builder.emit(InstKind::Binary {dst,op,a:word.regs[0],b:rhs.regs[0]});
            values.current=Some(Value {ty:Ty::scalar(scalar),regs:vec![dst],proof,uniform:word.uniform&&rhs.uniform,literal:false,
                constant:(step.present!=0).then(||vec![step.bits])});
        },
        WordAction::Accumulate=>values.joined=values.current.take(),
        WordAction::TakeAccumulated=>{values.current=values.joined.take();return u32::from(values.current.is_some());},
        WordAction::Zero=>values.current=Some(values.builder.constant(scalar,0)),
        WordAction::FinalComponent=>values.current=Some(values.builder.result_component(values.current.as_ref().unwrap(),0,scalar)),
        WordAction::NonzeroBytes=>return u32::from(values.input.nonzero_bytes(values.offset,scalar.size(),&values.builder.residency_path())),
        WordAction::MarkNonzero=>values.current.as_mut().unwrap().proof.mark_nonzero(0),
    }1
}


#[repr(C)]
struct VariantInput {components:usize,offset:u32,payload:u32,unit:u32,scalar:u32,discriminant:Bits}
impl VariantInput {
    fn of(ty:&Ty,index:usize,components:usize,payload:bool)->Self {
        Self {components,offset:ty.variant_payload_offset(),payload:u32::from(payload),unit:u32::from(ty.variants[index].ty.name=="void"),
            scalar:ty.components[0].0 as u32,discriminant:Bits::from(ty.variants[index].discriminant)}
    }
}
#[repr(u32)]
#[derive(Clone,Copy)]
enum VariantSide {Storage,Payload,Selected}
#[repr(C)]
struct VariantField {scalar:u32,offset:u32}
#[repr(C)]
struct VariantWord {bits:Bits,constant:u32}
#[repr(C)]
struct VariantView {component:usize,present:u32}
#[repr(u32)]
enum VariantAction {InitializeBuild,Tag,StoreBuild,PayloadFacts,RetainBuild,StartActive,AddActive,Inactive,EndActive,
    InitializeRead,StoreRead,Borrow,FinishRead,ExpectedTag,CompareTag}
#[repr(C)]
struct VariantStep {action:VariantAction,component:usize,other:usize,scalar:u32,offset:u32,known:u32,bits:Bits}
#[repr(C)]
struct VariantEmitter<'a,'w> {
    context:*mut VariantValues<'a,'w>,
    field:unsafe extern "C" fn(*const VariantValues<'a,'w>,VariantSide,usize)->VariantField,
    load:unsafe extern "C" fn(*mut VariantValues<'a,'w>,u32,u32)->VariantWord,
    views:unsafe extern "C" fn(*mut VariantValues<'a,'w>,VariantSide),
    next:unsafe extern "C" fn(*mut VariantValues<'a,'w>)->VariantView,
    active:unsafe extern "C" fn(*const VariantValues<'a,'w>,usize)->u32,
    action:unsafe extern "C" fn(*mut VariantValues<'a,'w>,VariantStep),
}
unsafe extern "C" {
    #[link_name="bootstrap.operator_types::build_variant_value"] fn build_variant_value(input:VariantInput,emitter:VariantEmitter<'_,'_>);
    #[link_name="bootstrap.operator_types::read_variant_payload"] fn read_variant_payload(input:VariantInput,emitter:VariantEmitter<'_,'_>);
    #[link_name="bootstrap.operator_types::test_variant_tag"] fn test_variant_tag(input:VariantInput,emitter:VariantEmitter<'_,'_>);
}
enum VariantMode {Build,Read,Test}
struct VariantValues<'a,'w> {
    builder:&'a mut Builder<'w>,mode:VariantMode,input:Option<&'a Value>,ty:Option<Ty>,payload:Option<Value>,output:Option<Value>,
    index:usize,offset:u32,word:Option<Value>,expected:Option<Value>,views:Option<std::collections::btree_set::IntoIter<usize>>,
    active:Option<BTreeSet<usize>>,span:Option<Span>,origin:Option<Arc<GeneratedOrigin>>,test:Option<Result<Value>>,
}
impl<'a,'w> VariantValues<'a,'w> {
    fn storage(&self)->&Ty {match self.mode {VariantMode::Build=>&self.output.as_ref().unwrap().ty,VariantMode::Read|VariantMode::Test=>&self.input.unwrap().ty}}
    fn value_type(&self,side:VariantSide)->&Ty {
        match side {
            VariantSide::Storage=>self.storage(),VariantSide::Selected=>&self.storage().variants[self.index].ty,
            VariantSide::Payload=>match self.mode {VariantMode::Build=>&self.payload.as_ref().unwrap().ty,VariantMode::Read=>&self.output.as_ref().unwrap().ty,VariantMode::Test=>&self.input.unwrap().ty.variants[self.index].ty},
        }
    }
    fn emitter(&mut self)->VariantEmitter<'a,'w> {VariantEmitter {context:self,field:variant_field,load:variant_load,views:variant_views,next:variant_next,active:variant_active,action:variant_action}}
}
unsafe extern "C" fn variant_field(context:*const VariantValues<'_,'_>,side:VariantSide,index:usize)->VariantField {
    let(scalar,offset)=unsafe {&*context}.value_type(side).components[index];VariantField {scalar:scalar as u32,offset}
}
unsafe extern "C" fn variant_load(context:*mut VariantValues<'_,'_>,offset:u32,scalar:u32)->VariantWord {
    let values=unsafe {&mut *context};let source=match values.mode {VariantMode::Build=>values.payload.as_ref().unwrap(),VariantMode::Read|VariantMode::Test=>values.input.unwrap()};
    let word=values.builder.result_word(source,offset,operators::SCALARS[scalar as usize]);
    let(bits,constant)=word.constant.as_ref().map_or((0,0),|bits|(bits[0],1));values.word=Some(word);VariantWord {bits,constant}
}
unsafe extern "C" fn variant_views(context:*mut VariantValues<'_,'_>,side:VariantSide) {
    let values=unsafe {&mut *context};values.views=Some(view_contracts::components(values.value_type(side)).into_iter());
}
unsafe extern "C" fn variant_next(context:*mut VariantValues<'_,'_>)->VariantView {
    let values=unsafe {&mut *context};match values.views.as_mut().unwrap().next() {
        Some(component)=>VariantView {component,present:1},None=>{values.views=None;VariantView {component:0,present:0}},
    }
}
unsafe extern "C" fn variant_active(context:*const VariantValues<'_,'_>,index:usize)->u32 {u32::from(unsafe {&*context}.active.as_ref().unwrap().contains(&index))}
unsafe extern "C" fn variant_action(context:*mut VariantValues<'_,'_>,step:VariantStep) {
    let values=unsafe {&mut *context};
    match step.action {
        VariantAction::InitializeBuild=>{
            let ty=values.ty.take().unwrap();let bits=vec![0;ty.components.len()];values.output=Some(values.builder.constant_value(ty,bits,None));
        },
        VariantAction::Tag=>{
            let tag=values.builder.constant(operators::SCALARS[step.scalar as usize],step.bits);let output=values.output.as_mut().unwrap();
            output.regs[0]=tag.regs[0];output.proof.assign(&[0],output.regs.len(),bounds::Proof::capture(&tag));output.constant.as_mut().unwrap()[0]=step.bits;
        },
        VariantAction::StoreBuild=>{
            let word=values.word.take().unwrap();let output=values.output.as_mut().unwrap();output.regs[step.component]=word.regs[0];
            output.proof.assign(&[step.component],output.regs.len(),bounds::Proof::capture(&word));
            if step.known!=0 {output.constant.as_mut().unwrap()[step.component]=step.bits;}else{output.constant=None;}
        },
        VariantAction::PayloadFacts=>{let payload=values.payload.take().unwrap();let output=values.output.as_mut().unwrap();output.retain_payload_nonzero(&payload,values.offset);output.uniform=payload.uniform;},
        VariantAction::RetainBuild=>{let output=values.output.as_mut().unwrap();output.proof.retain_view_borrows(&output.ty);},
        VariantAction::StartActive=>values.active=Some(BTreeSet::new()),
        VariantAction::AddActive=>{values.active.as_mut().unwrap().insert(step.component);},
        VariantAction::Inactive=>values.output.as_mut().unwrap().proof.set_view_borrow(step.component,view::Borrow::source(view::Source::Inactive)),
        VariantAction::EndActive=>values.active=None,
        VariantAction::InitializeRead=>{
            let input=values.input.unwrap();let ty=(*input.ty.variants[values.index].ty).clone();values.output=Some(Value {ty,proof:bounds::Proof::default(),regs:Vec::new(),constant:Some(Vec::new()),literal:false,uniform:input.uniform});
        },
        VariantAction::StoreRead=>{
            let word=values.word.take().unwrap();let output=values.output.as_mut().unwrap();output.regs.push(word.regs[0]);
            output.proof.assign(&[step.component],output.ty.components.len(),bounds::Proof::capture(&word));
            if step.known!=0 {output.constant.as_mut().unwrap().push(step.bits);}else{output.constant=None;}
        },
        VariantAction::Borrow=>{
            let source=match values.mode {VariantMode::Build=>values.payload.as_ref().unwrap(),VariantMode::Read|VariantMode::Test=>values.input.unwrap()};
            values.output.as_mut().unwrap().proof.set_view_borrow(step.component,source.proof.view_borrow(step.other));
        },
        VariantAction::FinishRead=>{
            let output=values.output.as_mut().unwrap();output.proof.retain_view_borrows(&output.ty);
            output.proof=values.builder.path_proof(&output.ty,std::mem::take(&mut output.proof));
        },
        VariantAction::ExpectedTag=>values.expected=Some(values.builder.constant_with_origin(operators::SCALARS[step.scalar as usize],step.bits,values.origin.clone())),
        VariantAction::CompareTag=>values.test=Some(values.builder.binary("==",values.word.take().unwrap(),values.expected.take().unwrap(),values.span.unwrap(),values.origin.take())),
    }
}

#[repr(C)]#[derive(Clone,Copy)]struct PatternCursor {phase:u32,index:usize}
#[repr(C)]#[derive(Clone,Copy)]struct PatternName {data:*const u8,length:usize}
impl PatternName {fn of(name:&str)->Self {Self {data:name.as_ptr(),length:name.len()}}}
#[repr(C)]#[derive(Clone,Copy)]struct PatternLabel {qualified:u32,owner:usize,start:usize}
#[repr(C)]#[derive(Clone,Copy)]struct PatternFacts {eligible:u32,owner:u32,found:u32,duplicate:u32,unit:u32,binding:u32,named:u32,present:u32,arms:usize,variants:usize}
#[repr(u32)]enum PatternNeed {InvalidType,Arm,Owner,Find,Unknown,Select,Duplicate,Inspect,MissingBinding,UnitBinding,Coverage,Incomplete,Complete}
#[repr(C)]struct PatternStep {cursor:PatternCursor,need:PatternNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::pattern_label"]fn pattern_label(spelling:PatternName)->PatternLabel;
 #[link_name="bootstrap.function_body::pattern_find"]fn pattern_find(name:PatternName,count:usize,context:*const Ty,variant:unsafe extern "C" fn(*const Ty,usize)->PatternName)->usize;
 #[link_name="bootstrap.function_body::match_binding"]fn pattern_binding(present:u32,data:*const u8,length:usize)->u32;
 #[link_name="bootstrap.function_body::pattern_step"]fn pattern_step(cursor:PatternCursor,facts:PatternFacts)->PatternStep;
}
unsafe extern "C" fn pattern_variant_name(context:*const Ty,index:usize)->PatternName {PatternName::of(&unsafe {&*context}.variants[index].name)}

#[repr(C)]#[derive(Clone,Copy)]pub(super) struct VariantBuildFacts {pub owned:u32,pub cpu:u32,pub application:u32,pub unit:u32,pub arguments:usize,pub continuing:u32,pub present:u32}
#[repr(u32)]pub(super) enum VariantBuildNeed {InvalidTarget,Payload,VoidArguments,PayloadArguments,Unit,Evaluate,Missing,Check,Storage,Build,Complete}
#[repr(C)]pub(super) struct VariantBuildStep {pub cursor:super::flow::LocalCursor,pub need:VariantBuildNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::variant_build_step"]pub(super) fn variant_build_step(cursor:super::flow::LocalCursor,facts:VariantBuildFacts)->VariantBuildStep;
}
