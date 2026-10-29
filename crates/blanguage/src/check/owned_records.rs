//! 记录的字段投影保持原局部槽、Frame 数据和每个资源叶的唯一拥有关系。
use super::*;
use application::Ownership;
use owned_values::{OwnedValue, OwnedValuePayload};
use returns::Returned;

#[derive(Clone)]
pub(super) struct OwnedProjection {
    pub root: String,
    pub path: Vec<usize>,
    pub components: Vec<usize>,
    pub ty: Ty,
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
impl Builder<'_> {
    pub(super) fn record_component(&self, value: &Value, index: usize) -> Value {
        let (offset, ty) = value.ty.aggregate_part(index);
        let components: Vec<_> = ty
            .components
            .iter()
            .map(|(_, at)| {
                value
                    .ty
                    .components
                    .iter()
                    .position(|(_, word)| *word == offset + at)
                    .unwrap()
            })
            .collect();
        Value {
            ty: ty.clone(),
            regs: components.iter().map(|i| value.regs[*i]).collect(),
            proof: value.projected_proof(&components),
            uniform: value.uniform,
            literal: false,
            constant: value
                .constant
                .as_ref()
                .map(|v| components.iter().map(|i| v[*i]).collect()),
        }
    }
    pub(super) fn unpack_record(&mut self, owner: &OwnedValue, index: usize) -> Returned {
        let value = self.record_component(&owner.value, index);
        if !value.ty.owns_resources() {
            return Returned::value(value);
        }
        let mut item = (**owner.payloads[index].as_ref().unwrap()).clone();
        self.rebind_returned(&mut item, value.regs.clone());
        if let Returned::OwnedValue(child) = &mut item {
            child.value = value;
        }
        if let Some(place)=owner.place(){
            let(offset,ty)=owner.value.ty.aggregate_part(index);
            let components=ty.components.iter().map(|(_,at)|owner.value.ty.components.iter().position(|(_,word)|*word==offset+at).unwrap()).collect::<Vec<_>>();
            let place=place.project(ty.clone(),&components);
            match &mut item {Returned::Dataset(data)=>data.place=Some(place),Returned::OwnedValue(child)=>child.place=Some(place),_=>{}}
        }
        item
    }
    #[inline(never)]
    pub(super) fn make_owned_record(
        &mut self,
        ty: Ty,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Returned>> {
        if ty.has_variants() {return self.fail(span,"B269","带载荷变体通过声明的分支构造");}
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span, "B224", "拥有资源的记录由普通CPU应用函数组织");
        }
        if args.len() != ty.aggregate_len() && !(ty.array.is_some() && args.len() == 1) {
            return self.fail(span, "B157", "记录构造需要按声明顺序提供每个字段");
        }
        let mut fields = Vec::new();
        let mut held = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let (_, field) = ty.aggregate_part(i);
            let item = self.evaluate_returned(arg, Some(field))?;
            if !self.flow.continues() {
                self.abandon_arguments(&held);
                return Ok(None);
            }
            let mut item = item.ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
                span: arg.span,
                code: "B228",
                message: "记录字段需要值或拥有资源".into(),
            })?;
            if ty.array.is_some() && args.len() == 1 {
                if matches!(&item,Returned::OwnedValue(value) if value.value.ty==ty) {
                    return Ok(Some(item));
                }
                if ty.aggregate_len() != 1 {
                    return self.fail(
                        span,
                        "B157",
                        "拥有数组构造需要逐项交付全部元素，不能复制一个拥有值填充",
                    );
                }
            }
            self.check_returned_type(&mut item, field, arg.span)?;
            let binding = match &item {
                Returned::Dataset(data) => Some(Binding::ApplicationData(data.clone())),
                Returned::OwnedValue(value) => Some(Binding::OwnedValue(value.clone())),
                Returned::Value(value) if value.ty.contains_views()=>Some(Binding::Value((**value).clone())),
                _ => None,
            };
            if let Some(binding) = binding {
                let name = self.hold_binding(binding);
                held.push((fields.len(), name));
            }
            fields.push(item);
        }
        for (index, name) in held {
            fields[index] = match self.env.remove(&name).unwrap() {
                Binding::ApplicationData(data) => Returned::Dataset(data),
                Binding::OwnedValue(value) => Returned::OwnedValue(value),
                Binding::Value(value)=>Returned::value(value),
                _ => unreachable!(),
            };
        }
        let mut payloads = Vec::new();
        let mut values = Vec::new();
        for (i, item) in fields.into_iter().enumerate() {
            let (_, field) = ty.aggregate_part(i);
            values.push(self.returned_storage(&item));
            payloads.push(field.owns_resources().then(|| Box::new(item)));
        }
        let mut value = self.pack_record(ty, values, span)?;
        value.proof.retain_view_borrows(&value.ty);
        Ok(Some(Returned::OwnedValue(Box::new(OwnedValuePayload {
            value,
            payloads,
            ownership: Ownership::Owned,
            place: None, declaration: 0,
        }))))
    }
    pub(super) fn owned_projection(&self, expr: &Expr) -> Option<OwnedProjection> {
        match &expr.kind {
            ExprKind::Name(name) => {
                let Binding::OwnedValue(owner) = self.env.get(name)? else {
                    return None;
                };
                Some(OwnedProjection {
                    root: name.clone(),
                    path: Vec::new(),
                    components: (0..owner.value.ty.components.len()).collect(),
                    ty: owner.value.ty.clone(),
                })
            }
            ExprKind::Index(base, index) => {
                let mut p = self.owned_projection(base)?;
                let i = match &index.kind {
                    ExprKind::StaticU32(value) => *value,
                    ExprKind::Number(text) => text.parse::<u32>().ok()?,
                    ExprKind::Name(name) => match self.env.get(name)? {
                        Binding::Value(value) if value.ty == Ty::scalar(Scalar::U32) => {
                            value.constant.as_ref()?.first().copied()? as u32
                        }
                        Binding::Owned(value)if value.place.ty()==&Ty::scalar(Scalar::U32)=>value.proof.known_value(value.place.ty())?.first().copied()? as u32,
                        _ => return None,
                    },
                    _ => return None,
                } as usize;
                if p.ty.array.is_none() || i >= p.ty.aggregate_len() {
                    return None;
                }
                let (offset, ty) = p.ty.aggregate_part(i);
                p.components = ty
                    .components
                    .iter()
                    .map(|(_, at)| {
                        p.components[p
                            .ty
                            .components
                            .iter()
                            .position(|(_, word)| *word == offset + at)
                            .unwrap()]
                    })
                    .collect();
                p.ty = ty.clone();
                p.path.push(i);
                Some(p)
            }
            ExprKind::Member(base, name) => {
                let mut p = self.owned_projection(base)?;
                let index = p.ty.fields.iter().position(|(field, _, _)| field == name)?;
                let (_, offset, ty) = &p.ty.fields[index];
                p.components = ty
                    .components
                    .iter()
                    .map(|(_, at)| {
                        p.components[p
                            .ty
                            .components
                            .iter()
                            .position(|(_, word)| *word == offset + at)
                            .unwrap()]
                    })
                    .collect();
                p.ty = (**ty).clone();
                p.path.push(index);
                Some(p)
            }
            _ => None,
        }
    }
    fn projection_place(&self, p: &OwnedProjection) -> Option<Place> {
        let Binding::OwnedValue(owner) = self.env.get(&p.root)? else {
            return None;
        };
        Some(owner.place()?.project(p.ty.clone(), &p.components))
    }
    pub(super) fn projected_owned(&mut self, p: &OwnedProjection, span: Span) -> Result<Returned> {
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
                if let Some(place) = self.projection_place(p) {
                    self.read_place(&place, span).map(Returned::value)
                } else {
                    Ok(Returned::Value(value))
                }
            }
            Returned::Dataset(data) => {
                let mut data=data;data.residency=self.resolve_owned_residency(&data.residency,None);self.application_available(&data,span)?;self.prepare_owned_read(&Returned::Dataset(data.clone()));
                Ok(Returned::Dataset(self.refresh_application_data(data,span)?))
            }
            other => Ok(other),
        }
    }
    pub(super) fn application_reference(
        &mut self,
        expr: &Expr,
    ) -> Result<Option<application::DataReference>> {
        if let ExprKind::Name(name) = &expr.kind {
            return self
                .application_value(name, expr.span)
                .map(|data| data.map(application::DataReference::direct));
        }
        let Some(p) = self
            .owned_projection(expr)
            .filter(|p| p.ty.dataset.is_some())
        else {
            return self.indexed_application_reference(expr);
        };
        let Returned::Dataset(data) = self.projected_owned(&p, expr.span)? else {
            unreachable!()
        };
        self.application_available(&data, expr.span)?;
        let Binding::OwnedValue(root) = self.env.get_mut(&p.root).unwrap() else {
            unreachable!()
        };
        let Returned::Dataset(meta) = metadata_mut(root, &p.path) else {
            unreachable!()
        };
        meta.handle.proof = data.handle.proof.clone();
        meta.length.proof = data.length.proof.clone();
        Ok(Some(application::DataReference::direct(data)))
    }
    #[inline(never)]
    pub(super) fn member_returned(
        &mut self,
        expr: &Expr,
        base: &Expr,
        name: &String,
    ) -> Result<Option<Returned>> {
        if self
            .owned_element_type(expr)
            .is_some_and(|ty| ty.owns_resources())
        {
            let Some(place)=self.place(expr)?else{return Ok(None)};
            return self.take_owned_place(place,expr.span).map(Some);
        }
        if let Some(p) = self.owned_projection(expr) {
            return self.projected_owned(&p, expr.span).map(Some);
        }
        if let Some(data) = self.application_reference(base)? {
            return if name == "length" {
                Ok(Some(Returned::value(data.length.clone())))
            } else {
                self.fail(expr.span, "B141", "dataset只公开只读length")
            };
        }
        if self.owned_element_type(base).is_some_and(|ty|ty.has_variants()) {return self.fail(expr.span,"B269","变体载荷通过match使用");}
        if !self.temporary_projection(base)
            && (self.is_place(expr)
                || matches!(&base.kind,ExprKind::Name(n) if matches!(self.env.get(n),Some(Binding::Data(_)|Binding::Tile(_)|Binding::Texture(_)))))
        {
            return self
                .member_expr(base, name, expr)
                .map(|v| v.map(Returned::value));
        }
        let value = self.evaluate_returned(base, None)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        match value {
            Some(Returned::OwnedValue(mut owner)) => {
                let Some(i) = owner
                    .value
                    .ty
                    .fields
                    .iter()
                    .position(|(field, _, _)| field == name)
                else {
                    return self.fail(expr.span, "B141", "拥有值字段不存在");
                };
                let item = self.unpack_record(&owner, i);
                if owner.value.ty.aggregate_part(i).1.owns_resources() {
                    match owner.payloads[i].as_deref_mut().unwrap() {
                        Returned::Dataset(d) => d.ownership = Ownership::Moved,
                        Returned::OwnedValue(v) => v.mark_moved(),
                        _ => unreachable!(),
                    }
                }
                let retained=item.view_value().map(|value|value.proof.typed_view_borrow(&value.ty)).unwrap_or_default();
                self.drop_returned(Returned::OwnedValue(owner),&retained,expr.span)?;
                Ok(Some(item))
            }
            Some(Returned::Dataset(data)) => {
                if name != "length" {
                    return self.fail(expr.span, "B141", "dataset只公开只读length");
                }
                let length = data.length.clone();
                self.drop_returned(Returned::Dataset(data),&view::Borrow::default(),expr.span)?;
                Ok(Some(Returned::value(length)))
            }
            Some(Returned::Value(value)) => self
                .member_value(*value, name, expr.span)
                .map(|v| Some(Returned::value(v))),
            _ => self.fail(expr.span, "B228", "字段访问需要值"),
        }
    }
}

