//! 拥有值沿原 Frame 持有资源；记录按字段组合，结果判别实际交付或释放的载荷。
use super::*;
#[path="owned_values/model.rs"]
mod model;
pub(super) use model::{live_data_at, owned_data_at, owned_data_at_mut};
pub(super) mod storage;
mod release;
use application::{Data, DataPayload, Ownership};
use returns::Returned;

// 拥有聚合保持原资源树；间接持有其 CPU 元数据，不扩大设备表达式栈帧。
pub(super) type OwnedValue = Box<OwnedValuePayload>;
#[derive(Clone)]
pub(super) struct OwnedValuePayload {
    pub value: Value,
    pub payloads: Vec<Option<Box<Returned>>>,
    pub ownership: Ownership,
    pub place: Option<Place>,
    pub declaration: usize,
}
impl OwnedValuePayload {
    pub(super) fn place(&self)->Option<Place>{self.place.clone()}
    pub(super) fn join_with_presence(
        &mut self, other: OwnedValue, stored: bool, choice: Option<&bounds::Condition>,
        presence: Option<&owned_presence::Presence>,
    ) {
        model::join(self, other, stored, choice, presence);
    }
    pub(super) fn all_available(&self)->bool{model::all_available(self)}
    pub(super) fn available(&self)->bool{model::available(self)}
    pub(super) fn borrow(&mut self){model::borrow(self)}
    pub(super) fn incomplete(&self)->bool{model::incomplete(self)}
    pub(super) async fn loop_compatible(&self,other:&Self,builder:&mut Builder<'_>,span:Span,path:&bounds::Clause,inputs:&BTreeMap<ValueIdentity,bounds::Proof>)->Result<bool>{model::loop_compatible(self,other,builder,span,path,inputs).await}
    pub(super) fn invalidate(&mut self){model::invalidate(self)}
    pub(super) fn invalidate_components(&mut self,components:&[usize],borrowed:Option<ValueIdentity>){model::invalidate_components(self,components,borrowed)}
    pub(super) fn mark_moved(&mut self){model::mark_moved(self)}
    pub(super) fn handles(&self)->Vec<ValueIdentity>{model::handles(self)}
    pub(super) fn resources(&self)->Vec<&Data>{model::resources(self)}
    pub(super) fn visit_resources(&self,visit:&mut impl FnMut(&Data)){model::visit_resources(self,visit)}
    pub(super) fn visit_live_paths(
        &self, visit: impl FnMut(&[usize], &Data, bool) -> std::ops::ControlFlow<()>,
    ) -> std::ops::ControlFlow<()> {
        model::visit_live_paths(self, visit)
    }
    pub(super) fn resources_mut(&mut self,visit:&mut impl FnMut(&mut Data)){model::resources_mut(self,visit)}
}

pub(super) enum StorageLayout {
    Value(Ty),
    Dataset { element: Ty, count: Option<u32> },
}

impl StorageLayout {
    async fn resolve(
        world: &World, ty: &TypeName, context: (&Source, &str, &[String]), span: Span,
    ) -> Result<Self> {
        let (source, module, imports) = context;
        let value = unsafe { world.call(world.ty(ty, source, span, module, imports)).await }?;
        if ty.dataset {
            let count = unsafe { world.call(world.shape(&ty.count, source, span, module, imports)).await }?;
            Ok(Self::Dataset { element: value, count })
        } else {
            Ok(Self::Value(value))
        }
    }

    pub(super) fn storage(&self) -> std::borrow::Cow<'_, Ty> {
        match self {
            Self::Value(ty) => std::borrow::Cow::Borrowed(ty),
            Self::Dataset { element, count } => std::borrow::Cow::Owned(Ty::owned_dataset(element.syntax(), *count)),
        }
    }
}

pub(super) async fn storage_type(world:&World,ty:&TypeName,context:(&Source,&str,&[String]),span:Span)->Result<Ty> {
    let layout = unsafe { world.call(StorageLayout::resolve(world, ty, context, span)).await }?;
    Ok(match layout {
        StorageLayout::Value(value) => value,
        dataset => dataset.storage().into_owned(),
    })
}

