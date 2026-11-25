//! 拥有值沿原 Frame 持有资源；记录按字段组合，结果判别实际交付或释放的载荷。
use super::*;
#[path="owned_values/model.rs"]
mod model;
pub(super) use model::{live_data_at, owned_data_at, owned_data_at_mut};
pub(super) use model::{InitialWrite, LoopTarget, initial_root_incomplete, initial_write};
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
    pub(super) async fn require_returned_initialized(&mut self, value: &mut Returned, span: Span) -> Result<()> {
        let complete = |data: &mut Data, elements| {
            if let Some(elements) = elements {
                data.elements = application::ElementFacts::Known(elements);
            }
            data.initialization = None;
        };
        match value {
            Returned::Dataset(data) => {
                let elements = unsafe { self.world.call(self.require_application_initialized(data, span)).await }?;
                complete(data, elements);
            },
            Returned::OwnedValue(value) => {
                let mut checked = Vec::new();
                for data in value.resources() {
                    checked.push(unsafe { self.world.call(self.require_application_initialized(data, span)).await }?);
                }
                let mut checked = checked.into_iter();
                value.resources_mut(&mut |data| complete(data, checked.next().unwrap()));
            },
            _ => {},
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
        let mut cursor=ReadCursor {phase:0,path:0};let mut facts=ReadFacts {source:0,binding:0,application:0,owns:0,dataset:0,present:0,kind:0,available:0,variants:0,place:0,continuing:0};
        let mut syntax=None;let mut place=None;let mut projection=None;let mut model=None;let mut owner=None;
        loop {
            facts.continuing=u32::from(self.flow.continues());let step=unsafe {read_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                ReadNeed::Inspect=>{let kind=expr.kind();facts.source=match &kind{ExprKind::Name(_)=>1,ExprKind::Member(..)=>2,ExprKind::Index(..)=>3,_=>0};syntax=Some(kind);},
                ReadNeed::Name=>{let ExprKind::Name(name)=syntax.as_ref().unwrap()else{unreachable!()};facts.binding=u32::from(matches!(self.env.get(*name),Some(Binding::Owned(_))));},
                ReadNeed::NameStorage=>{let ExprKind::Name(name)=syntax.as_ref().unwrap()else{unreachable!()};let Some(Binding::Owned(value))=self.env.get(*name)else{unreachable!()};facts.application=u32::from(matches!(value.place.storage(),Place::ApplicationElement{..}));},
                ReadNeed::NameType=>{let ExprKind::Name(name)=syntax.as_ref().unwrap()else{unreachable!()};let Some(Binding::Owned(value))=self.env.get(*name)else{unreachable!()};facts.owns=u32::from(value.place.ty().owns_resources());facts.dataset=u32::from(value.place.ty().dataset.is_some());},
                ReadNeed::Application=>{let ExprKind::Name(name)=syntax.as_ref().unwrap()else{unreachable!()};let Some(Binding::Owned(value))=self.env.get(*name)else{unreachable!()};let place=value.place.clone();model=Some(unsafe {self.world.call(self.application_owned_argument(&place,true,expr.span())).await}?);facts.kind=model.as_ref().unwrap().kind();},
                ReadNeed::ElementType=>facts.owns=u32::from(self.owned_element_type(expr).is_some_and(Ty::owns_resources)),
                ReadNeed::Place=>{place=unsafe {self.world.call(self.place(expr)).await}?;facts.present=u32::from(place.is_some());},
                ReadNeed::Model=>{model=Some(self.owned_place_model(place.as_ref().unwrap()));facts.kind=model.as_ref().unwrap().kind();},
                ReadNeed::Projection=>{projection=self.owned_projection(expr);facts.present=u32::from(projection.is_some());},
                ReadNeed::Projected=>{model=Some(unsafe {self.world.call(self.projected_owned(projection.as_ref().unwrap(),expr.span())).await}?);projection=None;facts.kind=model.as_ref().unwrap().kind();},
                ReadNeed::BadOwned=>return self.fail(expr.span(),"B224","read聚合需要拥有记录、数组或变体"),
                ReadNeed::Available=>{let Returned::OwnedValue(value)=model.as_ref().unwrap()else{unreachable!()};facts.available=u32::from(value.available());},
                ReadNeed::BadAvailable=>return self.fail(expr.span(),"B227","只读借用需要已初始化且未移动的全部活动资源"),
                ReadNeed::Prepare=>{let Returned::OwnedValue(value)=model.as_ref().unwrap()else{unreachable!()};self.prepare_owned_value(value);},
                ReadNeed::Variants=>facts.variants=u32::from(model.as_ref().unwrap().view_value().unwrap().ty.has_variants()),
                ReadNeed::ProjectionPlace=>{let Returned::OwnedValue(value)=model.as_ref().unwrap()else{unreachable!()};place=value.place();facts.place=u32::from(place.is_some());},
                ReadNeed::Initialize=>unsafe {self.world.call(self.require_initialized(place.as_ref().unwrap(),expr.span())).await}?,
                ReadNeed::Load=>{let Returned::OwnedValue(value)=model.as_mut().unwrap()else{unreachable!()};unsafe {self.world.call(self.load_owned_value(value,expr.span())).await}?;},
                ReadNeed::HoldOwned=>{let Returned::OwnedValue(value)=model.as_ref().unwrap()else{unreachable!()};owner=Some(self.hold_binding(Binding::OwnedValue(value.clone())));},
                ReadNeed::Borrow=>{let Returned::OwnedValue(value)=model.as_mut().unwrap()else{unreachable!()};value.borrow();},
                ReadNeed::HoldData=>{let Returned::Dataset(data)=model.as_mut().unwrap()else{unreachable!()};owner=Some(self.hold_binding(Binding::ApplicationData(data.clone())));data.ownership=Ownership::Borrowed;},
                ReadNeed::Return=>return Ok(Some((model.take().unwrap(),owner))),
                ReadNeed::BadRead=>return self.fail(expr.span(),"B224","read需要实际普通值或拥有聚合"),
                ReadNeed::Complete=>return Ok(None),
                ReadNeed::Evaluate=>{model=unsafe {self.world.call(self.evaluate_returned(expr,expected)).await}?;facts.present=u32::from(model.is_some());facts.kind=model.as_ref().map_or(0,Returned::kind);},
                ReadNeed::Member=>{
                    let ExprKind::Member(base,field)=syntax.as_ref().unwrap()else{unreachable!()};let mut cursor=ReadLocalCursor {phase:0};
                    let mut facts=ReadMemberFacts {field:ReadName::of(field),present:0,kind:0,found:0,empty:0,owner:0};
                    let mut parent=None;let mut owner=None;let mut item=None;let mut retained=None;let mut index=0;
                    loop {let step=unsafe {read_member_step(cursor,facts)};cursor=step.cursor;match step.need {
                        ReadMemberNeed::Parent=>{if let Some((value,name))=unsafe {self.world.call(self.read_aggregate(base,None)).await}? {parent=Some(value);owner=name;}facts.present=u32::from(parent.is_some());facts.kind=parent.as_ref().map_or(0,Returned::kind);},
                        ReadMemberNeed::Find=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};index=unsafe {read_field(ReadName::of(field),value.value.ty.fields.len(),&value.value.ty,read_field_name)};facts.found=u32::from(index<value.value.ty.fields.len());},
                        ReadMemberNeed::MissingField=>return self.fail(expr.span(),"B141","拥有值字段不存在"),
                        ReadMemberNeed::Unpack=>{let Returned::OwnedValue(value)=parent.take().unwrap()else{unreachable!()};item=Some(self.unpack_record(&value,index));},
                        ReadMemberNeed::Value=>{let Returned::Value(value)=parent.take().unwrap()else{unreachable!()};item=Some(Returned::value(self.member_value(*value,field,expr.span())?));},
                        ReadMemberNeed::Length=>{let Returned::Dataset(data)=parent.take().unwrap()else{unreachable!()};item=Some(Returned::value(data.length));},
                        ReadMemberNeed::BadField=>return self.fail(expr.span(),"B141","read字段需要实际记录或普通值"),
                        ReadMemberNeed::Item=>facts.kind=item.as_ref().unwrap().kind(),
                        ReadMemberNeed::Retained=>{let value=item.as_ref().unwrap().view_value().unwrap();let borrow=value.proof.typed_view_borrow(&value.ty);facts.empty=u32::from(borrow.empty());facts.owner=u32::from(owner.is_some());retained=Some(borrow);},
                        ReadMemberNeed::Release=>{let binding=self.env.remove(&owner.take().unwrap()).unwrap();unsafe {self.world.call(self.drop_binding(binding,retained.as_ref().unwrap(),expr.span())).await}?;},
                        ReadMemberNeed::Plain=>return Ok(Some((item.take().unwrap(),None))),
                        ReadMemberNeed::Return=>return Ok(Some((item.take().unwrap(),owner))),
                        ReadMemberNeed::Complete=>return Ok(None),
                    }}
                },
                ReadNeed::Index=>{
                    let ExprKind::Index(base,expression)=syntax.as_ref().unwrap()else{unreachable!()};let mut cursor=ReadIndexCursor {phase:0,model:0,index:0};
                    let mut facts=ReadIndexFacts {present:0,kind:0,indexed:0,owns:0,array:0,owner:0,place:0,valid:0,count:0,minimum:0,maximum:0,components:0};
                    let mut parent=None;let mut owner=None;let mut data=None;let mut place=None;let mut packed=None;let mut item=None;let mut index=None;let mut regs=Vec::new();
                    loop {let step=unsafe {read_index_step(cursor,facts)};cursor=step.cursor;match step.need {
                        ReadIndexNeed::Reference=>{data=unsafe {self.world.call(self.application_reference(base)).await}?;facts.present=u32::from(data.is_some());},
                        ReadIndexNeed::Parent=>{if let Some((value,name))=unsafe {self.world.call(self.read_aggregate(base,None)).await}? {parent=Some(value);owner=name;}facts.present=u32::from(parent.is_some());},
                        ReadIndexNeed::Kind=>{facts.kind=parent.as_ref().unwrap().kind();if let Returned::Value(value)=parent.as_ref().unwrap(){facts.indexed=u32::from(value.ty.indexed_count().is_some());}},
                        ReadIndexNeed::ArrayValue=>{let Returned::Value(value)=parent.take().unwrap()else{unreachable!()};item=unsafe {self.world.call(self.array_value_index(*value,expression)).await}?.map(Returned::value);facts.present=u32::from(item.is_some());},
                        ReadIndexNeed::ViewPlace=>{let Returned::Value(value)=parent.take().unwrap()else{unreachable!()};place=unsafe {self.world.call(self.view_value_place(*value,expression)).await}?;facts.present=u32::from(place.is_some());},
                        ReadIndexNeed::Data=>{let Returned::Dataset(value)=parent.take().unwrap()else{unreachable!()};data=Some(application::DataReference::direct(value));},
                        ReadIndexNeed::DataIndex=>{place=unsafe {self.world.call(self.application_index(data.as_ref().unwrap().clone(),expression)).await}?;facts.present=u32::from(place.is_some());},
                        ReadIndexNeed::DataType=>facts.owns=u32::from(place.as_ref().unwrap().ty().owns_resources()),
                        ReadIndexNeed::Plain=>item=Some(Returned::value(unsafe {self.world.call(self.read_place(place.as_ref().unwrap(),expr.span(),None)).await}?)),
                        ReadIndexNeed::Storage=>packed=Some(unsafe {self.world.call(self.read_storage_value(place.as_ref().unwrap(),expr.span(),None)).await}?),
                        ReadIndexNeed::Materialize=>item=Some(unsafe {self.world.call(self.materialize_element(data.as_ref().unwrap(),packed.take(),place.as_ref().unwrap())).await}?),
                        ReadIndexNeed::BorrowElement=>{let Returned::OwnedValue(value)=item.as_mut().unwrap()else{unreachable!()};value.borrow();value.place=place.take();},
                        ReadIndexNeed::Return=>return Ok(Some((item.take().unwrap(),owner))),
                        ReadIndexNeed::BadIndex=>return self.fail(expr.span(),"B224","read下标需要实际数组或存储"),
                        ReadIndexNeed::ArrayType=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};facts.array=u32::from(value.value.ty.array.is_some());if value.value.ty.array.is_some(){facts.count=u64::from(value.value.ty.values[0]);}},
                        ReadIndexNeed::BadArray=>return self.fail(expr.span(),"B224","聚合只读下标需要固定拥有数组"),
                        ReadIndexNeed::Owner=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};facts.owner=u32::from(owner.is_some());facts.place=u32::from(value.place.is_some());},
                        ReadIndexNeed::Hold=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};owner=Some(self.hold_binding(Binding::OwnedValue(value.clone())));},
                        ReadIndexNeed::Expression=>{index=unsafe {self.world.call(self.expr(expression)).await}?;facts.present=u32::from(index.is_some());},
                        ReadIndexNeed::Type=>self.require_same(&Ty::scalar(Scalar::U32),&index.as_ref().unwrap().ty,expr.span())?,
                        ReadIndexNeed::Bounds=>facts.valid=u32::from(self.index_in(index.as_ref().unwrap(),IndexDomain::Fixed(facts.count as u32),facts.count as u32,None)),
                        ReadIndexNeed::BadBounds=>return self.fail(expr.span(),"B137","拥有数组下标有效性无法静态证明"),
                        ReadIndexNeed::Range=>{facts.minimum=u64::from(index.as_ref().unwrap().minimum());facts.maximum=u64::from(index.as_ref().unwrap().maximum());},
                        ReadIndexNeed::Unpack|ReadIndexNeed::First=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};item=Some(self.unpack_record(value,step.position as usize));},
                        ReadIndexNeed::Join=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};let next=self.unpack_record(value,step.position as usize);item.as_mut().unwrap().join(next,false,None);},
                        ReadIndexNeed::SourcePlace=>{let Returned::OwnedValue(value)=parent.as_ref().unwrap()else{unreachable!()};place=value.place();facts.place=u32::from(place.is_some());},
                        ReadIndexNeed::MaterializeArray=>{let Returned::OwnedValue(value)=parent.take().unwrap()else{unreachable!()};place=Some(self.materialize_array(value.value,None).place);},
                        ReadIndexNeed::ArrayPlace=>{place=Some(self.array_index(place.take().unwrap(),index.take().unwrap(),expr.span())?);facts.components=place.as_ref().unwrap().ty().components.len()as u64;},
                        ReadIndexNeed::Component=>{let place=place.as_ref().unwrap();let scalar=place.ty().components[step.position as usize].0;regs.push(unsafe {self.world.call(self.read_place(&place.clone().project(Ty::scalar(scalar),&[step.position as usize]),expr.span(),None)).await}?.regs[0]);},
                        ReadIndexNeed::Rebind=>self.rebind_returned(item.as_mut().unwrap(),std::mem::take(&mut regs)),
                        ReadIndexNeed::ModelKind=>facts.kind=item.as_ref().unwrap().kind(),
                        ReadIndexNeed::SetPlace=>{let Returned::OwnedValue(value)=item.as_mut().unwrap()else{unreachable!()};value.place=place.take();},
                        ReadIndexNeed::BorrowModel=>{let Returned::OwnedValue(value)=item.as_mut().unwrap()else{unreachable!()};value.borrow();},
                        ReadIndexNeed::Complete=>return Ok(None),
                    }}
                },
            }
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
    pub(super) async fn make_owned_variant(&mut self,ty:Ty,index:usize,args:&Expressions,span:Span)->Result<Option<OwnedValue>> {
        use super::results::{VariantBuildFacts,VariantBuildNeed,variant_build_step};
        let mut cursor=super::flow::LocalCursor {phase:0};
        let mut facts=VariantBuildFacts {owned:1,cpu:u32::from(self.program.target==Target::Cpu),application:u32::from(self.application.is_some()),unit:0,arguments:args.len(),continuing:0,present:0};
        let mut item=None;let mut payload=None;
        loop {
            let step=unsafe {variant_build_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                VariantBuildNeed::InvalidTarget=>return self.fail(span,"B224","拥有变体属于普通CPU应用资源组织"),
                VariantBuildNeed::Payload=>facts.unit=u32::from(ty.variants[index].ty.name=="void"),
                VariantBuildNeed::VoidArguments=>return self.fail(span,"B269","void分支没有载荷"),
                VariantBuildNeed::PayloadArguments=>return self.fail(span,"B269","结果分支需要一个载荷"),
                VariantBuildNeed::Unit=>item=Some(Returned::Unit),
                VariantBuildNeed::Evaluate=>{
                    item=unsafe {self.world.call(self.evaluate_returned(&args.at(0),Some(&ty.variants[index].ty))).await}?;
                    facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(item.is_some());
                }
                VariantBuildNeed::Missing=>return self.fail(span,"B228","结果分支需要载荷"),
                VariantBuildNeed::Check=>unsafe {self.world.call(self.check_returned_type(item.as_mut().unwrap(),&ty.variants[index].ty,span)).await}?,
                VariantBuildNeed::Storage=>payload=Some(self.returned_storage(item.as_ref().unwrap())),
                VariantBuildNeed::Build=>{
                    let count=ty.variants.len();let value=self.variant_value(ty,index,payload);
                    let mut payloads=vec![None;count];payloads[index]=Some(Box::new(item.take().unwrap()));
                    return Ok(Some(Box::new(OwnedValuePayload {value,payloads,ownership:Ownership::Owned,place:None,declaration:0})));
                }
                VariantBuildNeed::Complete=>return Ok(None),
            }
        }
    }
    fn owned_variant_condition(&mut self,value:&OwnedValue,index:usize,span:Span,origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        if value.payloads[index].is_none(){return Ok(self.constant_with_origin(Scalar::Bool,0,origin));}
        if value.payloads.iter().enumerate().all(|(i,p)|i==index||p.is_none()){return Ok(self.constant_with_origin(Scalar::Bool,1,origin));}
        self.variant_condition(&value.value,index,span,origin)
    }
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
            async |b| {b.view_uses.before(expr.span());b.require_returned_projection(expr)?;unsafe {world.call(source_call::returned(b,expr,expected)).await}})) .await }?;
        match &mut returned {
            Some(Returned::Value(value)) => self.bound_value(value),
            Some(Returned::OwnedValue(value)) => self.bound_value(&mut value.value),
            _ => {}
        }
        Ok(returned)
    }
    pub(super) async fn evaluate_returned_other(&mut self,expr:&Expr,expected:Option<&Ty>)->Result<Option<Returned>> {
        let syntax=expr.kind();let source=expr.signature_node();let mut cursor=super::flow::LocalCursor {phase:0};
        let mut facts=ReturnExprFacts::default();let mut name=None;let mut ty:Option<Ty>=None;let mut place=None;
        loop {let step=unsafe {return_expr_step(cursor,source,facts)};cursor=step.cursor;match step.need {
            ReturnExprNeed::Target=>{let ExprKind::Target(target,call)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.target_call(*target,call,expr.span())).await};},
            ReturnExprNeed::Member=>{let ExprKind::Member(base,field)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.member_returned(expr,base,field)).await};},
            ReturnExprNeed::IndexType=>{let ExprKind::Index(base,_)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};facts.owns=u32::from(self.owned_element_type(base).is_some_and(|ty|ty.array.is_some()&&ty.owns_resources()));},
            ReturnExprNeed::Temporary=>{let ExprKind::Index(base,_)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};facts.temporary=u32::from(self.temporary_projection(base));},
            ReturnExprNeed::ExpressionType=>facts.owns=u32::from(unsafe{self.world.call(self.read_expression_type(expr)).await}?.is_some_and(|ty|ty.owns_resources())),
            ReturnExprNeed::OwnedIndex=>{let ExprKind::Index(base,index)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.owned_array_index(expr,base,index)).await};},
            ReturnExprNeed::Place=>{place=unsafe{self.world.call(self.place(expr)).await}?;facts.present=u32::from(place.is_some());},
            ReturnExprNeed::Take=>return unsafe{self.world.call(self.take_owned_place(place.take().unwrap(),expr.span())).await}.map(Some),
            ReturnExprNeed::BindType=>{let ExprKind::Construct(raw,_)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};name=Some(unsafe{self.world.call(self.scope.bindings().type_name(self.world,raw,(self.scope.source(),self.scope.module(),self.scope.imports()),expr.span())).await}?);},
            ReturnExprNeed::Storage=>{let value=unsafe{self.world.call(self.expected_storage(name.as_ref().unwrap(),expr.span())).await}?;facts.task=u32::from(value.task_result().is_some());facts.owns=u32::from(value.owns_resources());ty=Some(value);},
            ReturnExprNeed::BadTask=>return self.fail(expr.span(),"B224","任务只能从start_task取得"),
            ReturnExprNeed::OwnedRecord=>{let ExprKind::Construct(_,args)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.make_owned_record(ty.take().unwrap(),args,expr.span())).await};},
            ReturnExprNeed::Record=>{let ExprKind::Construct(_,args)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.record_values(ty.take().unwrap(),args.len(),expr.span(),async |builder,index,expected|unsafe{builder.world.call(builder.expr_expected(&args.at(index),expected)).await})).await}.map(|value|value.map(Returned::value));},
            ReturnExprNeed::Binding=>{let ExprKind::Name(name)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};facts.binding=match self.env.get(*name){Some(Binding::ApplicationData(_))=>1,Some(Binding::OwnedValue(_))=>2,_=>0};},
            ReturnExprNeed::Dataset=>{let ExprKind::Name(name)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.move_application_dataset(name,expr.span())).await}.map(|value|value.map(Returned::Dataset));},
            ReturnExprNeed::Owned=>{let ExprKind::Name(name)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.take_owned_value(name,expr.span())).await}.map(|value|Some(Returned::OwnedValue(value)));},
            ReturnExprNeed::Selection=>{let ExprKind::Select(condition,yes,no)=&syntax else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.selection_returned(condition,yes,no,expr.span(),expected)).await};},
            ReturnExprNeed::Value=>return unsafe{self.world.call(self.expr_expected(expr,expected)).await}.map(|value|value.map(Returned::value)),
            ReturnExprNeed::Complete=>return Ok(None),
        }}
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
    pub(super) async fn propagate_returned(&mut self,args:&Expressions,span:Span)->Result<Option<Returned>> {
        let mut cursor=TryCursor {phase:0};
        let mut facts=TryFacts {arguments:args.len(),continuing:0,present:0,kind:0,result:0,expected:0};
        let mut source=None;let mut condition=None;
        loop {
            facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(source.is_some());
            let step=unsafe {try_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                TryNeed::Arity=>return self.fail(span,"B269","try需要一个结果表达式"),
                TryNeed::Evaluate=>source=unsafe {self.world.call(self.evaluate_returned(&args.at(0),None)).await}?,
                TryNeed::Inspect=>{let source=source.as_ref().unwrap();facts.kind=source.kind();facts.result=u32::from(source.view_value().is_some_and(|value|value.ty.result));},
                TryNeed::BadSource=>return self.fail(span,"B269","try需要结果"),
                TryNeed::Expected=>facts.expected=u32::from(self.result_return_type().is_some_and(|ty|ty.result)),
                TryNeed::BadFunction=>return self.fail(span,"B269","try所在函数必须返回结果"),
                TryNeed::ErrorType=>self.require_same(&self.result_return_type().unwrap().arguments[1],&source.as_ref().unwrap().view_value().unwrap().ty.arguments[1],span)?,
                TryNeed::ValueCondition=>condition=Some(self.result_condition(source.as_ref().unwrap().view_value().unwrap(),span)?),
                TryNeed::OwnedCondition=>{let Returned::OwnedValue(value)=source.as_ref().unwrap() else{unreachable!()};condition=Some(self.owned_variant_condition(value,0,span,None)?);},
                TryNeed::Branches=>{
                    let source=source.as_ref().unwrap();let source_kind=facts.kind;
                    unsafe {self.world.call(self.conditional_blocks(condition.take().unwrap(),async |_|Ok(()),async |builder|{
                        let mut cursor=TryCursor {phase:0};let mut facts=TryErrorFacts {kind:source_kind,owns:0,item:0};
                        let mut item=None;let mut expected=None;let mut payload=None;let mut value=None;
                        loop {
                            facts.item=item.as_ref().map_or(0,Returned::kind);
                            let step=unsafe {try_error_step(cursor,facts)};cursor=step.cursor;
                            match step.need {
                                TryErrorNeed::Value=>item=Some(builder.result_payload(source.view_value().unwrap(),false).map_or(Returned::Unit,Returned::value)),
                                TryErrorNeed::Owned=>{let Returned::OwnedValue(source)=source else{unreachable!()};item=Some(unsafe {builder.world.call(builder.unpack_variant(source,1)).await}?);},
                                TryErrorNeed::Expected=>{let ty=builder.result_return_type().unwrap().clone();facts.owns=u32::from(ty.owns_resources());expected=Some(ty);},
                                TryErrorNeed::Storage=>payload=Some(builder.returned_storage(item.as_ref().unwrap())),
                                TryErrorNeed::MoveValue=>{let Returned::Value(item)=item.take().unwrap() else{unreachable!()};payload=Some(*item);},
                                TryErrorNeed::Build=>value=Some(builder.result_value(expected.take().unwrap(),false,payload.take())),
                                TryErrorNeed::ValueReturn=>return unsafe {builder.world.call(builder.finish_return(Some(Returned::value(value.take().unwrap())),span)).await},
                                TryErrorNeed::OwnedReturn=>{
                                    let value=value.take().unwrap();let mut payloads=vec![None;value.ty.variants.len()];payloads[1]=Some(Box::new(item.take().unwrap()));
                                    let result=Returned::OwnedValue(Box::new(OwnedValuePayload {value,payloads,ownership:Ownership::Owned,place:None,declaration:0}));
                                    return unsafe {builder.world.call(builder.finish_return(Some(result),span)).await};
                                }
                            }
                        }
                    },span)).await}?;
                },
                TryNeed::ValueSuccess=>return Ok(self.result_payload(source.as_ref().unwrap().view_value().unwrap(),true).map(Returned::value)),
                TryNeed::OwnedSuccess=>{let Returned::OwnedValue(value)=source.as_ref().unwrap() else{unreachable!()};return unsafe {self.world.call(self.unpack_variant(value,0)).await}.map(Some);},
                TryNeed::Complete=>return Ok(None),
            }
        }
    }
    #[inline(never)]
    pub(super) async fn match_returned(&mut self,expr:&Expr,arms:&MatchArms,span:Span)->Result<()> {
        let mut cursor=MatchCursor {phase:0};let mut facts=MatchFacts {owns:0,root:0,borrowed:0,continuing:0,present:0,kind:0};
        let mut root=None;let mut parent=None;let mut returned=None;let mut selected=Vec::new();
        loop {
            facts.continuing=u32::from(self.flow.continues());facts.present=u32::from(returned.is_some());facts.kind=returned.as_ref().map_or(0,Returned::kind);
            let step=unsafe {match_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                MatchNeed::Type=>facts.owns=u32::from(self.owned_element_type(expr).is_some_and(Ty::owns_resources)),
                MatchNeed::Root=>root=Some(expr.clone()),
                MatchNeed::RootKind=>{facts.root=match root.as_ref().unwrap().kind(){ExprKind::Member(base,_)=>{parent=Some(base);1},ExprKind::Index(base,_)=>{parent=Some(base);2},ExprKind::Name(_)=>3,_=>0};},
                MatchNeed::Parent=>root=parent.take(),
                MatchNeed::Borrowed=>{facts.borrowed=u32::from(matches!(root.as_ref().unwrap().kind(),ExprKind::Name(name)if matches!(self.env.get(name),Some(Binding::OwnedValue(value))if value.ownership==Ownership::Borrowed)));},
                MatchNeed::Read=>returned=unsafe {self.world.call(self.read_aggregate(expr,None)).await}?.map(|(value,_)|value),
                MatchNeed::Evaluate=>returned=unsafe {self.world.call(self.evaluate_returned(expr,None)).await}?,
                MatchNeed::Missing=>return self.fail(span,"B269","match需要变体值"),
                MatchNeed::InvalidType=>return self.fail(span,"B269","match需要带载荷变体或result值"),
                MatchNeed::Patterns=>{selected=unsafe {self.world.call(self.check_variant_patterns(&returned.as_ref().unwrap().view_value().unwrap().ty,arms,span)).await}?;},
                MatchNeed::Arms=>unsafe {self.world.call(self.match_variant_arms(returned.as_ref().unwrap(),&selected,0,span)).await}?,
                MatchNeed::Complete=>return Ok(()),
            }
        }
    }
    async fn match_variant_arms(&mut self,value:&Returned,arms:&[MatchArm],index:usize,span:Span)->Result<()> {
        let condition=match unsafe {match_branch(index,arms.len(),value.kind())} {
            MatchBranch::Last=>return unsafe {self.world.call(self.match_variant_arm(value,index,&arms[index])).await},
            MatchBranch::Value=>{let Returned::Value(value)=value else{unreachable!()};self.variant_condition(value,index,span,None)?},
            MatchBranch::Owned=>{let Returned::OwnedValue(value)=value else{unreachable!()};self.owned_variant_condition(value,index,span,None)?},
        };
        unsafe {self.world.call(self.conditional_blocks(condition,async |b|unsafe {b.world.call(b.match_variant_arm(value,index,&arms[index])).await},async |b|unsafe {b.world.call(b.match_variant_arms(value,arms,index+1,span)).await},span)).await}
    }
    async fn match_variant_arm(&mut self,value:&Returned,index:usize,arm:&MatchArm)->Result<()> {
        let mut cursor=MatchCursor {phase:0};let mut facts=MatchArmFacts {owned:u32::from(matches!(value,Returned::OwnedValue(_))),named:0,continuing:0,present:0};
        let mut item=None;let mut payload=None;let mut name=None;let mut binding=None;
        loop {
            facts.continuing=u32::from(self.flow.continues());let step=unsafe {match_arm_step(cursor,facts)};cursor=step.cursor;
            match step.need {
                MatchArmNeed::Unpack=>{let Returned::OwnedValue(value)=value else{unreachable!()};item=Some(unsafe {self.world.call(self.unpack_variant(value,index)).await}?);},
                MatchArmNeed::Payload=>{let Returned::Value(value)=value else{unreachable!()};payload=self.variant_payload(value,index);},
                MatchArmNeed::Name=>{name=arm.binding();let spelling=name.unwrap_or("");facts.named=unsafe {match_binding(u32::from(name.is_some()),spelling.as_ptr(),spelling.len())};},
                MatchArmNeed::BindValue=>self.bind_local_value(name.unwrap(),payload.take().unwrap(),arm.span())?,
                MatchArmNeed::BindReturned=>self.bind_returned(name.unwrap(),item.take().unwrap(),arm.span())?,
                MatchArmNeed::Body=>unsafe {self.world.call(self.block(&arm.body())).await}?,
                MatchArmNeed::Lookup=>{binding=self.env.get(name.unwrap()).cloned();facts.present=u32::from(binding.is_some());},
                MatchArmNeed::DropBinding=>unsafe {self.world.call(self.drop_binding(binding.take().unwrap(),&view::Borrow::default(),arm.span())).await}?,
                MatchArmNeed::Remove=>{self.env.remove(name.unwrap());return Ok(());},
                MatchArmNeed::DropValue=>unsafe {self.world.call(self.drop_returned(item.take().unwrap(),&view::Borrow::default(),arm.span())).await}?,
                MatchArmNeed::Complete=>return Ok(()),
            }
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

#[repr(C)]#[derive(Clone,Copy)]struct OwnedFacts {collision:u32,rebound:u32,variants:u32}
#[repr(u32)]enum OwnedNeed {Collision,Rebound,Variants,Bind}
#[repr(C)]struct OwnedStep {cursor:super::flow::LocalCursor,need:OwnedNeed,mutable:u32}
#[repr(C)]#[derive(Clone,Copy)]struct ReturnedFacts {kind:u32,collision:u32,rebound:u32}
#[repr(u32)]enum ReturnedNeed {Value,Owned,Unit,Collision,Rebound,Dataset,Name}
#[repr(C)]struct ReturnedStep {cursor:super::flow::LocalCursor,need:ReturnedNeed,mutable:u32}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::owned_step"]fn owned_step(cursor:super::flow::LocalCursor,facts:OwnedFacts)->OwnedStep;
 #[link_name="bootstrap.function_body::returned_step"]fn returned_step(cursor:super::flow::LocalCursor,facts:ReturnedFacts)->ReturnedStep;
}
impl Builder<'_> {
 pub(super) fn bind_owned_value(&mut self,name:&str,result:OwnedValue,span:Span)->Result<()> {
  let mut cursor=super::flow::LocalCursor {phase:0};let mut facts=OwnedFacts {collision:u32::from(self.env.contains_key(name)),rebound:0,variants:0};let mut result=Some(result);
  loop {let step=unsafe {owned_step(cursor,facts)};cursor=step.cursor;match step.need {
   OwnedNeed::Collision=>return self.fail(span,"B125",format!("重复局部变量或覆盖参数 {name}")),
   OwnedNeed::Rebound=>facts.rebound=u32::from(self.scope.body().is_some_and(|body|application::rebound(&body,name))),
   OwnedNeed::Variants=>facts.variants=u32::from(result.as_ref().unwrap().value.ty.has_variants()),
   OwnedNeed::Bind=>{let mut result=self.owned_value_binding(result.take().unwrap(),step.mutable!=0);result.declaration=span.start;self.env.insert(name.to_string(),Binding::OwnedValue(result));return Ok(());},
  }}
 }
 pub(super) fn bind_returned(&mut self,name:&str,value:Returned,span:Span)->Result<()> {
  let(kind,mut plain,mut owned,mut data)=match value {Returned::Value(v)=>(0,Some(v),None,None),Returned::OwnedValue(v)=>(1,None,Some(v),None),Returned::Dataset(v)=>(2,None,None,Some(v)),Returned::Unit=>(3,None,None,None)};
  let mut cursor=super::flow::LocalCursor {phase:0};let mut facts=ReturnedFacts {kind,collision:0,rebound:0};
  loop {let step=unsafe {returned_step(cursor,facts)};cursor=step.cursor;match step.need {
   ReturnedNeed::Value=>return self.bind_local_value(name,*plain.take().unwrap(),span),
   ReturnedNeed::Owned=>return self.bind_owned_value(name,owned.take().unwrap(),span),
   ReturnedNeed::Unit=>return self.fail(span,"B228","void分支没有可绑定值"),
   ReturnedNeed::Name=>facts.collision=u32::from(self.env.contains_key(name)),
   ReturnedNeed::Collision=>return self.fail(span,"B125","重复局部绑定"),
   ReturnedNeed::Rebound=>facts.rebound=u32::from(self.scope.body().is_some_and(|body|application::rebound(&body,name))),
   ReturnedNeed::Dataset=>{let data=self.application_bind(data.take().unwrap(),step.mutable!=0);self.env.insert(name.to_string(),Binding::ApplicationData(data));return Ok(());},
  }}
 }
}