impl Binding {
    pub(super) fn proof_storage(&mut self) -> Option<(&mut bounds::Proof, &mut bool, Place)> {
        match self {
            Binding::Owned(owner) => {
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
            Binding::Owned(owner) => Some(owner.place.clone()),
            Binding::OwnedValue(owner) => owner.place(),
            _ => None,
        }
    }
}

impl Builder<'_> {
    fn owned_expression_type(&self, expr: &Expr) -> Option<Ty> {
        match &expr.kind {
            ExprKind::Name(name) => match self.env.get(name)? {
                Binding::OwnedValue(owner) => Some(owner.value.ty.clone()),
                _ => None,
            },
            ExprKind::Member(base, field) => self
                .owned_expression_type(base)?
                .field(field)
                .map(|(_, ty)| ty.clone()),
            ExprKind::Index(base, _) => self.owned_expression_type(base)?.array.map(|ty| *ty),
            _ => None,
        }
    }
    pub(super) fn owned_element_type(&self, expr: &Expr) -> Option<Ty> {
        self.owned_expression_type(expr)
    }
    #[inline(never)]
    pub(super) fn owned_array_index(
        &mut self,
        expr: &Expr,
        base: &Expr,
        index: &Expr,
    ) -> Result<Option<Returned>> {
        if self.owned_expression_type(base).is_some() {
            let Some(place)=self.place(expr)?else{return Ok(None)};
            return self.take_owned_place(place,expr.span).map(Some);
        }
        let returned = self.evaluate_returned(base, None)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let owner = match returned {
            Some(Returned::OwnedValue(owner)) if owner.value.ty.array.is_some() => owner,
            Some(Returned::Dataset(data)) => {
                let held = self.hold_binding(Binding::ApplicationData(data.clone()));
                let Some(place) =
                    self.application_index(application::DataReference::direct(data), index)?
                else {
                    return Ok(None);
                };
                let value = self.read_place(&place, expr.span)?;
                self.finish_scoped_argument(&held, expr.span)?;
                return Ok(Some(Returned::value(value)));
            }
            Some(Returned::Value(value)) if value.ty.array.is_some() => {
                return self
                    .array_value_index(*value, index)
                    .map(|v| v.map(Returned::value));
            }
            Some(Returned::Value(value)) => {
                let Some(place) = self.view_value_place(*value, index)? else {
                    return Ok(None);
                };
                return self
                    .read_place(&place, expr.span)
                    .map(|v| Some(Returned::value(v)));
            }
            _ => return self.fail(expr.span, "B224", "拥有数组元素需要实际拥有数组值"),
        };
        let owner=self.owned_value_binding(owner,true);let place=owner.place().unwrap();let held=self.hold_binding(Binding::OwnedValue(owner));
        let Some(value)=self.expr(index)?else{return Ok(None)};let place=self.array_index(place,value,index.span)?;let item=self.take_owned_place(place,expr.span)?;
        self.finish_scoped_argument(&held,expr.span)?;
        Ok(Some(item))
    }
}

