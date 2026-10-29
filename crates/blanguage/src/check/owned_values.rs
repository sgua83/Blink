//! 拥有值沿原 Frame 持有资源；记录按字段组合，结果判别实际交付或释放的载荷。
use super::*;
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
    pub(super) fn join(&mut self, other: OwnedValue) {
        if self.ownership != other.ownership {
            self.ownership = Ownership::Moved;
        }
        self.value.proof = bounds::Proof::common(&self.value, &other.value);
        self.value.uniform &= other.value.uniform;
        if self.value.constant != other.value.constant {
            self.value.constant = None;
        }
        self.join_payloads(other.payloads);
    }
    pub(super) fn join_payloads(&mut self, payloads: Vec<Option<Box<Returned>>>) {
        for (current, other) in self.payloads.iter_mut().zip(payloads) {
            if let Some(other) = other {
                if let Some(current) = current {
                    current.join(*other);
                } else {
                    *current = Some(other);
                }
            }
        }
    }
    pub(super) fn all_available(&self) -> bool {
        self.ownership==Ownership::Owned && self.available()
    }
    pub(super) fn available(&self)->bool {
        self.ownership.available()
            && self.payloads.iter().enumerate().all(|(i, p)| {
                if !self.value.ty.has_variants() && !self.value.ty.aggregate_part(i).1.owns_resources() {
                    return true;
                }
                match p.as_deref() {
                    Some(Returned::Dataset(d)) => d.ownership.available(),
                    Some(Returned::OwnedValue(v)) => v.available(),
                    None => self.value.ty.has_variants(),
                    _ => true,
                }
            })
    }
    pub(super) fn borrow(&mut self) {
        self.ownership=Ownership::Borrowed;
        for item in self.payloads.iter_mut().flatten() {
            match item.as_mut() {
                Returned::Dataset(data)=>{
                    data.ownership=Ownership::Borrowed;data.writable=false;
                    if data.handle.proof.view_borrow(0).empty(){data.handle.proof=data.handle.proof.clone().with_view_sources(0,BTreeSet::from([view::Source::Data(data.handle.identity())]));}
                }
                Returned::OwnedValue(value)=>value.borrow(),_=>{},
            }
        }
    }
    pub(super) fn incomplete(&self) -> bool {
        self.payloads.iter().enumerate().any(|(i,p)|{
            if !self.value.ty.has_variants() && !self.value.ty.aggregate_part(i).1.owns_resources(){return false;}
            p.is_none() || matches!(p.as_deref(),Some(Returned::OwnedValue(v)) if v.incomplete()) || matches!(p.as_deref(),Some(Returned::Dataset(d)) if d.ownership==Ownership::Moved)
        })
    }
    pub(super) fn loop_compatible(
        &self,
        other: &Self,
        inputs: &mut residency::Inputs,
        source: &Source,
        span: Span,
        path: &bounds::Clause,
    ) -> bool {
        self.ownership == other.ownership
            && self.payloads.iter().zip(&other.payloads).all(|(a, b)| {
                match (a.as_deref(), b.as_deref()) {
                    (Some(Returned::Dataset(a)), Some(Returned::Dataset(b))) => {
                        a.ownership == b.ownership
                            && (a.ownership == Ownership::Moved
                                || inputs.same(&a.residency, &b.residency, source, span,path))
                    }
                    (Some(Returned::OwnedValue(a)), Some(Returned::OwnedValue(b))) => {
                        a.loop_compatible(b, inputs, source, span,path)
                    }
                    (None, Some(_)) => false,
                    _ => true,
                }
            })
    }
    pub(super) fn invalidate(&mut self) {
        self.value.constant = None;
        self.value.proof = self.value.proof.borrow_state();
        self.value.uniform = false;
        for item in self.payloads.iter_mut().flatten() {
            match item.as_mut() {
                Returned::Dataset(data) => {
                    data.handle.proof = data.handle.proof.borrow_state();
                    data.length.proof = data
                        .count
                        .map(|n| bounds::Proof::unsigned(n, n))
                        .unwrap_or_default();
                    data.length.constant = None;
                    data.length.uniform = false;
                }
                Returned::OwnedValue(value) => value.invalidate(),
                Returned::Value(value) => {
                    value.proof = bounds::Proof::default();
                    value.constant = None;
                    value.uniform = false;
                }
                Returned::Unit => {}
            }
        }
    }
    pub(super) fn invalidate_components(
        &mut self,
        components: &[usize],
        borrowed: Option<ValueIdentity>,
    ) {
        if self.value.ty.has_variants() {
            self.invalidate();
            return;
        }
        self.value.proof.assign(
            components,
            self.value.ty.components.len(),
            bounds::Proof::default(),
        );
        self.value.constant = None;
        self.value.uniform = false;
        for i in 0..self.value.ty.aggregate_len() {
            let (offset, ty) = self.value.ty.aggregate_part(i);
            if !ty.owns_resources() {
                continue;
            }
            let selected: Vec<_> = ty
                .components
                .iter()
                .enumerate()
                .filter_map(|(j, (_, at))| {
                    let at = self
                        .value
                        .ty
                        .components
                        .iter()
                        .position(|(_, word)| *word == offset + at)
                        .unwrap();
                    components.contains(&at).then_some(j)
                })
                .collect();
            if selected.is_empty() {
                continue;
            }
            if let Some(item) = &mut self.payloads[i] {
                match item.as_mut() {
                    Returned::OwnedValue(v) => v.invalidate_components(&selected, borrowed),
                    Returned::Dataset(d) => {
                        if borrowed.is_some_and(|id| d.handle.proof.known_identity(0) == Some(id)) {
                            continue;
                        }
                        d.handle.proof = d.handle.proof.borrow_state();
                        d.length.proof = d
                            .count
                            .map(|n| bounds::Proof::unsigned(n, n))
                            .unwrap_or_default();
                        d.length.constant = None;
                        d.length.uniform = false;
                    }
                    _ => {}
                }
            }
        }
    }
    pub(super) fn mark_moved(&mut self) {
        if self.value.ty.has_variants() || self.value.ty.cleanup.is_some() {
            self.ownership = Ownership::Moved;
            return;
        }
        self.ownership = Ownership::Owned;
        for item in self.payloads.iter_mut().flatten() {
            match item.as_mut() {
                Returned::Dataset(d) => {
                    d.ownership = Ownership::Moved;
                    d.residency = residency::Residency::default();
                }
                Returned::OwnedValue(v) => v.mark_moved(),
                _ => {}
            }
        }
    }
    pub(super) fn handles(&self) -> Vec<ValueIdentity> {
        self.payloads
            .iter()
            .flatten()
            .flat_map(|item| match item.as_ref() {
                Returned::Dataset(d) => vec![d.handle.identity()],
                Returned::OwnedValue(v) => v.handles(),
                _ => Vec::new(),
            })
            .collect()
    }
    pub(super) fn resources(&self) -> Vec<&Data> {
        if self.ownership == Ownership::Moved {
            return Vec::new();
        }
        self.payloads
            .iter()
            .flatten()
            .flat_map(|item| match item.as_ref() {
                Returned::Dataset(d) => vec![d],
                Returned::OwnedValue(v) => v.resources(),
                _ => Vec::new(),
            })
            .collect()
    }
    pub(super) fn resources_mut(&mut self, visit: &mut impl FnMut(&mut Data)) {
        if self.ownership == Ownership::Moved {
            return;
        }
        for item in self.payloads.iter_mut().flatten() {
            match item.as_mut() {
                Returned::Dataset(d) => visit(d),
                Returned::OwnedValue(v) => v.resources_mut(visit),
                _ => {}
            }
        }
    }
}
impl Builder<'_> {
    pub(super) fn returned_storage(&mut self, value: &Returned) -> Value {
        match value {
            Returned::Value(value) => value.clone(),
            Returned::OwnedValue(value) => value.value.clone(),
            Returned::Unit => Value {
                ty: Ty::unit(),
                regs: Vec::new(),
                proof: bounds::Proof::default(),
                uniform: true,
                literal: false,
                constant: Some(Vec::new()),
            },
            Returned::Dataset(data) => {
                let mut proof = bounds::Proof::default();
                proof.append(&data.handle);
                proof.append(&data.length);
                Value {
                    ty: Ty::owned_dataset(data.ty.clone(), data.count),
                    regs: vec![data.handle.regs[0], data.length.regs[0]],
                    proof,
                    uniform: data.handle.uniform && data.length.uniform,
                    literal: false,
                    constant: None,
                }
            }
        }
    }
    pub(super) fn expected_storage(&self, ty: &TypeName, span: Span) -> Result<Ty> {
        let definition = &self.definition;
        let value = self.world.ty(
            ty,
            &definition.source,
            span,
            &definition.module,
            &definition.imports,
        )?;
        if ty.dataset {
            let count = self.world.shape(
                &ty.count,
                &definition.source,
                span,
                &definition.module,
                &definition.imports,
            )?;
            Ok(Ty::owned_dataset(value, count))
        } else {
            Ok(value)
        }
    }
    pub(super) fn check_returned_type(
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
                self.application_data_type(
                    data,
                    expected.dataset.as_deref().unwrap(),
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
    pub(super) fn require_returned_initialized(&self,value:&mut Returned,span:Span)->Result<()> {
        match value {
            Returned::Dataset(data)=>{self.require_application_initialized(data,span)?;data.initialization=None;},
            Returned::OwnedValue(value)=>{
                for data in value.resources(){self.require_application_initialized(data,span)?;}
                value.resources_mut(&mut |data|data.initialization=None);
            },
            _=>{},
        }
        Ok(())
    }
    pub(super) fn rebind_returned(&mut self, value: &mut Returned, regs: Vec<Reg>) {
        match value {
            Returned::Unit => {}
            Returned::Value(value) => value.regs = regs,
            Returned::OwnedValue(result) => {
                result.value.regs = regs;
                result.place = None;
            }
            Returned::Dataset(data) => {
                data.handle.regs = vec![regs[0]];
                data.handle.proof = bounds::Proof::capture(&data.handle);
                data.length.regs = vec![regs[1]];
                data.length.proof =
                    bounds::Proof::capture(&data.length).with_domains(Domains::from([
                        IndexDomain::ApplicationLength(data.handle.identity()),
                    ]));
            }
        }
    }
    pub(super) fn template_returned(&mut self, mut value: Value, available: bool) -> Returned {
        if value.ty.dataset.is_none(){value.proof.retain_view_borrows(&value.ty);}
        if let Some(element) = value.ty.dataset.as_deref() {
            let mut handle = self.value_component(&value, 0);
            handle.proof = bounds::Proof::capture(&handle);
            let mut length = self.value_component(&value, 1);
            let count = value.ty.values.first().copied();
            length.proof = bounds::Proof::capture(&length).with_domains(Domains::from([
                IndexDomain::ApplicationLength(handle.identity()),
            ]));
            if let Some(n) = count {
                length.proof = bounds::Proof::unsigned(n, n).with_domains(Domains::from([
                    IndexDomain::ApplicationLength(handle.identity()),
                ]));
                length.constant = Some(vec![n]);
            }
            Returned::Dataset(Box::new(DataPayload {
                handle,
                length,
                ty: element.clone(),
                count,
                residency: residency::Residency::cpu(),
                ownership: if available {
                    Ownership::Owned
                } else {
                    Ownership::Moved
                },
                writable: true,
                place: None,
                initialization: None,
            }))
        } else if value.ty.owns_resources() && !value.ty.has_variants() {
            let payloads = (0..value.ty.aggregate_len())
                .map(|i| {
                    if !value.ty.aggregate_part(i).1.owns_resources() {
                        return None;
                    }
                    let field = self.record_component(&value, i);
                    Some(Box::new(self.template_returned(field, available)))
                })
                .collect();
            let ownership=if value.ty.cleanup.is_some() && !available {Ownership::Moved}else{Ownership::Owned};
            Returned::OwnedValue(Box::new(OwnedValuePayload {
                value,
                payloads,
                ownership,
                place: None, declaration: 0,
            }))
        } else if value.ty.owns_resources() {
            let mut payloads = vec![None;value.ty.variants.len()];
            if !available {
                return Returned::OwnedValue(Box::new(OwnedValuePayload {
                    value,
                    payloads,
                    ownership: Ownership::Moved,
                    place: None, declaration: 0,
                }));
            }
            for index in 0..value.ty.variants.len() {
                let payload = self.variant_payload(&value,index);
                payloads[index] = Some(Box::new(match payload {
                    Some(value) => self.template_returned(value, available),
                    None => Returned::Unit,
                }));
            }
            Returned::OwnedValue(Box::new(OwnedValuePayload {
                value,
                payloads,
                ownership: if available {
                    Ownership::Owned
                } else {
                    Ownership::Moved
                },
                place: None, declaration: 0,
            }))
        } else if value.ty.name == "void" {
            Returned::Unit
        } else {
            Returned::Value(value)
        }
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
    pub(super) fn incoming_owned_value(&mut self, value: Value) -> OwnedValue {
        let Returned::OwnedValue(result) = self.template_returned(value, true) else {
            unreachable!()
        };
        result
    }
    pub(super) fn read_owned_aggregate(&mut self,expr:&Expr)->Result<Option<(OwnedValue,Option<String>)>> {
        if self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()){
            let Some(place)=self.place(expr)?else{return Ok(None)};let model=self.owned_place_model(&place);let Returned::OwnedValue(mut value)=model else{return self.fail(expr.span,"B224","read聚合需要拥有记录、数组或变体")};
            if !value.available(){return self.fail(expr.span,"B227","只读借用需要已初始化且未移动的全部活动资源");}self.prepare_owned_read(&Returned::OwnedValue(value.clone()));if !value.value.ty.has_variants(){self.require_initialized(&place,expr.span)?;}self.load_owned_value(&mut value,expr.span)?;value.borrow();return Ok(Some((value,None)));
        }

        if let Some(projection)=self.owned_projection(expr) {
            let Returned::OwnedValue(mut value)=self.projected_owned(&projection,expr.span)? else {
                return self.fail(expr.span,"B224","read聚合需要拥有记录、数组或变体");
            };
            if !value.available(){return self.fail(expr.span,"B227","只读借用需要已初始化且未移动的全部活动资源");}
            if !value.value.ty.has_variants() && let Some(place)=value.place(){self.require_initialized(&place,expr.span)?;}
            self.load_owned_value(&mut value,expr.span)?;value.borrow();return Ok(Some((value,None)));
        }
        if let ExprKind::Member(base,field) = &expr.kind {
            let Some((value,owner))=self.read_owned_aggregate(base)? else{return Ok(None)};
                let index=value.value.ty.fields.iter().position(|(name,_,_)|name==field).ok_or_else(||Diagnostic{source:self.definition.source.clone(),span:expr.span,code:"B141",message:"拥有值字段不存在".into()})?;
            let item=self.unpack_record(&value,index);
            let Returned::OwnedValue(value)=item else{return self.fail(expr.span,"B224","read聚合需要拥有记录、数组或变体")};
            return Ok(Some((value,owner)));
        }
        if let ExprKind::Index(base,index) = &expr.kind {
            let Some((value,mut owner))=self.read_owned_aggregate(base)? else{return Ok(None)};
            let Some(element)=value.value.ty.array.as_deref().cloned()else{return self.fail(expr.span,"B224","聚合只读下标需要固定拥有数组")};
            if owner.is_none(){owner=Some(self.hold_binding(Binding::OwnedValue(value.clone())));}
            let Some(index)=self.expr(index)? else{return Ok(None)};
            self.require_same(&Ty::scalar(Scalar::U32),&index.ty,expr.span)?;
            let count=value.value.ty.values[0];
            if !self.index_in(&index,IndexDomain::Fixed(count),count){return self.fail(expr.span,"B137","拥有数组下标有效性无法静态证明")};
            let item=if index.minimum()==index.maximum(){self.unpack_record(&value,index.minimum()as usize)}else{
                let mut selected=(0..count).filter(|i|*i>=index.minimum()&&*i<=index.maximum());
                let mut model=self.unpack_record(&value,selected.next().unwrap()as usize);
                for i in selected{model.join(self.unpack_record(&value,i as usize));}
                let place=self.materialize_array(value.value.clone());let place=self.array_index(place,index,expr.span)?;
                let mut regs=Vec::new();
                for(i,(scalar,_))in element.components.iter().enumerate(){regs.push(self.read_place(&place.clone().project(Ty::scalar(*scalar),&[i]),expr.span)?.regs[0]);}
                self.rebind_returned(&mut model,regs);model
            };
            let Returned::OwnedValue(value)=item else{return self.fail(expr.span,"B224","read聚合需要拥有记录、数组或变体")};
            return Ok(Some((value,owner)));
        }
        let returned=self.evaluate_returned(expr,None)?;
        if !self.flow.continues(){return Ok(None);}
        let Some(Returned::OwnedValue(mut value))=returned else{return self.fail(expr.span,"B224","read聚合需要实际拥有值或其临时值")};
        if !value.available(){return self.fail(expr.span,"B227","只读借用需要已初始化且未移动的全部活动资源");}
        let owner=self.hold_binding(Binding::OwnedValue(value.clone()));value.borrow();Ok(Some((value,Some(owner))))
    }
    // 资源只读取实际位置中的元数据；载荷仍由原Frame持有。
    pub(super) fn read_storage_value(&mut self,place:&Place,span:Span)->Result<Value> {
        let ty=place.ty().clone();let mut regs=Vec::new();
        for(index,(scalar,_))in ty.components.iter().enumerate(){regs.push(self.read_place(&place.clone().project(Ty::scalar(*scalar),&[index]),span)?.regs[0]);}
        let(proof,uniform)=self.read_owned_proof(place,&regs);
        Ok(Value{ty,regs,proof,uniform,literal:false,constant:None})
    }
    pub(super) fn write_storage_value(&mut self,place:&Place,value:&Value,span:Span)->Result<()> {
        for(index,(scalar,_))in value.ty.components.iter().enumerate(){self.write_place(&place.clone().project(Ty::scalar(*scalar),&[index]),self.value_component(value,index),span)?;}
        Ok(())
    }
    pub(super) fn load_owned_value(&mut self,result:&mut OwnedValue,span:Span)->Result<()> {
        if let Some(place)=result.place(){let value=self.read_storage_value(&place,span)?;result.value.regs=value.regs;result.value.proof=bounds::Proof::capture(&result.value);}
        Ok(())
    }
    pub(super) fn take_owned_value(&mut self, name: &str, span: Span) -> Result<OwnedValue> {
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
        self.prepare_owned_read(&Returned::OwnedValue(result.clone()));
        if !result.value.ty.has_variants()
            && let Some(place) = result.place()
        {
            self.require_initialized(&place, span)?;
        }
        self.load_owned_value(&mut result,span)?;
        self.forget_owned_metadata(&Returned::OwnedValue(result.clone()));
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
            for(slot,reg)in slots.iter().zip(&result.value.regs){self.code.push(Inst::StoreLocal(*slot,*reg));}
            result.place=Some(Place::Local(result.value.ty.clone(),slots,storage));
        }
        if let Some(place) = result.place() {
            self.initialize(&place);
        }
        result
    }
    pub(super) fn bind_owned_value(
        &mut self,
        name: &String,
        result: OwnedValue,
        span: Span,
    ) -> Result<()> {
        if self.env.contains_key(name) {
            return self.fail(span, "B125", format!("重复局部变量或覆盖参数 {name}"));
        }
        let mutable = application::rebound(self.definition.function.body.as_ref().unwrap(), name)
            || !result.value.ty.has_variants();
        let mut result = self.owned_value_binding(result, mutable);
        result.declaration=span.start;
        self.env.insert(name.clone(), Binding::OwnedValue(result));
        Ok(())
    }
    pub(super) fn owned_value_declare(&mut self, name: &String, ty: Ty, span: Span) -> Result<()> {
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
            let Returned::OwnedValue(v) = self.template_returned(value, false) else {
                unreachable!()
            };
            v
        };
        let(slots,storage)=self.local_slots(&result.value.ty);
        result.place=Some(Place::Local(result.value.ty.clone(),slots,storage));
        result.declaration=span.start;
        self.env.insert(name.clone(), Binding::OwnedValue(result));
        Ok(())
    }
    fn make_owned_result(
        &mut self,
        ty: Ty,
        success: bool,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<OwnedValue>> {
        self.make_owned_variant(ty,usize::from(!success),args,span)
    }
    fn make_owned_variant(&mut self,ty:Ty,index:usize,args:&[Expr],span:Span)->Result<Option<OwnedValue>> {
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
            let result = self.evaluate_returned(&args[0], Some(&expected))?;
            if !self.flow.continues() {
                return Ok(None);
            }
            let mut value = result.ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
                span,
                code: "B228",
                message: "结果分支需要载荷".into(),
            })?;
            self.check_returned_type(&mut value, &expected, span)?;
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
        self.owned_variant_condition(value,0,span)
    }
    fn owned_variant_condition(&mut self,value:&OwnedValue,index:usize,span:Span)->Result<Value> {
        if value.payloads[index].is_none(){return Ok(self.constant(Scalar::Bool,0));}
        if value.payloads.iter().enumerate().all(|(i,p)|i==index||p.is_none()){return Ok(self.constant(Scalar::Bool,1));}
        self.variant_condition(&value.value,index,span)
    }
    fn unpack_owned(&mut self,value:&OwnedValue,success:bool)->Returned {self.unpack_variant(value,usize::from(!success))}
    fn unpack_variant(&mut self,value:&OwnedValue,index:usize)->Returned {
        let payload=self.variant_payload(&value.value,index);
        let Some(meta)=&value.payloads[index] else {return match payload {Some(value)=>self.template_returned(value,false),None=>Returned::Unit};};
        let mut item=(**meta).clone();if let Some(value)=payload {
            match &mut item {Returned::Value(current)=>*current=value,Returned::OwnedValue(current)=>{current.value=value;current.place=None;},_=>self.rebind_returned(&mut item,value.regs)}
        }item
    }
    pub(super) fn evaluate_returned(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Returned>> {
        self.view_uses.before(expr.span);
        if self.inside_user_owner(expr)&&self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()){return self.fail(expr.span,"B224","拥有包装的资源载荷须用into_inner移出，不能部分移动其字段");}
        if let ExprKind::Call(name,args)=&expr.kind {
            if name=="resize" {return self.resize_application_dataset(args,expr.span).map(|value|value.map(Returned::Dataset));}
            if name=="own" {return self.make_user_owned(args,expr.span);}
            if name=="into_inner" {return self.into_user_inner(args,expr.span);}
        }
        let constructor = match &expr.kind {
            ExprKind::Call(name,args) => self.variant_constructor_type(name,None,expected,expr.span)?.map(|(ty,index)|(ty,index,args)),
            ExprKind::Specialize(name,types,args) => self.variant_constructor_type(name,Some(types),expected,expr.span)?.map(|(ty,index)|(ty,index,args)),
            _=>None,
        };
        if let Some((ty,index,args))=constructor {
            return if ty.owns_resources() {self.make_owned_variant(ty,index,args,expr.span).map(|v|v.map(Returned::OwnedValue))}
                else {self.value_variant_constructor(ty,index,args,expr.span).map(|v|v.map(Returned::Value))};
        }
        match &expr.kind {
            ExprKind::Target(target, call) => return self.target_call(*target, call, expr.span),
            ExprKind::Member(base, name) => return self.member_returned(expr, base, name),
            ExprKind::Index(base, index)
                if self
                    .owned_element_type(base)
                    .is_some_and(|ty| ty.array.is_some() && ty.owns_resources())
                    || self.temporary_projection(base) =>
            {
                return self.owned_array_index(expr, base, index);
            }
            ExprKind::Construct(name, args) => {
                let ty = self.expected_storage(name, expr.span)?;
                if ty.owns_resources() {
                    return self.make_owned_record(ty, args, expr.span);
                }
            }
            ExprKind::Name(name)
                if matches!(self.env.get(name), Some(Binding::ApplicationData(_))) =>
            {
                return self
                    .move_application_dataset(name, expr.span)
                    .map(|v| v.map(Returned::Dataset));
            }
            ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::OwnedValue(_))) => {
                return self
                    .take_owned_value(name, expr.span)
                    .map(|v| Some(Returned::OwnedValue(v)));
            }
            ExprKind::Specialize(name, _, _) if name == "dataset" => {
                return self
                    .create_application_dataset(expr, expr.span)
                    .map(|v| v.map(Returned::Dataset));
            }
            ExprKind::Call(name, args)
                if matches!(name.as_str(), "Ok" | "Err")
                    && expected.is_some_and(Ty::owns_resources) =>
            {
                return self
                    .make_owned_result(expected.unwrap().clone(), name == "Ok", args, expr.span)
                    .map(|v| v.map(Returned::OwnedValue));
            }
            ExprKind::Specialize(name, explicit, args) if matches!(name.as_str(), "Ok" | "Err") => {
                let mut syntax = TypeName::named("result");
                syntax.arguments = explicit.clone();
                let ty = self.expected_storage(&syntax, expr.span)?;
                if ty.owns_resources() {
                    return self
                        .make_owned_result(ty, name == "Ok", args, expr.span)
                        .map(|v| v.map(Returned::OwnedValue));
                }
            }
            ExprKind::Call(name, args) if name == "try" => {
                return self.propagate_returned(args, expr.span);
            }
            ExprKind::Select(c, a, b) => {
                return self.selection_returned(c, a, b, expr.span, expected);
            }
            ExprKind::Call(name, args) | ExprKind::Specialize(name, _, args) => {
                if matches!(name.as_str(),"array"|"owned")
                    || self
                        .world
                        .resolve(
                            name,
                            &self.definition.module,
                            &self.definition.imports,
                            false,
                        )
                        .is_some()
                {
                    let mut syntax = TypeName::named(name);
                    if let ExprKind::Specialize(_, explicit, _) = &expr.kind {
                        syntax.arguments = explicit.clone();
                    }
                    let ty = self.expected_storage(&syntax, expr.span)?;
                    if ty.owns_resources() {
                        return self.make_owned_record(ty, args, expr.span);
                    }
                }
                if let Some(key) = self.world.resolve(
                    name,
                    &self.definition.module,
                    &self.definition.imports,
                    true,
                ) {
                    let explicit = if let ExprKind::Specialize(_, args, _) = &expr.kind {
                        Some(args.as_slice())
                    } else {
                        None
                    };
                    let Some(arguments) = self.function_arguments(&key, args, expr.span)? else {
                        return Ok(None);
                    };
                    let instance =
                        self.specialized_arguments(key, &arguments, explicit, expr.span)?;
                    return self.invoke_returned(instance, arguments, expr.span);
                }
            }
            _ => {}
        }
        if let ExprKind::Call(name, args) = &expr.kind {
            if matches!(name.as_str(), "Ok" | "Err") {
                return self
                    .expr_expected(expr, expected)
                    .map(|v| v.map(Returned::Value));
            }
            return self
                .builtin_call(name, args, expr.span)
                .map(|v| v.map(Returned::Value));
        }
        self.expr_expected(expr, expected)
            .map(|v| v.map(Returned::Value))
    }
    pub(super) fn propagate_returned(
        &mut self,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Returned>> {
        if args.len() != 1 {
            return self.fail(span, "B269", "try需要一个结果表达式");
        }
        let returned = self.evaluate_returned(&args[0], None)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let source = returned.ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B269",
            message: "try需要结果".into(),
        })?;
        let (value, owned) = match &source {
            Returned::OwnedValue(r) if r.value.ty.result => (r.value.clone(), Some(r)),
            Returned::Value(v) if v.ty.result => (v.clone(), None),
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
        self.conditional_blocks(
            condition,
            |_| Ok(()),
            |b| {
                let error = if let Some(r) = owned {
                    b.unpack_owned(r, false)
                } else {
                    b.result_payload(&value, false)
                        .map_or(Returned::Unit, Returned::Value)
                };
                let result = b.wrap_returned_result(expected, false, error);
                b.finish_return(Some(result), span)
            },
            span,
        )?;
        if !self.flow.continues() {
            return Ok(None);
        }
        Ok(Some(if let Some(r) = owned {
            self.unpack_owned(r, true)
        } else {
            match self.result_payload(&value, true) {
                Some(v) => Returned::Value(v),
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
            Returned::Value(value)
        }
    }
    pub(super) fn match_returned(&mut self, expr: &Expr, arms: &[MatchArm], span: Span) -> Result<()> {
        let mut root=expr;
        while let ExprKind::Member(base,_)|ExprKind::Index(base,_)=&root.kind{root=base;}
        let borrowed=self.owned_element_type(expr).is_some_and(|ty|ty.owns_resources()) && matches!(&root.kind,ExprKind::Name(name)if matches!(self.env.get(name),Some(Binding::OwnedValue(value))if value.ownership==Ownership::Borrowed));
        let (returned,held)=if borrowed {match self.read_owned_aggregate(expr)? {Some((value,held))=>(Some(Returned::OwnedValue(value)),held),None=>(None,None)}}else{(self.evaluate_returned(expr,None)?,None)};
        if !self.flow.continues() { return Ok(()); }
        let returned = returned.ok_or_else(|| Diagnostic { source:self.definition.source.clone(),span,code:"B269",message:"match需要变体值".into() })?;
        let value = self.returned_storage(&returned);
        let selected = self.check_variant_patterns(&value.ty,arms,span)?;
        self.match_variant_arms(&returned,&selected,0,span)?;
        if let Some(name)=held {if self.flow.continues(){self.finish_scoped_argument(&name,span)?;}else{self.env.remove(&name);}}
        Ok(())
    }
    fn match_variant_arms(&mut self, value: &Returned, arms: &[&MatchArm], index: usize, span: Span) -> Result<()> {
        if index+1==arms.len() { return self.match_variant_arm(value,index,arms[index]); }
        let condition = match value {
            Returned::Value(value) => self.variant_condition(value,index,span)?,
            Returned::OwnedValue(value) => self.owned_variant_condition(value,index,span)?,
            _=>unreachable!(),
        };
        self.conditional_blocks(condition, |b|b.match_variant_arm(value,index,arms[index]), |b|b.match_variant_arms(value,arms,index+1,span),span)
    }
    fn match_variant_arm(&mut self, value: &Returned, index: usize, arm: &MatchArm) -> Result<()> {
        match value {
            Returned::Value(value)=>self.value_variant_arm(value,index,arm),
            Returned::OwnedValue(value)=>self.owned_arm(value,index,&arm.binding,&arm.body,arm.span),
            _=>unreachable!(),
        }
    }
    fn owned_arm(&mut self,value:&OwnedValue,index:usize,name:&Option<String>,body:&[Stmt],span:Span)->Result<()> {
        let item=self.unpack_variant(value,index);
        if let Some(name)=name.as_ref().filter(|n|n.as_str()!="_") {
            self.bind_returned(name,item,span)?;self.block(body)?;
            if self.flow.continues() { if let Some(binding)=self.env.remove(name) {self.drop_binding(binding,&view::Borrow::default(),span)?;} }
            else {self.env.remove(name);}
            Ok(())
        } else {self.drop_returned(item,&view::Borrow::default(),span)?;self.block(body)}
    }
    pub(super) fn bind_returned(
        &mut self,
        name: &String,
        value: Returned,
        span: Span,
    ) -> Result<()> {
        match value {
            Returned::Value(value) => self.bind_local_value(name, value, span),
            Returned::OwnedValue(value) => self.bind_owned_value(name, value, span),
            Returned::Dataset(data) => {
                if self.env.contains_key(name) {
                    return self.fail(span, "B125", "重复局部绑定");
                }
                let mutable =
                    application::rebound(self.definition.function.body.as_ref().unwrap(), name);
                let data = self.application_bind(data, mutable);
                self.env
                    .insert(name.clone(), Binding::ApplicationData(data));
                Ok(())
            }
            Returned::Unit => self.fail(span, "B228", "void分支没有可绑定值"),
        }
    }
    pub(super) fn drop_returned(&mut self,value:Returned,retained:&view::Borrow,span:Span) -> Result<()> {
        self.prepare_owned_read(&value);
        match value {
            Returned::Dataset(mut data) if matches!(data.ownership,Ownership::Conditional(_))=>{let Ownership::Conditional(when)=data.ownership.clone()else{unreachable!()};data.ownership=Ownership::Owned;return self.owned_when(&when,span,|builder|{builder.prepare_owned_read(&Returned::Dataset(data.clone()));builder.drop_returned(Returned::Dataset(data),retained,span)})}
            Returned::OwnedValue(mut value) if matches!(value.ownership,Ownership::Conditional(_))=>{let Ownership::Conditional(when)=value.ownership.clone()else{unreachable!()};value.ownership=Ownership::Owned;return self.owned_when(&when,span,|builder|{builder.prepare_owned_read(&Returned::OwnedValue(value.clone()));builder.drop_returned(Returned::OwnedValue(value),retained,span)})}
            Returned::Dataset(data) if data.ownership == Ownership::Owned => {
                self.application_frame(span)?;
                self.application_drop(&data,retained,span)?;
            }
            Returned::OwnedValue(mut result) if result.ownership == Ownership::Owned => {
                if result.value.ty.cleanup.is_some(){return self.drop_user_owned(result,retained,span);}

                if !result.value.ty.has_variants() {
                    for i in (0..result.value.ty.aggregate_len()).rev() {
                        if result.value.ty.aggregate_part(i).1.owns_resources() {
                            let field = self.unpack_record(&result, i);
                            self.drop_returned(field,retained,span)?;
                        }
                    }
                    return Ok(());
                }
                self.load_owned_value(&mut result,span)?;
                result.place=None;
                self.drop_variant(&result,0,retained,span)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn drop_variant(&mut self,value:&OwnedValue,index:usize,retained:&view::Borrow,span:Span)->Result<()> {
        if index+1==value.payloads.len() {
            let item=self.unpack_variant(value,index);return self.drop_returned(item,retained,span);
        }
        let condition=self.owned_variant_condition(value,index,span)?;
        if let Some(bits)=&condition.constant {
            if bits[0]!=0 {let item=self.unpack_variant(value,index);self.drop_returned(item,retained,span)}
            else {self.drop_variant(value,index+1,retained,span)}
        } else {
            let outer=std::mem::take(&mut self.code);
            let item=self.unpack_variant(value,index);self.drop_returned(item,retained,span)?;
            let yes=std::mem::take(&mut self.code);
            self.drop_variant(value,index+1,retained,span)?;
            let no=std::mem::replace(&mut self.code,outer);
            self.code.push(Inst::If{condition:condition.regs[0],yes,no});Ok(())
        }
    }
    pub(super) fn drop_binding(&mut self,binding:Binding,retained:&view::Borrow,span:Span) -> Result<()> {
        match binding {
            Binding::OwnedValue(r) => self.drop_returned(Returned::OwnedValue(r),retained,span),
            Binding::ApplicationData(data) => self.drop_returned(Returned::Dataset(data),retained,span),
            _ => Ok(()),
        }
    }
}

impl Builder<'_> {
    pub(super) fn hold_argument(
        &mut self,
        argument: &cooperative::Argument,
        access: Access,
    ) -> Option<String> {
        let binding = match argument {
            cooperative::Argument::BorrowedData(reference) => {
                Binding::DataReference(reference.clone())
            }
            cooperative::Argument::Exclusive(place) | cooperative::Argument::Atomic(place)
                if matches!(place.storage(), Place::ApplicationElement { .. }) =>
            {
                Binding::AtomicBorrow(place.clone())
            }
            cooperative::Argument::Value(value) if value.ty.contains_views() => Binding::Value(value.clone()),
            cooperative::Argument::OwnedValue(value) => Binding::OwnedValue(value.clone()),
            cooperative::Argument::OwnedReference{value,owner,..}=>{if let Some(owner)=owner{return Some(owner.clone());}let mut value=value.clone();value.borrow();Binding::OwnedValue(value)},
            cooperative::Argument::ApplicationData(data) if access == Access::Value => {
                Binding::ApplicationData(data.clone())
            }
            cooperative::Argument::Scoped { owner, .. } => return Some(owner.clone()),
            _ => return None,
        };
        Some(self.hold_binding(binding))
    }
    pub(super) fn hold_binding(&mut self, binding: Binding) -> String {
        let name = format!("\0argument:{}", self.env.len());
        self.env.insert(name.clone(), binding);
        name
    }
    pub(super) fn finish_arguments(
        &mut self,
        held: &[(usize, String)],
        arguments: &mut [cooperative::Argument],
    ) {
        for (index, name) in held {
            if matches!(arguments[*index], cooperative::Argument::Scoped { .. }|cooperative::Argument::OwnedReference{owner:Some(_),..}) {
                continue;
            }
            let value = self.env.remove(name).unwrap();
            if matches!(arguments[*index],cooperative::Argument::OwnedReference{..})||matches!(value, Binding::DataReference(_) | Binding::AtomicBorrow(_)) {
                continue;
            }
            arguments[*index] = match value {
                Binding::ApplicationData(data) => cooperative::Argument::ApplicationData(data),
                Binding::OwnedValue(value) => cooperative::Argument::OwnedValue(value),
                Binding::Value(value) => cooperative::Argument::Value(value),
                _ => unreachable!(),
            };
        }
    }
    pub(super) fn abandon_arguments(&mut self, held: &[(usize, String)]) {
        for (_, name) in held {
            self.env.remove(name);
        }
    }
    pub(super) fn scoped_argument(&mut self, expr: &Expr) -> Result<Option<cooperative::Argument>> {
        let value = self.evaluate_returned(expr, None)?;
        if !self.flow.continues() {
            return Ok(None);
        }
        let Some(Returned::Dataset(data)) = value else {
            return self.fail(
                expr.span,
                "B224",
                "dataset借用需要实际资源或拥有数据的临时值",
            );
        };
        let name = format!("\0argument:{}", self.env.len());
        let mut reference = data.clone();
        reference.ownership = Ownership::Borrowed;
        self.env
            .insert(name.clone(), Binding::ApplicationData(data));
        Ok(Some(cooperative::Argument::Scoped {
            value: Returned::Dataset(reference),
            owner: name,
        }))
    }
    pub(super) fn finish_scoped_argument(&mut self, name: &str, span: Span) -> Result<()> {
        let binding=self.env.remove(name).unwrap();
        self.drop_binding(binding,&view::Borrow::default(),span)
    }
}

impl crate::module::codec::Encode for OwnedValuePayload {
    fn encode(&self,w:&mut crate::module::codec::Writer){self.value.encode(w);self.payloads.encode(w);self.ownership.encode(w);}
}
impl crate::module::codec::Decode for OwnedValuePayload {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{Ok(Self{value:Value::decode(r)?,payloads:Vec::decode(r)?,ownership:Ownership::decode(r)?,place:None,declaration:0})}
}