#[repr(C)]#[derive(Clone,Copy)]struct MatchCursor {phase:u32}
#[repr(C)]#[derive(Clone,Copy)]struct MatchFacts {owns:u32,root:u32,borrowed:u32,continuing:u32,present:u32,kind:u32}
#[repr(u32)]enum MatchNeed {Type,Root,RootKind,Parent,Borrowed,Read,Evaluate,Missing,InvalidType,Patterns,Arms,Complete}
#[repr(C)]struct MatchStep {cursor:MatchCursor,need:MatchNeed}
#[repr(u32)]enum MatchBranch {Last,Value,Owned}
#[repr(C)]#[derive(Clone,Copy)]struct MatchArmFacts {owned:u32,named:u32,continuing:u32,present:u32}
#[repr(u32)]enum MatchArmNeed {Unpack,Payload,Name,BindValue,BindReturned,Body,Lookup,DropBinding,Remove,DropValue,Complete}
#[repr(C)]struct MatchArmStep {cursor:MatchCursor,need:MatchArmNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::match_step"]fn match_step(cursor:MatchCursor,facts:MatchFacts)->MatchStep;
 #[link_name="bootstrap.function_body::match_branch"]fn match_branch(index:usize,count:usize,kind:u32)->MatchBranch;
 #[link_name="bootstrap.function_body::match_binding"]fn match_binding(present:u32,data:*const u8,length:usize)->u32;
 #[link_name="bootstrap.function_body::match_arm_step"]fn match_arm_step(cursor:MatchCursor,facts:MatchArmFacts)->MatchArmStep;
}

