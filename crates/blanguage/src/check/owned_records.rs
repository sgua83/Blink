//! 记录的字段投影保持原局部槽、Frame 数据和每个资源叶的唯一拥有关系。
use super::*;
use application::{Data, Ownership};
use owned_values::OwnedValue;
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
        let (_, offset, ty) = &value.ty.fields[index];
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
            ty: (**ty).clone(),
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
            return Returned::Value(value);
        }
        let mut item = (**owner.payloads[index].as_ref().unwrap()).clone();
        self.rebind_returned(&mut item, value.regs.clone());
        if let Returned::OwnedValue(child) = &mut item {
            child.value = value;
        }
        if let Some(slots) = &owner.slots {
            let (_, offset, ty) = &owner.value.ty.fields[index];
            let selected: Vec<_> = ty
                .components
                .iter()
                .map(|(_, at)| {
                    slots[owner
                        .value
                        .ty
                        .components
                        .iter()
                        .position(|(_, word)| *word == offset + at)
                        .unwrap()]
                })
                .collect();
            match &mut item {
                Returned::Dataset(data) => data.slots = Some([selected[0], selected[1]]),
                Returned::OwnedValue(child) => child.slots = Some(selected),
                _ => {}
            }
        }
        item
    }
    pub(super) fn make_owned_record(
        &mut self,
        ty: Ty,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Returned>> {
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span, "B224", "拥有资源的记录由普通CPU应用函数组织");
        }
        if args.len() != ty.fields.len() {
            return self.fail(span, "B157", "记录构造需要按声明顺序提供每个字段");
        }
        let mut fields = Vec::new();
        let mut held = Vec::new();
        for ((_, _, field), arg) in ty.fields.iter().zip(args) {
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
            self.check_returned_type(&mut item, field, arg.span)?;
            let binding = match &item {
                Returned::Dataset(data) => Some(Binding::ApplicationData(data.clone())),
                Returned::OwnedValue(value) => Some(Binding::OwnedValue(value.clone())),
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
                _ => unreachable!(),
            };
        }
        let mut payloads = Vec::new();
        let mut values = Vec::new();
        for ((_, _, field), item) in ty.fields.iter().zip(fields) {
            values.push(self.returned_storage(&item));
            payloads.push(field.owns_resources().then(|| Box::new(item)));
        }
        let value = self.pack_record(ty, values, span)?;
        Ok(Some(Returned::OwnedValue(OwnedValue {
            value,
            payloads,
            ownership: Ownership::Owned,
            slots: None,
        })))
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
    fn projected_owned(&mut self, p: &OwnedProjection, span: Span) -> Result<Returned> {
        let Some(Binding::OwnedValue(root)) = self.env.get(&p.root).cloned() else {
            unreachable!()
        };
        let mut item = Returned::OwnedValue(root);
        for index in &p.path {
            item = match item {
                Returned::OwnedValue(owner) => self.unpack_record(&owner, *index),
                Returned::Value(value) => Returned::Value(self.record_component(&value, *index)),
                _ => unreachable!(),
            };
        }
        match item {
            Returned::Value(value) => {
                if let Some(place) = self.projection_place(p) {
                    self.read_place(&place, span).map(Returned::Value)
                } else {
                    Ok(Returned::Value(value))
                }
            }
            Returned::Dataset(data) if data.ownership != Ownership::Moved => {
                Ok(Returned::Dataset(self.refresh_application_data(data)))
            }
            other => Ok(other),
        }
    }
    pub(super) fn movable_returned(&self, item: &Returned, span: Span) -> Result<()> {
        match item {
            Returned::Dataset(d) if d.ownership == Ownership::Owned => {
                self.application_movable(d, span)
            }
            Returned::OwnedValue(v) => {
                for d in v.resources() {
                    if d.ownership == Ownership::Owned {
                        self.application_movable(d, span)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(super) fn take_owned_projection(
        &mut self,
        p: OwnedProjection,
        span: Span,
    ) -> Result<Returned> {
        if p.path.is_empty() {
            return self
                .take_owned_value(&p.root, span)
                .map(Returned::OwnedValue);
        }
        let mut item = self.projected_owned(&p, span)?;
        self.movable_returned(&item, span)?;
        match &mut item {
            Returned::Dataset(data) => {
                self.application_available(data, span)?;
                if data.ownership != Ownership::Owned {
                    return self.fail(span, "B227", "借用资源不能从字段移动");
                }
            }
            Returned::OwnedValue(owner) => {
                if !owner.all_available() {
                    return self.fail(span, "B227", "记录字段仍有未初始化或已经移动的资源");
                }
            }
            Returned::Value(_) => {}
            Returned::Unit => unreachable!(),
        }
        if let Some(place) = self.projection_place(&p) {
            self.require_initialized(&place, span)?;
        }
        match &mut item {
            Returned::Dataset(data) => data.slots = None,
            Returned::OwnedValue(value) => {
                self.load_owned_value(value);
                value.slots = None;
            }
            _ => {}
        }
        if p.ty.owns_resources() {
            let Binding::OwnedValue(root) = self.env.get_mut(&p.root).unwrap() else {
                unreachable!()
            };
            match metadata_mut(root, &p.path) {
                Returned::Dataset(d) => {
                    d.ownership = Ownership::Moved;
                    d.residency = None;
                }
                Returned::OwnedValue(v) => v.mark_moved(),
                _ => unreachable!(),
            }
        }
        Ok(item)
    }
    pub(super) fn application_reference(&mut self, expr: &Expr) -> Result<Option<Data>> {
        if let ExprKind::Name(name) = &expr.kind {
            return self.application_value(name, expr.span);
        }
        let Some(p) = self
            .owned_projection(expr)
            .filter(|p| p.ty.dataset.is_some())
        else {
            return Ok(None);
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
        Ok(Some(data))
    }
    pub(super) fn assign_owned_projection(
        &mut self,
        p: OwnedProjection,
        op: &str,
        rhs: &Expr,
        span: Span,
    ) -> Result<()> {
        if op != "=" {
            return self.fail(span, "B224", "资源字段只支持整份移动赋值");
        }
        let item = self.evaluate_returned(rhs, Some(&p.ty))?;
        if !self.flow.continues() {
            return Ok(());
        }
        let mut item = item.ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B228",
            message: "字段赋值需要值或拥有资源".into(),
        })?;
        self.check_returned_type(&mut item, &p.ty, span)?;
        let old = self.projected_owned(&p, span)?;
        self.movable_returned(&old, span)?;
        self.drop_returned(old, span)?;
        let value = self.returned_storage(&item);
        let Binding::OwnedValue(root) = self.env.get_mut(&p.root).unwrap() else {
            unreachable!()
        };
        let slots = root.slots.as_ref().unwrap();
        for (component, reg) in p.components.iter().zip(&value.regs) {
            self.code.push(Inst::StoreLocal(slots[*component], *reg));
            self.initialized.insert(Position::Local(slots[*component]));
        }
        root.value.proof.assign(
            &p.components,
            root.value.ty.components.len(),
            bounds::Proof::capture(&value),
        );
        root.value.constant = None;
        root.value.uniform = false;
        let old = metadata_mut(root, &p.path);
        if self.loop_depth > 0 {
            match (&mut item, &*old) {
                (Returned::OwnedValue(a), Returned::OwnedValue(b)) => {
                    a.join_payloads(b.payloads.clone())
                }
                _ => {}
            }
        }
        *old = item;
        Ok(())
    }
    pub(super) fn member_returned(
        &mut self,
        expr: &Expr,
        base: &Expr,
        name: &String,
    ) -> Result<Option<Returned>> {
        if let Some(p) = self.owned_projection(expr) {
            return self.take_owned_projection(p, expr.span).map(Some);
        }
        if let Some(data) = self.application_reference(base)? {
            return if name == "length" {
                Ok(Some(Returned::Value(data.length)))
            } else {
                self.fail(expr.span, "B141", "dataset只公开只读length")
            };
        }
        if self.is_place(expr)
            || matches!(&base.kind,ExprKind::Name(n) if matches!(self.env.get(n),Some(Binding::Data(_)|Binding::Tile(_)|Binding::Texture(_))))
        {
            return self
                .member_expr(base, name, expr)
                .map(|v| v.map(Returned::Value));
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
                if owner.value.ty.fields[i].2.owns_resources() {
                    match owner.payloads[i].as_deref_mut().unwrap() {
                        Returned::Dataset(d) => d.ownership = Ownership::Moved,
                        Returned::OwnedValue(v) => v.mark_moved(),
                        _ => unreachable!(),
                    }
                }
                self.drop_returned(Returned::OwnedValue(owner), expr.span)?;
                Ok(Some(item))
            }
            Some(Returned::Dataset(data)) => {
                if name != "length" {
                    return self.fail(expr.span, "B141", "dataset只公开只读length");
                }
                let length = data.length.clone();
                self.drop_returned(Returned::Dataset(data), expr.span)?;
                Ok(Some(Returned::Value(length)))
            }
            Some(Returned::Value(value)) => self
                .member_value(value, name, expr.span)
                .map(|v| Some(Returned::Value(v))),
            _ => self.fail(expr.span, "B228", "字段访问需要值"),
        }
    }
    pub(super) fn join_owned_releases(
        &mut self,
        a: Returned,
        b: &Returned,
        span: Span,
    ) -> Result<()> {
        match (a, b) {
            (Returned::Dataset(a), Returned::Dataset(b))
                if a.ownership == Ownership::Owned && b.ownership == Ownership::Moved =>
            {
                self.drop_returned(Returned::Dataset(a), span)
            }
            (Returned::OwnedValue(a), Returned::OwnedValue(b))
                if a.ownership == Ownership::Owned =>
            {
                if b.ownership == Ownership::Moved {
                    return self.drop_returned(Returned::OwnedValue(a), span);
                }
                if !a.value.ty.result {
                    for i in 0..a.value.ty.fields.len() {
                        if a.value.ty.fields[i].2.owns_resources() {
                            let x = self.unpack_record(&a, i);
                            let y = self.unpack_record(b, i);
                            self.join_owned_releases(x, &y, span)?;
                        }
                    }
                }
                Ok(())
            }
            _ => Ok(()),
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