impl Builder<'_> {
    fn indexed_application_reference(
        &mut self,
        expr: &Expr,
    ) -> Result<Option<application::DataReference>> {
        if !self.owned_element_type(expr).is_some_and(|ty|ty.dataset.is_some()) {
            return Ok(None);
        };
        let Some(place) = self.place(expr)? else {
            return Ok(None);
        };
        let candidates=self.owned_place_candidates(&place);
        let model=self.owned_place_model(&place);self.prepare_owned_read(&model);self.require_initialized(&place,expr.span)?;
        let Returned::Dataset(mut data)=model else{unreachable!()};self.application_available(&data,expr.span)?;

        let sources=candidates.iter().map(|(_,_,model)|{let Returned::Dataset(data)=model else{unreachable!()};data.handle.identity()}).collect();
        data=self.refresh_application_data(data,expr.span)?;data.ownership=Ownership::Borrowed;data.place=Some(place);
        Ok(Some(application::DataReference { data, sources }))
    }
}

impl Builder<'_> {
    pub(super) fn temporary_projection(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Index(base, _) | ExprKind::Member(base, _) => self.temporary_projection(base),
            ExprKind::Call(..)
            | ExprKind::Specialize(..)
            | ExprKind::Construct(..)
            | ExprKind::Select(..)
            | ExprKind::Target(..) => true,
            _ => false,
        }
    }
}