#[repr(C)]#[derive(Clone,Copy)]struct TryCursor {phase:u32}
#[repr(C)]#[derive(Clone,Copy)]struct TryFacts {arguments:usize,continuing:u32,present:u32,kind:u32,result:u32,expected:u32}
#[repr(u32)]enum TryNeed {Arity,Evaluate,Inspect,BadSource,Expected,BadFunction,ErrorType,ValueCondition,OwnedCondition,Branches,ValueSuccess,OwnedSuccess,Complete}
#[repr(C)]struct TryStep {cursor:TryCursor,need:TryNeed}
#[repr(C)]#[derive(Clone,Copy)]struct TryErrorFacts {kind:u32,owns:u32,item:u32}
#[repr(u32)]enum TryErrorNeed {Value,Owned,Expected,Storage,MoveValue,Build,ValueReturn,OwnedReturn}
#[repr(C)]struct TryErrorStep {cursor:TryCursor,need:TryErrorNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::try_step"]fn try_step(cursor:TryCursor,facts:TryFacts)->TryStep;
 #[link_name="bootstrap.function_body::try_error_step"]fn try_error_step(cursor:TryCursor,facts:TryErrorFacts)->TryErrorStep;
}

#[repr(C)]#[derive(Clone,Copy)]struct ReadCursor {phase:u32,path:u32}
#[repr(C)]#[derive(Clone,Copy)]struct ReadFacts {source:u32,binding:u32,application:u32,owns:u32,dataset:u32,present:u32,kind:u32,available:u32,variants:u32,place:u32,continuing:u32}
#[repr(u32)]enum ReadNeed {Inspect,Name,NameStorage,NameType,Application,ElementType,Place,Model,Projection,Projected,BadOwned,Available,BadAvailable,Prepare,Variants,ProjectionPlace,Initialize,Load,HoldOwned,Borrow,HoldData,Return,Member,Index,Evaluate,BadRead,Complete}
#[repr(C)]struct ReadStep {cursor:ReadCursor,need:ReadNeed}
#[repr(C)]#[derive(Clone,Copy)]struct ReadLocalCursor {phase:u32}
#[repr(C)]#[derive(Clone,Copy)]pub(super) struct ReadName {data:*const u8,length:usize}
impl ReadName {pub(super) fn of(value:&str)->Self{Self{data:value.as_ptr(),length:value.len()}}}
#[repr(C)]#[derive(Clone,Copy)]struct ReadMemberFacts {field:ReadName,present:u32,kind:u32,found:u32,empty:u32,owner:u32}
#[repr(u32)]enum ReadMemberNeed {Parent,Find,MissingField,Unpack,Value,Length,BadField,Item,Retained,Release,Plain,Return,Complete}
#[repr(C)]struct ReadMemberStep {cursor:ReadLocalCursor,need:ReadMemberNeed}
#[repr(C)]#[derive(Clone,Copy)]struct ReadIndexCursor {phase:u32,model:u32,index:u64}
#[repr(C)]#[derive(Clone,Copy)]struct ReadIndexFacts {present:u32,kind:u32,indexed:u32,owns:u32,array:u32,owner:u32,place:u32,valid:u32,count:u64,minimum:u64,maximum:u64,components:u64}
#[repr(u32)]enum ReadIndexNeed {Reference,Parent,Kind,ArrayValue,ViewPlace,Data,DataIndex,DataType,Plain,Storage,Materialize,BorrowElement,Return,BadIndex,ArrayType,BadArray,Owner,Hold,Expression,Type,Bounds,BadBounds,Range,Unpack,First,Join,SourcePlace,MaterializeArray,ArrayPlace,Component,Rebind,ModelKind,BorrowModel,Complete,SetPlace}
#[repr(C)]struct ReadIndexStep {cursor:ReadIndexCursor,need:ReadIndexNeed,position:u64}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::read_step"]fn read_step(cursor:ReadCursor,facts:ReadFacts)->ReadStep;
 #[link_name="bootstrap.function_body::read_member_step"]fn read_member_step(cursor:ReadLocalCursor,facts:ReadMemberFacts)->ReadMemberStep;
 #[link_name="bootstrap.function_body::read_field"]pub(super) fn read_field(name:ReadName,count:usize,context:*const Ty,field:unsafe extern "C" fn(*const Ty,usize)->ReadName)->usize;
 #[link_name="bootstrap.function_body::read_index_step"]fn read_index_step(cursor:ReadIndexCursor,facts:ReadIndexFacts)->ReadIndexStep;
}
pub(super) unsafe extern "C" fn read_field_name(context:*const Ty,index:usize)->ReadName {ReadName::of(&unsafe{&*context}.fields[index].0)}

#[repr(C)]#[derive(Clone,Copy,Default)]struct ReturnExprFacts {binding:u32,owns:u32,temporary:u32,present:u32,task:u32}
#[repr(u32)]enum ReturnExprNeed {Target,Member,IndexType,Temporary,ExpressionType,OwnedIndex,Place,Take,BindType,Storage,BadTask,OwnedRecord,Record,Binding,Dataset,Owned,Selection,Value,Complete}
#[repr(C)]struct ReturnExprStep {cursor:super::flow::LocalCursor,need:ReturnExprNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::return_expr_step"]fn return_expr_step(cursor:super::flow::LocalCursor,source:crate::syntax::wire::SignatureNode,facts:ReturnExprFacts)->ReturnExprStep;
}
