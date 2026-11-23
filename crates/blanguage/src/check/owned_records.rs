//! 记录的字段投影保持原局部槽、Frame 数据和每个资源叶的唯一拥有关系。
use super::*;
use application::Ownership;
use owned_values::{OwnedValue,ReadName as MemberName,read_field as member_field,read_field_name as member_field_name};
use returns::Returned;
mod projection;

#[derive(Clone)]
pub(super) struct OwnedProjection {
    pub root: String,
    pub path: Vec<usize>,
    pub offset: u32,
}
fn metadata_mut<'a>(owner: &'a mut OwnedValue, path: &[usize]) -> &'a mut Returned {
    let item = owner.payloads[path[0]].as_deref_mut().unwrap();
    if path.len() == 1 {
        item
    } else {
        let Returned::OwnedValue(child) = item else {
            unreachable!()
        };
        metadata_mut(child, &path[1..])
    }
}
pub(super) fn owned_field_place(ty: &Ty, place: &Place, field: &Ty, offset: u32) -> Place {
    place.clone().project(field.clone(), &projection::components(ty, field, offset))
}

impl Builder<'_> {
    pub(super) fn record_component(&self, value:&Value,index:usize)->Value {
        let(offset,ty)=value.ty.aggregate_part(index);
        let components=projection::components(&value.ty,ty,offset);
        self.component_value(value,ty,&components)
    }
    fn component_value(&self,value:&Value,ty:&Ty,components:&[usize])->Value {
        Value {ty:ty.clone(),regs:components.iter().map(|i|value.regs[*i]).collect(),
            proof:value.projected_proof(components),uniform:value.uniform,literal:false,
            constant:value.constant.as_ref().map(|v|components.iter().map(|i|v[*i]).collect())}
    }
    pub(super) fn unpack_record(&self, owner: &OwnedValue, index: usize) -> Returned {
        let (offset, ty) = owner.value.ty.aggregate_part(index);
        let components = projection::components(&owner.value.ty, ty, offset);
        let value = self.component_value(&owner.value, ty, &components);
        let place = owner.place.as_ref().map(|place| move || place.clone().project(ty.clone(), &components));
        owned_values::storage::unpack(self, value, || unsafe { owner.payloads[index].as_deref().unwrap_unchecked() }, place)
    }
    pub(super) fn owned_projection(&self, expr: &Expr) -> Option<OwnedProjection> {
        projection::owned(self,expr).map(|(value,_)|value)
    }
    fn projection_place(&self, p: &OwnedProjection, ty:&Ty) -> Option<Place> {
        let Binding::OwnedValue(owner) = self.env.get(&p.root)? else { return None; };
        let place=owner.place()?;
        let components=projection::components(&owner.value.ty,ty,p.offset);
        Some(place.project(ty.clone(),&components))
    }
    pub(super) async fn projected_owned(&mut self, p: &OwnedProjection, span: Span) -> Result<Returned> {
        let Some(Binding::OwnedValue(root)) = self.env.get(&p.root).cloned() else {
            unreachable!()
        };
        let mut item = Returned::OwnedValue(root);
        for index in &p.path {
            if let Returned::OwnedValue(owner)=&mut item {self.require_user_owner(owner,span)?;}
            item = match item {
                Returned::OwnedValue(owner) => self.unpack_record(&owner, *index),
                Returned::Value(value) => Returned::value(self.record_component(&value, *index)),
                _ => unreachable!(),
            };
        }
        self.resolve_owned_model(&mut item,None);
        match item {
            Returned::Value(value) => {
                if let Some(place) = self.projection_place(p,&value.ty) {
                    unsafe { self.world.call(self.read_place(&place, span, None)).await }.map(Returned::value)
                } else {
                    Ok(Returned::Value(value))
                }
            }
            Returned::Dataset(data) => {
                let mut data=data;data.residency=self.resolve_owned_residency(&data.residency,None);self.application_available(&data,span)?;self.prepare_owned_data(&data);
                Ok(Returned::Dataset(unsafe { self.world.call(self.refresh_application_data(data,span)).await }?))
            }
            other => Ok(other),
        }
    }
    pub(super) async fn application_named_reference(
        &mut self, name: &str, span: Span,
    ) -> Result<Option<application::DataReference>> {
        if let Some(Binding::DataReference(data)) = self.env.get(name).cloned() {
            self.application_available(&data.data, span)?;
            return Ok(Some(data));
        }
        unsafe { self.world.call(self.application_value(name, span)).await }.map(|data| data.map(application::DataReference::direct))
    }
    pub(super) async fn application_reference(
        &mut self,
        expr: &Expr,
    ) -> Result<Option<application::DataReference>> {
        if let ExprKind::Name(name) = expr.kind() {
            return unsafe { self.world.call(self.application_named_reference(name, expr.span())).await };
        }
        if let ExprKind::Member(base,field)=expr.kind() {
            if let Some(mut data)=unsafe { self.world.call(self.application_reference(&base)).await }? {
                let(_,ty)=data.element_layout();
                let Some(index)=ty.fields.iter().position(|(name,..)|name==field)else{
                    return if field=="length"{Ok(None)}else{self.fail(expr.span(),"B141","dataset字段不存在")};
                };
                data.projection.push(index);return Ok(Some(data));
            }
        }
        let Some(p) = projection::owned(self,expr)
            .filter(|(_,ty)|ty.dataset.is_some()).map(|(value,_)|value)
        else {
            if !self.owned_element_type(expr).is_some_and(|ty|ty.dataset.is_some())
                && !unsafe { self.world.call(self.read_expression_type(expr)).await }?.is_some_and(|ty|ty.dataset.is_some()) {
                return Ok(None);
            }
            let Some(place) = unsafe { self.world.call(self.place(expr)).await }? else {
                return Ok(None);
            };
            return unsafe { self.world.call(self.application_place_reference(place,expr.span())).await }.map(Some);
        };
        let Returned::Dataset(data) = unsafe { self.world.call(self.projected_owned(&p, expr.span())).await }? else {
            unreachable!()
        };
        self.application_available(&data, expr.span())?;
        let Binding::OwnedValue(root) = self.env.get_mut(&p.root).unwrap() else {
            unreachable!()
        };
        let Returned::Dataset(meta) = metadata_mut(root, &p.path) else {
            unreachable!()
        };
        meta.root.proof = data.root.proof.clone();
        meta.length.proof = data.length.proof.clone();
        Ok(Some(application::DataReference::direct(data)))
    }
    #[inline(never)]
    pub(super) async fn member_returned(&mut self,expr:&Expr,base:&Expr,name:&str)->Result<Option<Returned>> {
        let mut cursor=super::flow::LocalCursor {phase:0};
        let mut facts=MemberFacts {name:MemberName::of(name),owns:0,present:0,variants:0,temporary:0,place:0,resource:0,binding:0,continuing:0,kind:0,found:0,field_owns:0,item_kind:0,empty:0,array:0,view:0};
        let mut place=None;let mut projection=None;let mut reference=None;let mut parent=None;let mut item=None;let mut field=0usize;let mut retained=None;let mut dataset=None;let mut ordinary=None;let mut count=None;
        loop {let step=unsafe {member_step(cursor,facts)};cursor=step.cursor;match step.need {
            MemberNeed::Type=>facts.owns=u32::from(self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources())),
            MemberNeed::Place=>{place=unsafe{self.world.call(self.place(expr)).await}?;facts.present=u32::from(place.is_some());},
            MemberNeed::Take=>return unsafe{self.world.call(self.take_owned_place(place.take().unwrap(),expr.span())).await}.map(Some),
            MemberNeed::Projection=>{projection=self.owned_projection(expr);facts.present=u32::from(projection.is_some());},
            MemberNeed::Projected=>return unsafe{self.world.call(self.projected_owned(projection.as_ref().unwrap(),expr.span())).await}.map(Some),
            MemberNeed::Reference=>{reference=unsafe{self.world.call(self.application_reference(base)).await}?;facts.present=u32::from(reference.is_some());},
            MemberNeed::ReferenceLength=>return Ok(Some(Returned::value(reference.take().unwrap().data.length))),
            MemberNeed::BadField=>return self.fail(expr.span(),"B141","dataset只公开只读length"),
            MemberNeed::Variants=>facts.variants=u32::from(self.owned_element_type(base).is_some_and(|ty|ty.has_variants())),
            MemberNeed::BadVariant=>return self.fail(expr.span(),"B269","变体载荷通过match使用"),
            MemberNeed::Temporary=>facts.temporary=u32::from(self.temporary_projection(base)),
            MemberNeed::IsPlace=>facts.place=u32::from(self.is_place(expr)),
            MemberNeed::Resource=>{dataset=self.dataset_reference(base)?;facts.resource=u32::from(dataset.is_some());},
            MemberNeed::Binding=>facts.binding=match base.kind(){ExprKind::Name(n)=>match self.env.get(n){Some(Binding::Texture(_))=>1,Some(Binding::Tile(_))=>2,_=>0},_=>0},
            MemberNeed::TextureShape=>{let ExprKind::Name(n)=base.kind()else{unsafe{std::hint::unreachable_unchecked()}};let Some(Binding::Texture(reference))=self.env.get(n)else{unsafe{std::hint::unreachable_unchecked()}};count=reference.shape(self.program);facts.present=u32::from(count.is_some());},
            MemberNeed::TextureLength=>return Ok(Some(Returned::value(self.constant(Scalar::U32,Bits::from(count.unwrap()))))),
            MemberNeed::BadTexture=>return self.fail(expr.span(),"B239","纹理资源数组只公开只读 length"),
            MemberNeed::TileLength=>{let ExprKind::Name(n)=base.kind()else{unsafe{std::hint::unreachable_unchecked()}};let Some(Binding::Tile(view))=self.env.get(n)else{unsafe{std::hint::unreachable_unchecked()}};let length=view.length.clone();return Ok(Some(Returned::value(self.tile_length(length))));},
            MemberNeed::TileLane=>return Ok(Some(Returned::value(self.tile_lane()))),
            MemberNeed::BadTile=>return self.fail(expr.span(),"B141","tile 只公开只读 length 与 lane"),
            MemberNeed::DatasetLength=>return Ok(Some(Returned::value(self.data_length(dataset.as_ref().unwrap())))),
            MemberNeed::BasePlace=>facts.present=u32::from(self.is_place(base)),
            MemberNeed::OrdinaryPlace=>{place=unsafe{self.world.call(self.place(base)).await}?;facts.present=u32::from(place.is_some());},
            MemberNeed::PlaceKind=>{let ty=place.as_ref().unwrap().ty();facts.array=u32::from(ty.array.is_some());facts.view=u32::from(ty.view.is_some());},
            MemberNeed::ArrayLength=>{let length=place.as_ref().unwrap().ty().values[0];return Ok(Some(Returned::value(self.constant(Scalar::U32,Bits::from(length)))));},
            MemberNeed::ReadView=>{let view=unsafe{self.world.call(self.read_place(place.as_ref().unwrap(),expr.span(),None)).await}?;return Ok(Some(Returned::value(Self::view_length(&view))));},
            MemberNeed::BadView=>return self.fail(expr.span(),"B185","view 只公开只读 length，不公开设备地址分量"),
            MemberNeed::ReadField=>{let p=place.take().unwrap();let(sub,components)=member_type(p.ty(),name,self.scope.source(),expr.span())?;return unsafe{self.world.call(self.read_place(&p.project(sub,&components),expr.span(),None)).await}.map(|value|Some(Returned::value(value)));},
            MemberNeed::OrdinaryValue=>{ordinary=unsafe{self.world.call(self.expr(base)).await}?;facts.present=u32::from(ordinary.is_some());},
            MemberNeed::OrdinaryMemberValue=>return self.member_value(ordinary.take().unwrap(),name,expr.span()).map(|value|Some(Returned::value(value))),
            MemberNeed::Evaluate=>{parent=unsafe{self.world.call(self.evaluate_returned(base,None)).await}?;facts.continuing=u32::from(self.flow.continues());facts.kind=parent.as_ref().map_or(0,Returned::kind);},
            MemberNeed::Find=>{let Returned::OwnedValue(owner)=parent.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};field=unsafe{member_field(MemberName::of(name),owner.value.ty.fields.len(),&owner.value.ty,member_field_name)};facts.found=u32::from(field<owner.value.ty.fields.len());},
            MemberNeed::MissingField=>return self.fail(expr.span(),"B141","拥有值字段不存在"),
            MemberNeed::Unpack=>{let Returned::OwnedValue(owner)=parent.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};item=Some(self.unpack_record(owner,field));},
            MemberNeed::FieldType=>{let Returned::OwnedValue(owner)=parent.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};facts.field_owns=u32::from(owner.value.ty.aggregate_part(field).1.owns_resources());},
            MemberNeed::ItemKind=>{let Returned::OwnedValue(owner)=parent.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};facts.item_kind=owner.payloads[field].as_deref().unwrap().kind();},
            MemberNeed::MoveData=>{let Returned::OwnedValue(owner)=parent.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let Returned::Dataset(data)=owner.payloads[field].as_deref_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};data.ownership=Ownership::Moved;},
            MemberNeed::MoveOwner=>{let Returned::OwnedValue(owner)=parent.as_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let Returned::OwnedValue(value)=owner.payloads[field].as_deref_mut().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};value.mark_moved();},
            MemberNeed::Retained=>{let borrow=item.as_ref().unwrap().view_value().map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();facts.empty=u32::from(borrow.empty());retained=Some(borrow);},
            MemberNeed::DropOwner=>unsafe{self.world.call(self.drop_returned(parent.take().unwrap(),retained.as_ref().unwrap(),expr.span())).await}?,
            MemberNeed::HoldOwner=>{let Returned::OwnedValue(owner)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};self.hold_binding(Binding::OwnedValue(owner));},
            MemberNeed::Length=>{let Returned::Dataset(data)=parent.as_ref().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};item=Some(Returned::value(data.length.clone()));},
            MemberNeed::DropData=>unsafe{self.world.call(self.drop_returned(parent.take().unwrap(),&view::Borrow::default(),expr.span())).await}?,
            MemberNeed::Value=>{let Returned::Value(value)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};return self.member_value(*value,name,expr.span()).map(|value|Some(Returned::value(value)));},
            MemberNeed::Missing=>return self.fail(expr.span(),"B228","字段访问需要值"),
            MemberNeed::Return=>return Ok(item),
            MemberNeed::Complete=>return Ok(None),
        }}
    }

}

