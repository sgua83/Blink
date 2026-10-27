//! 拥有值沿原 Frame 持有资源；记录按字段组合，结果判别实际交付或释放的载荷。
use super::*;
use application::{Data, Ownership};
use returns::Returned;

#[derive(Clone)]
pub(super) struct OwnedValue {
    pub value: Value,
    pub payloads: Vec<Option<Box<Returned>>>,
    pub ownership: Ownership,
    pub slots: Option<Vec<usize>>,
}
impl OwnedValue {
    pub(super) fn place(&self) -> Option<Place> {
        self.slots.as_ref().map(|slots| {
            let base = slots[0] - self.value.ty.components[0].1 as usize / 4;
            Place::Local(
                self.value.ty.clone(),
                slots.clone(),
                base..base + ir::align_up(self.value.ty.size, self.value.ty.align) as usize / 4,
            )
        })
    }
    pub(super) fn join(&mut self, other: Self) {
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
        self.ownership == Ownership::Owned
            && self.payloads.iter().enumerate().all(|(i, p)| {
                if !self.value.ty.result && !self.value.ty.fields[i].2.owns_resources() {
                    return true;
                }
                match p.as_deref() {
                    Some(Returned::Dataset(d)) => d.ownership == Ownership::Owned,
                    Some(Returned::OwnedValue(v)) => v.all_available(),
                    None => self.value.ty.result,
                    _ => true,
                }
            })
    }
    pub(super) fn incomplete(&self) -> bool {
        self.payloads.iter().enumerate().any(|(i,p)|{
            if !self.value.ty.result && !self.value.ty.fields[i].2.owns_resources(){return false;}
            p.is_none() || matches!(p.as_deref(),Some(Returned::OwnedValue(v)) if v.incomplete()) || matches!(p.as_deref(),Some(Returned::Dataset(d)) if d.ownership==Ownership::Moved)
        })
    }
    pub(super) fn loop_compatible(&self, other: &Self, inputs: &residency::Inputs) -> bool {
        self.ownership == other.ownership
            && self.payloads.iter().zip(&other.payloads).all(|(a, b)| {
                match (a.as_deref(), b.as_deref()) {
                    (Some(Returned::Dataset(a)), Some(Returned::Dataset(b))) => {
                        a.ownership == b.ownership
                            && (a.ownership == Ownership::Moved || inputs.same(&a.residency, &b.residency))
                    }
                    (Some(Returned::OwnedValue(a)), Some(Returned::OwnedValue(b))) => {
                        a.loop_compatible(b, inputs)
                    }
                    (None, Some(_)) => false,
                    _ => true,
                }
            })
    }
    pub(super) fn invalidate(&mut self) {
        self.value.constant = None;
        self.value.proof = bounds::Proof::default();
        self.value.uniform = false;
        for item in self.payloads.iter_mut().flatten() {
            match item.as_mut() {
                Returned::Dataset(data) => {
                    data.handle.proof = bounds::Proof::default();
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
        if self.value.ty.result {
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
        for i in 0..self.value.ty.fields.len() {
            let (_, offset, ty) = &self.value.ty.fields[i];
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
                        d.handle.proof = bounds::Proof::default();
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
        if self.value.ty.result {
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
    pub(super) fn rebind_returned(&mut self, value: &mut Returned, regs: Vec<Reg>) {
        match value {
            Returned::Unit => {}
            Returned::Value(value) => value.regs = regs,
            Returned::OwnedValue(result) => {
                result.value.regs = regs;
                result.slots = None;
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
    pub(super) fn template_returned(&mut self, value: Value, available: bool) -> Returned {
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
            Returned::Dataset(Data {
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
                slots: None,
                initialization: None,
            })
        } else if value.ty.owns_resources() && !value.ty.result {
            let payloads = (0..value.ty.fields.len())
                .map(|i| {
                    if !value.ty.fields[i].2.owns_resources() {
                        return None;
                    }
                    let field = self.record_component(&value, i);
                    Some(Box::new(self.template_returned(field, available)))
                })
                .collect();
            Returned::OwnedValue(OwnedValue {
                value,
                payloads,
                ownership: Ownership::Owned,
                slots: None,
            })
        } else if value.ty.owns_resources() {
            let mut payloads = vec![None, None];
            if !available {
                return Returned::OwnedValue(OwnedValue {
                    value,
                    payloads,
                    ownership: Ownership::Moved,
                    slots: None,
                });
            }
            for (index, success) in [true, false].into_iter().enumerate() {
                let payload = self.result_payload(&value, success);
                payloads[index] = Some(Box::new(match payload {
                    Some(value) => self.template_returned(value, available),
                    None => Returned::Unit,
                }));
            }
            Returned::OwnedValue(OwnedValue {
                value,
                payloads,
                ownership: if available {
                    Ownership::Owned
                } else {
                    Ownership::Moved
                },
                slots: None,
            })
        } else if value.ty.name == "void" {
            Returned::Unit
        } else {
            Returned::Value(value)
        }
    }
    fn value_component(&self, value: &Value, index: usize) -> Value {
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
    pub(super) fn load_owned_value(&mut self, result: &mut OwnedValue) {
        if let Some(slots) = &result.slots {
            result.value.regs = slots
                .iter()
                .map(|slot| {
                    let reg = self.reg(self.locals[*slot]);
                    self.code.push(Inst::LoadLocal(reg, *slot));
                    reg
                })
                .collect();
            result.value.proof = bounds::Proof::capture(&result.value);
        }
    }
    pub(super) fn take_owned_value(&mut self, name: &str, span: Span) -> Result<OwnedValue> {
        let Some(Binding::OwnedValue(mut result)) = self.env.get(name).cloned() else {
            unreachable!()
        };
        if result
            .handles()
            .iter()
            .any(|id| self.application_borrowed(*id))
        {
            return self.fail(span, "B227", "资源字段仍被游标或位置借用，不能移动拥有值");
        }
        if !result.all_available() {
            return self.fail(span, "B227", "拥有值存在未初始化或已移动的资源字段");
        }
        if !result.value.ty.result
            && let Some(place) = result.place()
        {
            self.require_initialized(&place, span)?;
        }
        self.load_owned_value(&mut result);
        if let Some(Binding::OwnedValue(source)) = self.env.get_mut(name) {
            source.mark_moved();
        }
        result.slots = None;
        Ok(result)
    }
    pub(super) fn owned_value_binding(
        &mut self,
        mut result: OwnedValue,
        mutable: bool,
    ) -> OwnedValue {
        if mutable {
            let (slots, _) = self.local_slots(&result.value.ty);
            for (slot, reg) in slots.iter().zip(&result.value.regs) {
                self.code.push(Inst::StoreLocal(*slot, *reg));
            }
            result.slots = Some(slots);
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
            || !result.value.ty.result;
        let result = self.owned_value_binding(result, mutable);
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
        let mut result = if value.ty.result {
            OwnedValue {
                value,
                payloads: vec![None, None],
                ownership: Ownership::Moved,
                slots: None,
            }
        } else {
            let Returned::OwnedValue(v) = self.template_returned(value, false) else {
                unreachable!()
            };
            v
        };
        let (slots, _) = self.local_slots(&result.value.ty);
        result.slots = Some(slots);
        self.env.insert(name.clone(), Binding::OwnedValue(result));
        Ok(())
    }
    pub(super) fn owned_value_assign(
        &mut self,
        name: &str,
        target: OwnedValue,
        op: &str,
        rhs: &Expr,
        span: Span,
    ) -> Result<()> {
        if op != "=" {
            return self.fail(span, "B224", "拥有结果交接整份载荷，不支持数值复合赋值");
        }
        let result = self.evaluate_returned(rhs, Some(&target.value.ty))?;
        if !self.flow.continues() {
            return Ok(());
        }
        let mut value = result.ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B228",
            message: "赋值需要结果".into(),
        })?;
        self.check_returned_type(&mut value, &target.value.ty, span)?;
        if matches!(self.env.get(name),Some(Binding::OwnedValue(r)) if r.ownership==Ownership::Owned)
        {
            let Some(Binding::OwnedValue(old)) = self.env.get(name).cloned() else {
                unreachable!()
            };
            self.movable_returned(&Returned::OwnedValue(old.clone()), span)?;
            self.drop_returned(Returned::OwnedValue(old), span)?;
        }
        let Returned::OwnedValue(mut value) = value else {
            return self.fail(span, "B224", "需要相同拥有结果类型");
        };
        if self.loop_depth > 0 {
            value.join_payloads(target.payloads);
        }
        let slots = target.slots.unwrap();
        for (slot, reg) in slots.iter().zip(&value.value.regs) {
            self.code.push(Inst::StoreLocal(*slot, *reg));
        }
        value.slots = Some(slots);
        self.initialize(&value.place().unwrap());
        self.env.insert(name.into(), Binding::OwnedValue(value));
        Ok(())
    }
    fn make_owned_result(
        &mut self,
        ty: Ty,
        success: bool,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<OwnedValue>> {
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span, "B224", "拥有结果属于普通CPU应用资源组织");
        }
        let index = usize::from(!success);
        let expected = ty.arguments[index].clone();
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
        let value = self.result_value(ty, success, Some(payload));
        let mut payloads = vec![None, None];
        payloads[index] = Some(Box::new(item));
        Ok(Some(OwnedValue {
            value,
            payloads,
            ownership: Ownership::Owned,
            slots: None,
        }))
    }
    fn owned_condition(&mut self, value: &OwnedValue, span: Span) -> Result<Value> {
        if value.payloads[0].is_none() {
            return Ok(self.constant(Scalar::Bool, 0));
        }
        if value.payloads[1].is_none() {
            return Ok(self.constant(Scalar::Bool, 1));
        }
        self.result_condition(&value.value, span)
    }
    fn unpack_owned(&mut self, value: &OwnedValue, success: bool) -> Returned {
        let index = usize::from(!success);
        let payload = self.result_payload(&value.value, success);
        let Some(meta) = &value.payloads[index] else {
            return match payload {
                Some(value) => self.template_returned(value, false),
                None => Returned::Unit,
            };
        };
        let mut item = (**meta).clone();
        if let Some(value) = payload {
            self.rebind_returned(&mut item, value.regs);
        }
        item
    }
    pub(super) fn evaluate_returned(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Returned>> {
        match &expr.kind {
            ExprKind::Target(target, call) => return self.target_call(*target, call, expr.span),
            ExprKind::Member(base, name) => return self.member_returned(expr, base, name),
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
                    .application_resource(expr, expr.span)
                    .map(|v| v.map(Returned::Dataset));
            }
            ExprKind::Name(name) if matches!(self.env.get(name), Some(Binding::OwnedValue(_))) => {
                return self
                    .take_owned_value(name, expr.span)
                    .map(|v| Some(Returned::OwnedValue(v)));
            }
            ExprKind::Specialize(name, _, _) if name == "dataset" => {
                return self
                    .application_resource(expr, expr.span)
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
                if self
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
            let mut payloads = vec![None, None];
            payloads[usize::from(!success)] = Some(Box::new(item));
            Returned::OwnedValue(OwnedValue {
                value,
                payloads,
                ownership: Ownership::Owned,
                slots: None,
            })
        } else {
            Returned::Value(value)
        }
    }
    pub(super) fn match_returned(
        &mut self,
        expr: &Expr,
        yes_name: &Option<String>,
        yes: &[Stmt],
        no_name: &Option<String>,
        no: &[Stmt],
        span: Span,
    ) -> Result<()> {
        let returned = self.evaluate_returned(expr, None)?;
        if !self.flow.continues() {
            return Ok(());
        }
        let returned = returned.ok_or_else(|| Diagnostic {
            source: self.definition.source.clone(),
            span,
            code: "B269",
            message: "match需要结果".into(),
        })?;
        if let Returned::Value(value) = returned {
            return self.match_value(value, yes_name, yes, no_name, no, span);
        }
        let Returned::OwnedValue(value) = returned else {
            return self.fail(span, "B269", "match需要结果");
        };
        self.check_result_patterns(&value.value.ty, yes_name, no_name, span)?;
        let condition = self.owned_condition(&value, span)?;
        self.conditional_blocks(
            condition,
            |b| b.owned_arm(&value, true, yes_name, yes, span),
            |b| b.owned_arm(&value, false, no_name, no, span),
        )
    }
    fn owned_arm(
        &mut self,
        value: &OwnedValue,
        success: bool,
        name: &Option<String>,
        body: &[Stmt],
        span: Span,
    ) -> Result<()> {
        let item = self.unpack_owned(value, success);
        if let Some(name) = name.as_ref().filter(|n| n.as_str() != "_") {
            self.bind_returned(name, item, span)?;
            self.block(body)?;
            if self.flow.continues() {
                if let Some(binding) = self.env.remove(name) {
                    self.drop_binding(binding, span)?;
                }
            } else {
                self.env.remove(name);
            }
            Ok(())
        } else {
            self.drop_returned(item, span)?;
            self.block(body)
        }
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
    pub(super) fn drop_returned(&mut self, value: Returned, span: Span) -> Result<()> {
        match value {
            Returned::Dataset(data) if data.ownership == Ownership::Owned => {
                self.application_frame(span)?;
                let code = self.application_drop(&data);
                self.code.extend(code);
            }
            Returned::OwnedValue(mut result) if result.ownership == Ownership::Owned => {
                if !result.value.ty.result {
                    for i in 0..result.value.ty.fields.len() {
                        if result.value.ty.fields[i].2.owns_resources() {
                            let field = self.unpack_record(&result, i);
                            self.drop_returned(field, span)?;
                        }
                    }
                    return Ok(());
                }
                if let Some(slots) = &result.slots {
                    result.value.regs = slots
                        .iter()
                        .map(|slot| {
                            let r = self.reg(self.locals[*slot]);
                            self.code.push(Inst::LoadLocal(r, *slot));
                            r
                        })
                        .collect();
                    result.slots = None;
                }
                let condition = self.owned_condition(&result, span)?;
                if let Some(bits) = &condition.constant {
                    let item = self.unpack_owned(&result, bits[0] != 0);
                    self.drop_returned(item, span)?;
                } else {
                    let outer = std::mem::take(&mut self.code);
                    let a = self.unpack_owned(&result, true);
                    self.drop_returned(a, span)?;
                    let yes = std::mem::take(&mut self.code);
                    let b = self.unpack_owned(&result, false);
                    self.drop_returned(b, span)?;
                    let no = std::mem::replace(&mut self.code, outer);
                    self.code.push(Inst::If {
                        condition: condition.regs[0],
                        yes,
                        no,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn drop_binding(&mut self, binding: Binding, span: Span) -> Result<()> {
        match binding {
            Binding::OwnedValue(r) => self.drop_returned(Returned::OwnedValue(r), span),
            Binding::ApplicationData(data) => self.drop_returned(Returned::Dataset(data), span),
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
            cooperative::Argument::OwnedValue(value) => Binding::OwnedValue(value.clone()),
            cooperative::Argument::ApplicationData(data) if access == Access::Value => {
                Binding::ApplicationData(data.clone())
            }
            cooperative::Argument::ScopedData { owner, .. } => return Some(owner.clone()),
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
            if matches!(arguments[*index], cooperative::Argument::ScopedData { .. }) {
                continue;
            }
            let value = self.env.remove(name).unwrap();
            arguments[*index] = match value {
                Binding::ApplicationData(data) => cooperative::Argument::ApplicationData(data),
                Binding::OwnedValue(value) => cooperative::Argument::OwnedValue(value),
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
        Ok(Some(cooperative::Argument::ScopedData {
            data: reference,
            owner: name,
        }))
    }
    pub(super) fn finish_scoped_argument(&mut self, name: &str, span: Span) -> Result<()> {
        let Binding::ApplicationData(data) = self.env.remove(name).unwrap() else {
            unreachable!()
        };
        self.drop_returned(Returned::Dataset(data), span)
    }
}

crate::module::codec::record!(OwnedValue {
    value,
    payloads,
    ownership,
    slots
});
