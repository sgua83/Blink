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

    pub(super) async fn result_constructor(
        &mut self,
        ty: Ty,
        success: bool,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        unsafe { self.world.call(self.value_variant_constructor(ty,usize::from(!success),args,span)).await }
    }
    #[inline(never)]
    pub(super) async fn value_variant_constructor(&mut self,ty:Ty,index:usize,args:&Expressions,span:Span)->Result<Option<Value>> {
        unsafe { self.world.call(self.variant_values(ty,index,args.len(),span,async |builder,expected|unsafe { builder.world.call(builder.expr_expected(&args.at(0),Some(expected))).await })).await }
    }
    pub(super) async fn variant_values(&mut self,ty:Ty,index:usize,count:usize,span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self,&Ty)->Result<Option<Value>>)->Result<Option<Value>> {
        let payload=(*ty.variants[index].ty).clone();
        let value = if payload.name == "void" {
            if count!=0 {
                return self.fail(span, "B269", "void 结果分支不接受载荷");
            }
            None
        } else {
            if count != 1 {
                return self.fail(span, "B269", "结果分支需要一个匹配类型的载荷");
            }
            let Some(value) = argument(self,&payload).await? else {
                return Ok(None);
            };
            self.require_same(&payload, &value.ty, span)?;
            Some(value)
        };
        Ok(Some(self.variant_value(ty,index,value)))
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
        let mut value = self.constant_value(ty.clone(), vec![0; ty.components.len()],None);
        let tag = self.constant(ty.components[0].0,Bits::from(ty.variants[index].discriminant));
        value.regs[0] = tag.regs[0];
        value
            .proof
            .assign(&[0], value.regs.len(), bounds::Proof::capture(&tag));
        if let Some(bits) = &mut value.constant {
            bits[0] = Bits::from(ty.variants[index].discriminant);
        }
        if let Some(payload) = payload {
            let offset = ty.variant_payload_offset();
            for (component, (scalar, position)) in ty.components.iter().enumerate().skip(1) {
                let word = self.result_word(&payload, position - offset, *scalar);
                value.regs[component] = word.regs[0];
                value.proof.assign(&[component], value.regs.len(), bounds::Proof::capture(&word));
                if let (Some(bits), Some(source)) = (&mut value.constant, &word.constant) {
                    bits[component] = source[0];
                } else {
                    value.constant = None;
                }
            }
            for component in view_contracts::components(&payload.ty) {
                let word=offset+payload.ty.components[component].1;
                let target=ty.components.iter().position(|(_,at)|*at==word).unwrap();
                value.proof.set_view_borrow(target,payload.proof.view_borrow(component));
            }
            value.retain_payload_nonzero(&payload, offset);
            value.uniform = payload.uniform;
        }
        value.proof.retain_view_borrows(&ty);
        let offset=ty.variant_payload_offset();
        let active=view_contracts::components(&ty.variants[index].ty).into_iter().map(|component|ty.components.iter().position(|(_,at)|*at==offset+ty.variants[index].ty.components[component].1).unwrap()).collect::<BTreeSet<_>>();
        for component in view_contracts::components(&ty).difference(&active){value.proof.set_view_borrow(*component,view::Borrow::source(view::Source::Inactive));}
        value.literal = false;
        value
    }
    pub(super) fn result_payload(&mut self, value: &Value, success: bool) -> Option<Value> {
        self.variant_payload(value,usize::from(!success))
    }
    pub(super) fn variant_payload(&mut self, value: &Value, index: usize) -> Option<Value> {
        let ty = (*value.ty.variants[index].ty).clone();
        if ty.name == "void" {
            return None;
        }
        let offset = value.ty.variant_payload_offset();
        let mut proof = bounds::Proof::default();
        let mut regs = Vec::new();
        let mut constant = Some(Vec::new());
        for (index, (scalar, at)) in ty.components.iter().enumerate() {
            let fresh = self.result_word(value, offset + at, *scalar);
            regs.push(fresh.regs[0]);
            proof.assign(
                &[index],
                ty.components.len(),
                bounds::Proof::capture(&fresh),
            );
            if let (Some(bits), Some(source)) = (&mut constant, fresh.constant) {
                bits.push(source[0]);
            } else {
                constant = None;
            }
        }
        for component in view_contracts::components(&ty) {
            let word=offset+ty.components[component].1;
            let source=value.ty.components.iter().position(|(_,at)|*at==word).unwrap();
            proof.set_view_borrow(component,value.proof.view_borrow(source));
        }
        proof.retain_view_borrows(&ty);
        let proof = self.path_proof(&ty, proof);
        Some(Value {
            ty,
            regs,
            proof,
            literal: false,
            uniform: value.uniform,
            constant,
        })
    }
    pub(super) fn result_condition(&mut self, value: &Value, span: Span) -> Result<Value> {
        self.variant_condition(value,0,span,None)
    }
    pub(super) fn variant_condition(&mut self,value:&Value,index:usize,span:Span,origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        let scalar=value.ty.components[0].0;
        let tag=self.result_word(value,0,scalar);
        let expected=self.constant_with_origin(scalar,Bits::from(value.ty.variants[index].discriminant),origin.clone());
        self.binary("==",tag,expected,span,origin)
    }
    pub(super) async fn check_variant_patterns(
        &self, ty: &Ty, arms: &MatchArms, span: Span,
    ) -> Result<Vec<MatchArm>> {
        if !ty.has_variants() { return self.fail(span, "B269", "match需要带载荷变体或result值"); }
        let mut selected = vec![None; ty.variants.len()];
        for arm in arms {
            let (owner, name) = arm.variant().rsplit_once("::").map_or((None, arm.variant()), |(owner, name)| (Some(owner), name));
            if let Some(owner) = owner {
                let mut syntax = ty.syntax(); syntax.name = owner.into(); syntax.canonical = false;
                let expected = unsafe { self.world.call(self.expected_storage(&syntax, arm.span())).await }?;
                self.require_same(ty, &expected, arm.span())?;
            }
            let Some(index) = ty.variants.iter().position(|v| v.name == name) else { return self.fail(arm.span(),"B269",format!("{}没有变体分支{name}",ty.name)); };
            if selected[index].replace(arm.clone()).is_some() { return self.fail(arm.span(),"B269","同一变体分支不能重复"); }
            let unit = ty.variants[index].ty.name == "void";
            if arm.binding().is_none() && !unit { return self.fail(arm.span(),"B269","有载荷分支需要名称或_模式"); }
            if unit && arm.binding().is_some_and(|name| name!="_") { return self.fail(arm.span(),"B269","无载荷分支没有可绑定的数据"); }
        }
        if selected.iter().any(Option::is_none) { return self.fail(span,"B269","match必须处理声明的全部变体分支"); }
        Ok(selected.into_iter().map(Option::unwrap).collect())
    }
    pub(super) async fn value_variant_arm(&mut self, value: &Value, index: usize, arm: &MatchArm) -> Result<()> {
        let payload = self.variant_payload(value,index);
        if let Some(name) = arm.binding().filter(|name| *name!="_") {
            self.bind_local_value(name,payload.unwrap(),arm.span())?;
            unsafe { self.world.call(self.block(&arm.body())).await }?; self.env.remove(name); Ok(())
        } else { unsafe { self.world.call(self.block(&arm.body())).await } }
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