impl Binding {
    pub(super) fn proof_storage(&mut self) -> Option<(&mut bounds::Proof, &mut bool, Place)> {
        match self {
            Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => {
                Some((&mut owner.proof, &mut owner.uniform, owner.place.clone()))
            }
            Binding::OwnedValue(owner) => {
                let place = owner.place()?;
                Some((&mut owner.value.proof, &mut owner.value.uniform, place))
            }
            _ => None,
        }
    }
    pub(super) fn stored_place(&self) -> Option<Place> {
        match self {
            Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => Some(owner.place.clone()),
            Binding::OwnedValue(owner) => owner.place(),
            _ => None,
        }
    }
}

impl Builder<'_> {
    fn owned_expression_type(&self, expr: &Expr) -> Option<&Ty> {
        projection::ty(self,expr)
    }
    pub(super) async fn read_expression_type(&self,expr:&Expr)->Result<Option<Ty>> {
        Ok(match expr.kind() {
            ExprKind::Name(name)=>match self.env.get(name) {
                Some(Binding::OwnedValue(value))=>Some(value.value.ty.clone()),
                Some(Binding::Owned(owner))if matches!(owner.place.storage(),Place::ApplicationElement{..})=>Some(owner.place.ty().clone()),
                Some(Binding::ApplicationData(data))=>Some(Ty::owned_dataset(data.ty.syntax(),data.count)),
                Some(Binding::DataReference(data))=>Some(Ty::owned_dataset(data.element_layout().1.syntax(),data.count)),
                _=>None,
            },
            ExprKind::Member(base,field)=>{
                let Some(ty)=unsafe { self.world.call(self.read_expression_type(&base)).await }? else{return Ok(None);};
                if let Some(reference)=&ty.dataset {
                    let element=unsafe { self.world.call(self.expected_storage(reference,expr.span())).await }?;
                    element.field(field).map(|(_,field)|Ty::owned_dataset(field.syntax(),ty.values.first().copied()))
                }else{ty.field(field).map(|(_,ty)|ty.clone())}
            },
            ExprKind::Index(base,_)=>{
                let Some(ty)=unsafe { self.world.call(self.read_expression_type(&base)).await }? else{return Ok(None);};
                if let Some(reference)=ty.dataset {Some(unsafe { self.world.call(self.expected_storage(&reference,expr.span())).await }?)}
                else{ty.array.map(|element|*element)}
            },
            _=>None,
        })
    }
    pub(super) fn owned_element_type(&self, expr: &Expr) -> Option<&Ty> {
        self.owned_expression_type(expr)
    }
    #[inline(never)]
    pub(super) async fn owned_array_index(&mut self,expr:&Expr,base:&Expr,index:&Expr)->Result<Option<Returned>> {
        let mut cursor=super::flow::LocalCursor {phase:0};let mut facts=OwnedIndexFacts::default();
        let mut parent=None;let mut place=None;let mut held=None;let mut data=None;let mut offset=None;let mut item=None;
        loop {let step=unsafe{owned_index_step(cursor,facts)};cursor=step.cursor;match step.need {
            OwnedIndexNeed::ExistingType=>facts.defined=u32::from(self.owned_expression_type(base).is_some()),
            OwnedIndexNeed::Place=>{place=unsafe{self.world.call(self.place(expr)).await}?;facts.present=u32::from(place.is_some());},
            OwnedIndexNeed::TakePlace=>return unsafe{self.world.call(self.take_owned_place(place.take().unwrap(),expr.span())).await}.map(Some),
            OwnedIndexNeed::Evaluate=>{parent=unsafe{self.world.call(self.evaluate_returned(base,None)).await}?;facts.continuing=u32::from(self.flow.continues());},
            OwnedIndexNeed::Inspect=>{facts.kind=parent.as_ref().map_or(0,Returned::kind);facts.array=u32::from(matches!(parent.as_ref(),Some(Returned::OwnedValue(value))if value.value.ty.array.is_some()));facts.indexed=u32::from(matches!(parent.as_ref(),Some(Returned::Value(value))if value.ty.indexed_count().is_some()));},
            OwnedIndexNeed::BadArray=>return self.fail(expr.span(),"B224","拥有数组元素需要实际拥有数组值"),
            OwnedIndexNeed::HoldData=>{let Returned::Dataset(value)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};held=Some(self.hold_binding(Binding::ApplicationData(value.clone())));data=Some(application::DataReference::direct(value));},
            OwnedIndexNeed::DataIndex=>{place=unsafe{self.world.call(self.application_index(data.take().unwrap(),index)).await}?;facts.present=u32::from(place.is_some());},
            OwnedIndexNeed::DataType=>facts.owns=u32::from(place.as_ref().unwrap().ty().owns_resources()),
            OwnedIndexNeed::TakeData=>item=Some(unsafe{self.world.call(self.take_owned_place(place.take().unwrap(),expr.span())).await}?),
            OwnedIndexNeed::ReadData=>item=Some(Returned::value(unsafe{self.world.call(self.read_place(place.as_ref().unwrap(),expr.span(),None)).await}?)),
            OwnedIndexNeed::Storage=>{let value=self.returned_storage(item.as_ref().unwrap());facts.empty=u32::from(value.proof.typed_view_borrow(&value.ty).empty());},
            OwnedIndexNeed::Release=>unsafe{self.world.call(self.finish_scoped_argument(held.as_ref().unwrap(),expr.span())).await}?,
            OwnedIndexNeed::Return=>return Ok(item),
            OwnedIndexNeed::HoldOwner=>{let Returned::OwnedValue(owner)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};let owner=self.owned_value_binding(owner,true);place=Some(owner.place().unwrap());held=Some(self.hold_binding(Binding::OwnedValue(owner)));},
            OwnedIndexNeed::Index=>{offset=unsafe{self.world.call(self.expr(index)).await}?;facts.present=u32::from(offset.is_some());},
            OwnedIndexNeed::ArrayPlace=>place=Some(self.array_index(place.take().unwrap(),offset.take().unwrap(),index.span())?),
            OwnedIndexNeed::TakeElement=>item=Some(unsafe{self.world.call(self.take_owned_place(place.take().unwrap(),expr.span())).await}?),
            OwnedIndexNeed::ArrayValue=>{let Returned::Value(value)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};return unsafe{self.world.call(self.array_value_index(*value,index)).await}.map(|value|value.map(Returned::value));},
            OwnedIndexNeed::ViewPlace=>{let Returned::Value(value)=parent.take().unwrap()else{unsafe{std::hint::unreachable_unchecked()}};place=unsafe{self.world.call(self.view_value_place(*value,index)).await}?;facts.present=u32::from(place.is_some());},
            OwnedIndexNeed::ReadView=>return unsafe{self.world.call(self.read_place(place.as_ref().unwrap(),expr.span(),None)).await}.map(|value|Some(Returned::value(value))),
            OwnedIndexNeed::Complete=>return Ok(None),
        }}
    }

}