impl Builder<'_> {
    pub(super) async fn expected_storage(&self,ty:&TypeName,span:Span)->Result<Ty> {
        unsafe { self.world.call(storage_type(self.world,ty,(self.scope.source(),self.scope.module(),self.scope.imports()),span)).await }
    }

    pub(super) async fn storage_layout(&self, ty: &TypeName, span: Span) -> Result<StorageLayout> {
        let context = (self.scope.source(), self.scope.module(), self.scope.imports());
        unsafe { self.world.call(StorageLayout::resolve(self.world, ty, context, span)).await }
    }

    pub(super) async fn check_returned_type(
        &self,
        value: &mut Returned,
        expected: &Ty,
        span: Span,
    ) -> Result<()> {
        match value {
            Returned::Dataset(data)=>{if self.application_initialized(data){data.initialization=None;}},
            Returned::OwnedValue(value)=>value.resources_mut(&mut |data|{if self.application_initialized(data){data.initialization=None;}}),
            _=>{},
        }
        match value {
            Returned::Dataset(data) if expected.dataset.is_some() => {
                let count = expected.values.first().copied();
                let element=unsafe { self.world.call(self.expected_storage(expected.dataset.as_deref().unwrap(),span)).await }?;
                self.application_data_type(
                    data,
                    &element,
                    count,
                    span,
                )?;
                data.count = count;
                Ok(())
            }
            Returned::OwnedValue(result) if expected.owns_resources() => {
                self.require_same(expected, &result.value.ty, span)
            }
            Returned::Value(value) if !expected.owns_resources() => {
                self.require_same(expected, &value.ty, span)
            }
            Returned::Unit if expected.name == "void" => Ok(()),
            _ => self.fail(
                span,
                "B224",
                "普通值与拥有资源不能互换，结果交接需要相同载荷与所有权类型",
            ),
        }
    }
    pub(super) async fn require_returned_initialized(&mut self,value:&mut Returned,span:Span)->Result<()> {
        match value {
            Returned::Dataset(data)=>{unsafe { self.world.call(self.require_application_initialized(data,span)).await }?;data.initialization=None;},
            Returned::OwnedValue(value)=>{
                for data in value.resources(){unsafe { self.world.call(self.require_application_initialized(data,span)).await }?;}
                value.resources_mut(&mut |data|data.initialization=None);
            },
            _=>{},
        }
        Ok(())
    }
    pub(super) fn value_component(&self, value: &Value, index: usize) -> Value {
        Value {
            ty: Ty::scalar(value.ty.components[index].0),
            regs: vec![value.regs[index]],
            proof: value.projected_proof(&[index]),
            uniform: value.uniform,
            literal: false,
            constant: value.constant.as_ref().map(|v| vec![v[index]]),
        }
    }
    pub(super) async fn incoming_owned_value(&mut self, mut value: Value) -> Result<OwnedValue> {
        value.proof=bounds::Proof::capture(&value);
        let Returned::OwnedValue(result) = unsafe { self.world.call(self.template_returned(value, true)).await }? else {
            unreachable!()
        };
        Ok(result)
    }
    pub(super) async fn read_aggregate(&mut self,expr:&Expr,expected:Option<&Ty>)->Result<Option<(Returned,Option<String>)>> {
        if let ExprKind::Name(name)=expr.kind()
            && let Some(Binding::Owned(owner))=self.env.get(name).cloned()
            && matches!(owner.place.storage(),Place::ApplicationElement{..})
            && owner.place.ty().owns_resources() && owner.place.ty().dataset.is_none() {
            let Returned::OwnedValue(mut value)=unsafe { self.world.call(self.application_owned_argument(&owner.place,true,expr.span())).await }? else{return self.fail(expr.span(),"B224","read聚合需要拥有记录、数组或变体")};
            if !value.available(){return self.fail(expr.span(),"B227","只读借用需要已初始化且未移动的全部活动资源");}
            self.prepare_owned_value(&value);value.borrow();
            return Ok(Some((Returned::OwnedValue(value),None)));
        }
        if self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()){
            let Some(place)=unsafe { self.world.call(self.place(expr)).await }?else{return Ok(None)};let model=self.owned_place_model(&place);let Returned::OwnedValue(mut value)=model else{return self.fail(expr.span(),"B224","read聚合需要拥有记录、数组或变体")};
            if !value.available(){return self.fail(expr.span(),"B227","只读借用需要已初始化且未移动的全部活动资源");}self.prepare_owned_value(&value);if !value.value.ty.has_variants(){unsafe { self.world.call(self.require_initialized(&place,expr.span())).await }?;}unsafe { self.world.call(self.load_owned_value(&mut value,expr.span())).await }?;value.borrow();return Ok(Some((Returned::OwnedValue(value),None)));
        }
        if let Some(projection)=self.owned_projection(expr) {
            let Returned::OwnedValue(mut value)=unsafe { self.world.call(self.projected_owned(&projection,expr.span())).await }? else{return self.fail(expr.span(),"B224","read聚合需要拥有记录、数组或变体")};
            if !value.available(){return self.fail(expr.span(),"B227","只读借用需要已初始化且未移动的全部活动资源");}
            if !value.value.ty.has_variants() && let Some(place)=value.place(){unsafe { self.world.call(self.require_initialized(&place,expr.span())).await }?;}
            unsafe { self.world.call(self.load_owned_value(&mut value,expr.span())).await }?;value.borrow();return Ok(Some((Returned::OwnedValue(value),None)));
        }
        if let ExprKind::Member(base,field) = expr.kind() {
            let Some((value,owner))=unsafe { self.world.call(self.read_aggregate(&base,None)).await }? else{return Ok(None)};
            let item=match value {
                Returned::OwnedValue(value)=>{
                    let index=value.value.ty.fields.iter().position(|(name,_,_)|name==field).ok_or_else(||Diagnostic{source:self.scope.source().clone(),span:expr.span(),code:"B141",message:"拥有值字段不存在".into()})?;
                    self.unpack_record(&value,index)
                }
                Returned::Value(value)=>Returned::value(self.member_value(*value,field,expr.span())?),
                Returned::Dataset(data) if field=="length"=>Returned::value(data.length),
                _=>return self.fail(expr.span(),"B141","read字段需要实际记录或普通值"),
            };
            if let Returned::Value(value)=&item {
                let retained=value.proof.typed_view_borrow(&value.ty);
                if retained.empty() && let Some(owner)=owner {
                    let binding=self.env.remove(&owner).unwrap();
                    unsafe { self.world.call(self.drop_binding(binding,&retained,expr.span())).await }?;
                }
                return Ok(Some((item,None)));
            }
            return Ok(Some((item,owner)));
        }
        if let ExprKind::Index(base,index) = expr.kind() {
            if let Some(data)=unsafe { self.world.call(self.application_reference(&base)).await }? {
                let Some(place)=unsafe { self.world.call(self.application_index(data.clone(),&index)).await }?else{return Ok(None)};
                if !place.ty().owns_resources(){return unsafe { self.world.call(self.read_place(&place,expr.span(), None)).await }.map(|value|Some((Returned::value(value),None)));}
                let packed=unsafe { self.world.call(self.read_storage_value(&place,expr.span(), None)).await }?;
                let Returned::OwnedValue(mut value)=unsafe { self.world.call(self.materialize_element(&data,Some(packed),&place)).await }? else{unreachable!()};
                value.borrow();value.place=Some(place);return Ok(Some((Returned::OwnedValue(value),None)));
            }
            let Some((value,mut owner))=unsafe { self.world.call(self.read_aggregate(&base,None)).await }? else{return Ok(None)};
            let value=match value {
                Returned::Value(value)=>{
                    let value=if value.ty.indexed_count().is_some(){let Some(value)=unsafe { self.world.call(self.array_value_index(*value,&index)).await }?else{return Ok(None)};value}
                        else {let Some(place)=unsafe { self.world.call(self.view_value_place(*value,&index)).await }?else{return Ok(None)};unsafe { self.world.call(self.read_place(&place,expr.span(), None)).await }?};
                    return Ok(Some((Returned::value(value),owner)));
                }
                Returned::Dataset(data)=>{
                    let data=application::DataReference::direct(data);
                    let Some(place)=unsafe { self.world.call(self.application_index(data.clone(),&index)).await }?else{return Ok(None)};
                    if !place.ty().owns_resources(){return unsafe { self.world.call(self.read_place(&place,expr.span(), None)).await }.map(|value|Some((Returned::value(value),owner)));}
                    let packed=unsafe { self.world.call(self.read_storage_value(&place,expr.span(), None)).await }?;
                    let Returned::OwnedValue(mut value)=unsafe { self.world.call(self.materialize_element(&data,Some(packed),&place)).await }?else{unreachable!()};
                    value.borrow();value.place=Some(place);return Ok(Some((Returned::OwnedValue(value),owner)));
                }
                Returned::OwnedValue(value)=>value,
                Returned::Unit=>return self.fail(expr.span(),"B224","read下标需要实际数组或存储"),
            };
            let Some(element)=value.value.ty.array.as_deref().cloned()else{return self.fail(expr.span(),"B224","聚合只读下标需要固定拥有数组")};
            if owner.is_none()&&value.place.is_none(){owner=Some(self.hold_binding(Binding::OwnedValue(value.clone())));}
            let Some(index)=unsafe { self.world.call(self.expr(&index)).await }? else{return Ok(None)};
            self.require_same(&Ty::scalar(Scalar::U32),&index.ty,expr.span())?;
            let count=value.value.ty.values[0];
            if !self.index_in(&index,IndexDomain::Fixed(count),count,None){return self.fail(expr.span(),"B137","拥有数组下标有效性无法静态证明")};
            let mut item=if index.minimum()==index.maximum(){self.unpack_record(&value,index.minimum()as usize)}else{
                let mut selected=(0..count).filter(|i|*i>=index.minimum()&&*i<=index.maximum());
                let mut model=self.unpack_record(&value,selected.next().unwrap()as usize);
                for i in selected{model.join(self.unpack_record(&value,i as usize),false,None);}
                let place=match value.place(){Some(place)=>place,None=>self.materialize_array(value.value.clone(),None).place};let place=self.array_index(place,index,expr.span())?;
                let mut regs=Vec::new();
                for(i,(scalar,_))in element.components.iter().enumerate(){regs.push(unsafe { self.world.call(self.read_place(&place.clone().project(Ty::scalar(*scalar),&[i]),expr.span(), None)).await }?.regs[0]);}
                self.rebind_returned(&mut model,regs);
                if let Returned::OwnedValue(value)=&mut model{value.place=Some(place);}model
            };
            if let Returned::OwnedValue(value)=&mut item {value.borrow();}
            return Ok(Some((item,owner)));
        }
        let returned=unsafe { self.world.call(self.evaluate_returned(expr,expected)).await }?;
        if !self.flow.continues(){return Ok(None);}
        match returned {
            Some(Returned::OwnedValue(mut value))=>{
                if !value.available(){return self.fail(expr.span(),"B227","只读借用需要已初始化且未移动的全部活动资源");}
                let owner=self.hold_binding(Binding::OwnedValue(value.clone()));value.borrow();Ok(Some((Returned::OwnedValue(value),Some(owner))))
            }
            Some(Returned::Dataset(mut data))=>{let owner=self.hold_binding(Binding::ApplicationData(data.clone()));data.ownership=Ownership::Borrowed;Ok(Some((Returned::Dataset(data),Some(owner))))}
            Some(Returned::Value(value))=>Ok(Some((Returned::Value(value),None))),
            _=>self.fail(expr.span(),"B224","read需要实际普通值或拥有聚合"),
        }
    }
    // 资源只读取实际位置中的元数据；载荷仍由原Frame持有。
    pub(super) async fn read_storage_value(&mut self,place:&Place,span:Span,element_read_origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        let ty=place.ty().clone();let mut regs=Vec::new();
        for(index,(scalar,_))in ty.components.iter().enumerate(){regs.push(unsafe { self.world.call(self.read_place(&place.clone().project(Ty::scalar(*scalar),&[index]),span,element_read_origin.clone())).await }?.regs[0]);}
        let (proof, uniform, _)=unsafe { self.world.call(self.read_owned_proof(place, &regs, &[])).await }?;
        Ok(Value{ty,regs,proof,uniform,literal:false,constant:None})
    }
    pub(super) async fn write_storage_value(&mut self,place:&Place,value:&Value,span:Span,local_origin:Option<Arc<GeneratedOrigin>>)->Result<()> {
        for(index,(scalar,_))in value.ty.components.iter().enumerate(){unsafe { self.world.call(self.write_place(&place.clone().project(Ty::scalar(*scalar),&[index]),self.value_component(value,index),span,local_origin.clone())).await }?;}
        Ok(())
    }
    pub(super) async fn load_owned_value(&mut self,result:&mut OwnedValue,span:Span)->Result<()> {
        if let Some(place)=result.place(){let value=unsafe { self.world.call(self.read_storage_value(&place,span, None)).await }?;result.value.regs=value.regs;result.value.proof=bounds::Proof::capture(&result.value);}
        Ok(())
    }
    #[inline(never)]
    pub(super) async fn take_owned_value(&mut self, name: &str, span: Span) -> Result<OwnedValue> {
        let Some(Binding::OwnedValue(result)) = self.env.get(name).cloned() else {
            unreachable!()
        };
        let mut model=Returned::OwnedValue(result);self.resolve_owned_model(&mut model,None);let Returned::OwnedValue(mut result)=model else{unreachable!()};
        // 元数据移动不迁移Frame存储；借用在实际调用、改写和释放处交接。
        if result
            .handles()
            .iter()
            .any(|id| self.application_position_borrowed(*id))
        {
            return self.fail(span, "B227", "资源字段仍被游标或位置借用，不能移动拥有值");
        }
        if !result.all_available() {
            return self.fail(span, "B227", "拥有值存在未初始化或已移动的资源字段");
        }
        self.prepare_owned_value(&result);
        if let Some(place)=result.place() {
            unsafe { self.world.call(self.require_place_access(&place,Access::Write,span)).await }?;
            if !result.value.ty.has_variants() {
                unsafe { self.world.call(self.require_initialized(&place,span)).await }?;
            }
        }
        unsafe { self.world.call(self.load_owned_value(&mut result,span)).await }?;
        self.forget_owned_value(&result);
        if let Some(Binding::OwnedValue(source)) = self.env.get_mut(name) {
            source.mark_moved();
        }
        result.place = None;
        Ok(result)
    }
    pub(super) fn owned_value_binding(
        &mut self,
        mut result: OwnedValue,
        mutable: bool,
    ) -> OwnedValue {
        if result.place.is_none() && (mutable || result.value.ty.contains_arrays()) {
            let(slots,storage)=self.local_slots(&result.value.ty);
            for(slot,reg)in slots.iter().zip(&result.value.regs){self.emit(InstKind::StoreLocal(*slot,*reg));}
            result.place=Some(Place::Local(result.value.ty.clone(),slots,storage));
        }
        if let Some(place) = result.place() {
            self.initialize(&place);
        }
        result
    }
    pub(super) fn bind_owned_value(
        &mut self,
        name: &str,
        result: OwnedValue,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        let mutable = self.scope.body().is_some_and(|body| application::rebound(&body, name))
            || !result.value.ty.has_variants();
        let mut result = self.owned_value_binding(result, mutable);
        result.declaration=span.start;
        self.env.insert(name.to_string(), Binding::OwnedValue(result));
        Ok(())
    }
    pub(super) async fn owned_value_declare(&mut self, name: &str, ty: Ty, span: Span) -> Result<()> {
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span, "B224", "拥有结果属于普通CPU应用资源组织");
        }
        let regs = ty.components.iter().map(|(s, _)| self.reg(*s)).collect();
        let value = Value {
            ty,
            regs,
            proof: bounds::Proof::default(),
            constant: None,
            uniform: false,
            literal: false,
        };
        let mut result = if value.ty.has_variants() {
            let payloads=vec![None;value.ty.variants.len()];
            Box::new(OwnedValuePayload {
                value,
                payloads,
                ownership: Ownership::Moved,
                place: None, declaration: 0,
            })
        } else {
            let Returned::OwnedValue(v) = unsafe { self.world.call(self.template_returned(value, false)).await }? else {
                unreachable!()
            };
            v
        };
        let(slots,storage)=self.local_slots(&result.value.ty);
        result.place=Some(Place::Local(result.value.ty.clone(),slots,storage));
        result.declaration=span.start;
        self.env.insert(name.to_string(), Binding::OwnedValue(result));
        Ok(())
    }
    #[inline(never)]
    pub(super) async fn make_owned_result(
        &mut self,
        ty: Ty,
        success: bool,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<OwnedValue>> {
        unsafe { self.world.call(self.make_owned_variant(ty,usize::from(!success),args,span)).await }
    }
    #[inline(never)]
    pub(super) async fn make_owned_variant(&mut self,ty:Ty,index:usize,args:&Expressions,span:Span)->Result<Option<OwnedValue>> {
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span,"B224","拥有变体属于普通CPU应用资源组织");
        }
        let expected=(*ty.variants[index].ty).clone();
        let item = if expected.name == "void" {
            if !args.is_empty() {
                return self.fail(span, "B269", "void分支没有载荷");
            }
            Returned::Unit
        } else {
            if args.len() != 1 {
                return self.fail(span, "B269", "结果分支需要一个载荷");
            }
            let result = unsafe { self.world.call(self.evaluate_returned(&args.at(0), Some(&expected))).await }?;
            if !self.flow.continues() {
                return Ok(None);
            }
            let mut value = result.ok_or_else(|| Diagnostic {
                source: self.scope.source().clone(),
                span,
                code: "B228",
                message: "结果分支需要载荷".into(),
            })?;
            unsafe { self.world.call(self.check_returned_type(&mut value, &expected, span)).await }?;
            value
        };
        let payload = self.returned_storage(&item);
        let count=ty.variants.len();
        let value=self.variant_value(ty,index,Some(payload));
        let mut payloads=vec![None;count];
        payloads[index] = Some(Box::new(item));
        Ok(Some(Box::new(OwnedValuePayload {
            value,
            payloads,
            ownership: Ownership::Owned,
            place: None, declaration: 0,
        })))
    }
    fn owned_condition(&mut self, value: &OwnedValue, span: Span) -> Result<Value> {
        self.owned_variant_condition(value,0,span,None)
    }
    fn owned_variant_condition(&mut self,value:&OwnedValue,index:usize,span:Span,origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        if value.payloads[index].is_none(){return Ok(self.constant_with_origin(Scalar::Bool,0,origin));}
        if value.payloads.iter().enumerate().all(|(i,p)|i==index||p.is_none()){return Ok(self.constant_with_origin(Scalar::Bool,1,origin));}
        self.variant_condition(&value.value,index,span,origin)
    }
    async fn unpack_owned(&mut self,value:&OwnedValue,success:bool)->Result<Returned> {unsafe { self.world.call(self.unpack_variant(value,usize::from(!success))).await }}
    pub(super) async fn unpack_variant(&mut self,value:&OwnedValue,index:usize)->Result<Returned> {
        let payload=self.variant_payload(&value.value,index);
        let Some(meta)=&value.payloads[index] else {return match payload {Some(value)=>unsafe { self.world.call(self.template_returned(value,false)).await },None=>Ok(Returned::Unit)};};
        let mut item=(**meta).clone();if let Some(value)=payload {
            match &mut item {Returned::Value(current)=>**current=value,Returned::OwnedValue(current)=>{current.value=value;current.place=None;},_=>self.rebind_returned(&mut item,value.regs)}
        }Ok(item)
    }
    pub(super) async fn evaluate_returned(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Returned>> {
        let world = self.world;
        let mut returned = unsafe { world.call(self.at_source(expr.span(), SourceSyntax::Expression,
            async |b| unsafe { world.call(b.evaluate_returned_syntax(expr, expected)) .await })) .await }?;
        match &mut returned {
            Some(Returned::Value(value)) => self.bound_value(value),
            Some(Returned::OwnedValue(value)) => self.bound_value(&mut value.value),
            _ => {}
        }
        Ok(returned)
    }
    async fn evaluate_returned_syntax(&mut self, expr: &Expr, expected: Option<&Ty>) -> Result<Option<Returned>> {
        self.view_uses.before(expr.span());
        self.require_returned_projection(expr)?;
        unsafe{self.world.call(source_call::returned(self,expr,expected)).await}
    }
    pub(super) async fn evaluate_returned_other(&mut self,expr:&Expr,expected:Option<&Ty>)->Result<Option<Returned>>{
        match expr.kind() {
            ExprKind::Target(target, call) => return unsafe { self.world.call(self.target_call(target, &call, expr.span())).await },
            ExprKind::Member(base, name) => return unsafe { self.world.call(self.member_returned(expr, &base, name)).await },
            ExprKind::Index(base, index) => return unsafe { self.world.call(self.indexed_returned(expr, &base, &index, expected)).await },
            ExprKind::Construct(name, args) => {
                let name=unsafe { self.world.call(self.scope.bindings().type_name(self.world,&name,(self.scope.source(),self.scope.module(),self.scope.imports()),expr.span())).await }?;
                return unsafe { self.world.call(self.construct_record_returned(&name,&args,expr.span())).await };
            }
            ExprKind::Name(name)
                if matches!(self.env.get(name), Some(Binding::ApplicationData(_))) =>
            {
                return unsafe { self.world.call(self
                    .move_application_dataset(name, expr.span())).await }
                    .map(|v| v.map(Returned::Dataset));
            }
            ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::OwnedValue(_))) => {
                return unsafe { self.world.call(self
                    .take_owned_value(name, expr.span())).await }
                    .map(|v| Some(Returned::OwnedValue(v)));
            }
            ExprKind::Select(c,a,b)=>return unsafe{self.world.call(self.selection_returned(&c,&a,&b,expr.span(),expected)).await},
            _=>{}
        }
        unsafe{self.world.call(self.typed_value_returned(expr,expected)).await}
    }
    #[inline(never)]
    async fn typed_value_returned(&mut self, expr: &Expr, expected: Option<&Ty>) -> Result<Option<Returned>> {
        unsafe { self.world.call(self.expr_expected(expr,expected)).await }.map(|value|value.map(Returned::value))
    }
    #[inline(never)]
    pub(super) async fn expected_owned_result(&mut self, expected: &Ty, success: bool, args: &Expressions, span: Span) -> Result<Option<Returned>> {
        unsafe { self.world.call(self.make_owned_result(expected.clone(),success,args,span)).await }.map(|value|value.map(Returned::OwnedValue))
    }
    #[inline(never)]
    pub(super) fn require_returned_projection(&self, expr: &Expr) -> Result<()> {
        if self.inside_user_owner(expr) && self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()) {
            return self.fail(expr.span(),"B224","拥有包装的资源载荷须用into_inner移出，不能部分移动其字段");
        }
        Ok(())
    }
    pub(super) async fn prepared_variant_returned(&mut self,expr:&Expr,ty:Ty,index:usize,args:&Expressions,owned:bool)->Result<Option<Returned>>{
        let world=self.world;
        let mut returned=unsafe{world.call(self.at_source(expr.span(),SourceSyntax::Expression,async |builder|{
            builder.view_uses.before(expr.span());
            builder.require_returned_projection(expr)?;
            if owned{
                unsafe{world.call(builder.make_owned_variant(ty,index,args,expr.span())).await}.map(|value|value.map(Returned::OwnedValue))
            }else{unsafe{world.call(builder.value_variant_constructor(ty,index,args,expr.span())).await}.map(|value|value.map(Returned::value))}
        })).await}?;
        match &mut returned{
            Some(Returned::Value(value))=>self.bound_value(value),
            Some(Returned::OwnedValue(value))=>self.bound_value(&mut value.value),
            _=>{}
        }
        Ok(returned)
    }
    #[inline(never)]
    async fn indexed_returned(&mut self, expr: &Expr, base: &Expr, index: &Expr, expected: Option<&Ty>) -> Result<Option<Returned>> {
        if self.owned_element_type(base).is_some_and(|ty|ty.array.is_some() && ty.owns_resources()) || self.temporary_projection(base) {
            return unsafe { self.world.call(self.owned_array_index(expr,base,index)).await };
        }
        if unsafe { self.world.call(self.read_expression_type(expr)).await }?.is_some_and(|ty|ty.owns_resources()){
            let Some(place)=unsafe { self.world.call(self.place(expr)).await }?else{return Ok(None)};
            return unsafe { self.world.call(self.take_owned_place(place,expr.span())).await }.map(Some);
        }
        unsafe { self.world.call(self.expr_expected(expr,expected)).await }.map(|value|value.map(Returned::value))
    }
    #[inline(never)]
    pub(super) async fn construct_record_returned(&mut self,name:&TypeName,args:&Expressions,span:Span)->Result<Option<Returned>> {
        let ty=unsafe { self.world.call(self.expected_storage(name,span)).await }?;
        if ty.task_result().is_some(){return self.fail(span,"B224","任务只能从start_task取得");}
        if ty.owns_resources(){return unsafe { self.world.call(self.make_owned_record(ty,args,span)).await };}
        unsafe { self.world.call(self.record_values(ty,args.len(),span,async |builder,index,expected|
            unsafe { builder.world.call(builder.expr_expected(&args.at(index),expected)).await })).await }.map(|value|value.map(Returned::value))
    }
    #[inline(never)]
    pub(super) async fn propagate_returned(
        &mut self,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Returned>> {
        if args.len() != 1 {
            return self.fail(span, "B269", "try需要一个结果表达式");
        }
        let returned = unsafe { self.world.call(self.evaluate_returned(&args.at(0), None)).await }?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let source = returned.ok_or_else(|| Diagnostic {
            source: self.scope.source().clone(),
            span,
            code: "B269",
            message: "try需要结果".into(),
        })?;
        let (value, owned) = match &source {
            Returned::OwnedValue(r) if r.value.ty.result => (r.value.clone(), Some(r)),
            Returned::Value(v) if v.ty.result => ((**v).clone(), None),
            _ => return self.fail(span, "B269", "try需要结果"),
        };
        let Some(expected) = self.result_return_type().filter(|ty| ty.result) else {
            return self.fail(span, "B269", "try所在函数必须返回结果");
        };
        self.require_same(&expected.arguments[1], &value.ty.arguments[1], span)?;
        let condition = if let Some(r) = owned {
            self.owned_condition(r, span)?
        } else {
            self.result_condition(&value, span)?
        };
        unsafe { self.world.call(self.conditional_blocks(
            condition,
            async |_| Ok(()),
            async |b| {
                let error = if let Some(r) = owned {
                    unsafe { b.world.call(b.unpack_owned(r, false)).await }?
                } else {
                    b.result_payload(&value, false)
                        .map_or(Returned::Unit, Returned::value)
                };
                let result = b.wrap_returned_result(expected, false, error);
                unsafe { b.world.call(b.finish_return(Some(result), span)).await }
            },
            span,
        )).await }?;
        if !self.flow.continues() {
            return Ok(None);
        }
        Ok(Some(if let Some(r) = owned {
            unsafe { self.world.call(self.unpack_owned(r, true)).await }?
        } else {
            match self.result_payload(&value, true) {
                Some(v) => Returned::value(v),
                None => return Ok(None),
            }
        }))
    }
    fn wrap_returned_result(&mut self, ty: Ty, success: bool, item: Returned) -> Returned {
        let payload = self.returned_storage(&item);
        let value = self.result_value(ty.clone(), success, Some(payload));
        if ty.owns_resources() {
            let mut payloads = vec![None;value.ty.variants.len()];
            payloads[usize::from(!success)] = Some(Box::new(item));
            Returned::OwnedValue(Box::new(OwnedValuePayload {
                value,
                payloads,
                ownership: Ownership::Owned,
                place: None, declaration: 0,
            }))
        } else {
            Returned::value(value)
        }
    }
    #[inline(never)]
    pub(super) async fn match_returned(&mut self, expr: &Expr, arms: &MatchArms, span: Span) -> Result<()> {
        let mut root=expr.clone();
        while let ExprKind::Member(base,_)|ExprKind::Index(base,_)=root.kind(){root=base;}
        let borrowed=self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()) && matches!(root.kind(),ExprKind::Name(name)if matches!(self.env.get(name),Some(Binding::OwnedValue(value))if value.ownership==Ownership::Borrowed));
        let (returned,held)=if borrowed {match unsafe { self.world.call(self.read_aggregate(expr,None)).await }? {Some((value,held))=>(Some(value),held),None=>(None,None)}}else{(unsafe { self.world.call(self.evaluate_returned(expr,None)).await }?,None)};
        if !self.flow.continues() { return Ok(()); }
        let returned = returned.ok_or_else(|| Diagnostic { source:self.scope.source().clone(),span,code:"B269",message:"match需要变体值".into() })?;
        let value = self.returned_storage(&returned);
        let selected = unsafe { self.world.call(self.check_variant_patterns(&value.ty,arms,span)).await }?;
        unsafe { self.world.call(self.match_variant_arms(&returned,&selected,0,span)).await }?;
        if let Some(name)=held {if self.flow.continues(){unsafe { self.world.call(self.finish_scoped_argument(&name,span)).await }?;}else{self.env.remove(&name);}}
        Ok(())
    }
    async fn match_variant_arms(&mut self, value: &Returned, arms: &[MatchArm], index: usize, span: Span) -> Result<()> {
        if index+1==arms.len() { return unsafe { self.world.call(self.match_variant_arm(value,index,&arms[index])).await }; }
        let condition = match value {
            Returned::Value(value) => self.variant_condition(value,index,span,None)?,
            Returned::OwnedValue(value) => self.owned_variant_condition(value,index,span,None)?,
            _=>unreachable!(),
        };
        unsafe { self.world.call(self.conditional_blocks(condition, async |b|unsafe { b.world.call(b.match_variant_arm(value,index,&arms[index])).await }, async |b|unsafe { b.world.call(b.match_variant_arms(value,arms,index+1,span)).await },span)).await }
    }
    async fn match_variant_arm(&mut self, value: &Returned, index: usize, arm: &MatchArm) -> Result<()> {
        match value {
            Returned::Value(value)=>unsafe { self.world.call(self.value_variant_arm(value,index,arm)).await },
            Returned::OwnedValue(value)=>unsafe { self.world.call(self.owned_arm(value,index,arm.binding(),&arm.body(),arm.span())).await },
            _=>unreachable!(),
        }
    }
    async fn owned_arm(&mut self,value:&OwnedValue,index:usize,name:Option<&str>,body:&Block,span:Span)->Result<()> {
        let item=unsafe { self.world.call(self.unpack_variant(value,index)).await }?;
        if let Some(name)=name.filter(|n|*n!="_") {
            self.bind_returned(name,item,span)?;unsafe { self.world.call(self.block(body)).await }?;
            if self.flow.continues() && let Some(binding)=self.env.get(name).cloned() {
                unsafe { self.world.call(self.drop_binding(binding,&view::Borrow::default(),span)).await }?;
            }
            self.env.remove(name);
            Ok(())
        } else {unsafe { self.world.call(self.drop_returned(item,&view::Borrow::default(),span)).await }?;unsafe { self.world.call(self.block(body)).await }}
    }
    pub(super) fn bind_returned(
        &mut self,
        name: &str,
        value: Returned,
        span: Span,
    ) -> Result<()> {
        match value {
            Returned::Value(value) => self.bind_local_value(name, *value, span),
            Returned::OwnedValue(value) => self.bind_owned_value(name, value, span),
            Returned::Dataset(data) => {
                if self.env.contains_key(name) {
                    return self.fail(span, "B125", "重复局部绑定");
                }
                let mutable =
                    self.scope.body().is_some_and(|body| application::rebound(&body, name));
                let data = self.application_bind(data, mutable);
                self.env
                    .insert(name.to_string(), Binding::ApplicationData(data));
                Ok(())
            }
            Returned::Unit => self.fail(span, "B228", "void分支没有可绑定值"),
        }
    }
    pub(super) async fn drop_returned(&mut self,value:Returned,retained:&view::Borrow,span:Span) -> Result<()> {
        let transferred=match &value{
            Returned::OwnedValue(value)if value.ownership.may_own()=>view_liveness::Transfer::Owned{value,place:value.place.as_ref()},
            Returned::Dataset(data)if data.ownership.may_own()=>view_liveness::Transfer::OwnedData(data),
            _=>return Ok(()),
        };
        let mut borrow=unsafe { self.world.call(self.view_uses.all(self,std::slice::from_ref(&transferred))).await }?;borrow.union(retained);
        let consumed=match &value {
            Returned::OwnedValue(value)=>value.place(),
            Returned::Dataset(data)=>data.place.clone(),
            _=>None,
        };
        if let Some(place)=&consumed {
            unsafe { self.world.call(self.require_place_access(place,Access::Write,span)).await }?;
        }
        unsafe { self.world.call(self.cleanup_group(consumed.into_iter().collect(),async |builder|unsafe { builder.world.call(builder.drop_owned_tree(value,&borrow,span)).await })).await }
    }
    pub(super) fn error_position_arguments(&mut self, place: &Place, status: Reg, span: Span) -> Result<Vec<CallArgument>> {
        let address = self.call_reference(place, Access::Read, false, span)?;
        let pointer = self.reg(Scalar::U64);
        self.emit(InstKind::PositionAddress { dst: pointer, address });
        let extent = self.constant(Scalar::U64, Bits::from(place.ty().size)).regs[0];
        Ok([status, pointer, extent].into_iter().map(|reg| CallArgument::Value {
            origin: None, ty: Ty::scalar(Scalar::U64), values: vec![reg], dataset_borrow: None,
        }).collect())
    }

    pub(super) async fn drop_binding(&mut self,binding:Binding,retained:&view::Borrow,span:Span) -> Result<()> {
        match binding {
            Binding::OwnedValue(r) => unsafe { self.world.call(self.drop_returned(Returned::OwnedValue(r),retained,span)).await },
            Binding::ApplicationData(data) => unsafe { self.world.call(self.drop_returned(Returned::Dataset(data),retained,span)).await },
            _ => Ok(()),
        }
    }
}