impl Builder<'_> {
    pub(super) async fn application_place_reference(
        &mut self,
        place: Place,
        span: Span,
    ) -> Result<application::DataReference> {
        let (mut data,sources)=if let Place::ApplicationElement{access,..}=place.storage() {
            let Returned::Dataset(mut data)=unsafe { self.world.call(self.application_owned_argument(&place,true,span)).await }?else{unreachable!()};
            data.access=*access;
            (data,Vec::new())
        }else{
            let candidates=self.owned_place_candidates(&place);
            let model=self.join_owned_place_candidates(&place, &candidates);self.prepare_owned_read(&model);unsafe { self.world.call(self.require_initialized(&place,span)).await }?;
            let Returned::Dataset(mut data)=model else{unreachable!()};
            let sources:Vec<_>=candidates.iter().map(|(_,_,model)|{let Returned::Dataset(data)=model else{unreachable!()};data.root.identity()}).collect();
            data.root.proof.set_view_borrow(0,self.data_borrow(&sources));
            (data,sources)
        };
        self.application_available(&data,span)?;
        data=unsafe { self.world.call(self.refresh_application_data(data,span)).await }?;data.ownership=Ownership::Borrowed;data.place=Some(place);
        Ok(application::DataReference { data, sources, projection:Vec::new() })
    }
}

impl Builder<'_> {
    pub(super) fn temporary_projection(&self, expr: &Expr) -> bool {
        projection::temporary(expr)
    }
}

#[repr(C)]#[derive(Clone,Copy)]struct MemberFacts {name:MemberName,owns:u32,present:u32,variants:u32,temporary:u32,place:u32,resource:u32,binding:u32,continuing:u32,kind:u32,found:u32,field_owns:u32,item_kind:u32,empty:u32,array:u32,view:u32}
#[repr(u32)]enum MemberNeed {Type,Place,Take,Projection,Projected,Reference,ReferenceLength,BadField,Variants,BadVariant,Temporary,IsPlace,Resource,Binding,TextureShape,Evaluate,Find,MissingField,Unpack,FieldType,ItemKind,MoveData,MoveOwner,Retained,DropOwner,HoldOwner,Length,DropData,Value,Missing,Return,Complete,TextureLength,BadTexture,TileLength,TileLane,BadTile,DatasetLength,BasePlace,OrdinaryPlace,PlaceKind,ArrayLength,ReadView,BadView,ReadField,OrdinaryValue,OrdinaryMemberValue}
#[repr(C)]struct MemberStep {cursor:super::flow::LocalCursor,need:MemberNeed}
#[repr(C)]#[derive(Clone,Copy,Default)]struct OwnedIndexFacts {defined:u32,present:u32,continuing:u32,kind:u32,array:u32,indexed:u32,owns:u32,empty:u32}
#[repr(u32)]enum OwnedIndexNeed {ExistingType,Place,TakePlace,Evaluate,Inspect,BadArray,HoldData,DataIndex,DataType,TakeData,ReadData,Storage,Release,Return,HoldOwner,Index,ArrayPlace,TakeElement,ArrayValue,ViewPlace,ReadView,Complete}
#[repr(C)]struct OwnedIndexStep {cursor:super::flow::LocalCursor,need:OwnedIndexNeed}
unsafe extern "C" {
 #[link_name="bootstrap.function_body::member_step"]fn member_step(cursor:super::flow::LocalCursor,facts:MemberFacts)->MemberStep;
 #[link_name="bootstrap.function_body::owned_index_step"]fn owned_index_step(cursor:super::flow::LocalCursor,facts:OwnedIndexFacts)->OwnedIndexStep;
}