impl Builder<'_> {
    pub(super) fn hold_binding(&mut self, binding: Binding) -> String {
        let index=self.env.keys().filter_map(|name|name.strip_prefix("\0argument:"))
            .fold(0,|next,index|next.max(index.parse::<usize>().unwrap()+1));
        let name=format!("\0argument:{index}");
        self.env.insert(name.clone(), binding);
        name
    }
    pub(super) fn abandon_arguments(&mut self, held: &[(usize, String)]) {
        for (_, name) in held {
            self.env.remove(name);
        }
    }
    pub(super) async fn scoped_argument(&mut self, expr: &Expr) -> Result<Option<cooperative::Argument>> {
        let value = unsafe { self.world.call(self.evaluate_returned(expr, None)).await }?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let Some(Returned::Dataset(data)) = value else {
            return self.fail(
                expr.span(),
                "B224",
                "dataset借用需要实际资源或拥有数据的临时值",
            );
        };
        let mut reference = data.clone();
        reference.ownership = Ownership::Borrowed;
        let name=self.hold_binding(Binding::ApplicationData(data));
        Ok(Some(cooperative::Argument::Scoped {
            value: Returned::Dataset(reference),
            owner: name,
        }))
    }
    pub(super) async fn finish_scoped_argument(&mut self, name: &str, span: Span) -> Result<()> {
        let binding=self.env.remove(name).unwrap();
        unsafe { self.world.call(self.drop_binding(binding,&view::Borrow::default(),span)).await }
    }
}




impl Builder<'_> {
    // 临时拥有者仍只在原env中；完整消费结束后沿原资源作用域出口释放。
    pub(super) async fn temporary_scope<T>(&mut self, span: Span, work: impl std::ops::AsyncFnOnce(&mut Self)->Result<(T,view::Borrow)>)->Result<T> {
        let world = self.world;
        let original=self.env.keys().filter(|name|name.starts_with("\0argument:")).cloned().collect::<BTreeSet<_>>();
        let (value,retained)=work(self).await?;
        if self.flow.continues() {
            self.view_uses.after(span);
            let keep=self.env.keys().filter(|name|!name.starts_with("\0argument:")||original.contains(*name)).cloned().collect::<BTreeSet<_>>();
            unsafe { world.call(self.release_application_data(Some(&keep),retained)) .await }?;
        }
        Ok(value)
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(OwnedValuePayload);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        value: Slot,
        payloads: Slot,
        ownership: Slot,
    }
    #[repr(C)]
    struct Writer {
        value: WriteCallback,
        payloads: WriteCallback,
        ownership: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        value: ReadCallback,
        payloads: ReadCallback,
        ownership: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_owned_value_payload"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_owned_value_payload"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for OwnedValuePayload {
        fn empty() -> Self {
            Self {
                value: <Value as Native>::empty(),
                payloads: <Vec<Option<Box<Returned>>> as Native>::empty(),
                ownership: <Ownership as Native>::empty(),
                place: None,
                declaration: 0,
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        value: contracts::address(&self.value).cast_mut(),
                        payloads: contracts::address(&self.payloads).cast_mut(),
                        ownership: contracts::address(&self.ownership).cast_mut(),
                    },
                    Writer {
                        value: contracts::write_value::<Value>,
                        payloads: contracts::write_value::<Vec<Option<Box<Returned>>>>,
                        ownership: contracts::write_value::<Ownership>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        value: contracts::slot(&mut output.value),
                        payloads: contracts::slot(&mut output.payloads),
                        ownership: contracts::slot(&mut output.ownership),
                    },
                    Reader {
                        value: contracts::read_value::<Value>,
                        payloads: contracts::read_value::<Vec<Option<Box<Returned>>>>,
                        ownership: contracts::read_value::<Ownership>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};
