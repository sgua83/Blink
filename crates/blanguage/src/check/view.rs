//! view 的引用值拥有范围身份；读取共用同一 IR，由各后端解释物理地址。
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Data(ValueIdentity),
    Parameter(ValueIdentity),
    Unresolved,
}
impl Source {
    pub(super) fn bind_outputs(self, bindings: &BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        match self {
            Self::Data(id) => Self::Data(bindings.get(&id).copied().unwrap_or(id)),
            Self::Parameter(id) => Self::Parameter(bindings.get(&id).copied().unwrap_or(id)),
            Self::Unresolved => self,
        }
    }
    pub(super) fn at_call(
        self,
        bindings: &BTreeMap<ValueIdentity, bounds::Proof>,
    ) -> BTreeSet<Self> {
        match self {
            Self::Parameter(id) => bindings
                .get(&id)
                .map(|p| p.view_sources(0))
                .unwrap_or_else(|| BTreeSet::from([Self::Unresolved])),
            Self::Data(id) => {
                let Some(proof) = bindings.get(&id) else {
                    return BTreeSet::from([Self::Unresolved]);
                };
                let sources = proof.view_sources(0);
                if !sources.is_empty() {
                    sources
                } else {
                    BTreeSet::from([proof
                        .known_identity(0)
                        .map(Self::Data)
                        .unwrap_or(Self::Unresolved)])
                }
            }
            Self::Unresolved => BTreeSet::from([self]),
        }
    }
}
crate::module::codec::variants!(Source{0=>Data(id),1=>Parameter(id),2=>Unresolved});
impl Binding {
    pub(super) fn view_sources(&self) -> BTreeSet<Source> {
        match self {
            Self::Value(value) if value.ty.contains_views() => value.proof.borrowed_views(),
            Self::Owned(value) if value.place.ty().contains_views() => value.proof.borrowed_views(),
            Self::ViewElement { view, .. } => view.proof.borrowed_views(),
            _ => BTreeSet::new(),
        }
    }
}
impl Builder<'_> {
    pub(super) fn application_view_borrowed(&self, identity: ValueIdentity) -> bool {
        self.env
            .values()
            .any(|b| b.view_sources().contains(&Source::Data(identity)))
    }
    pub(super) fn require_view_unborrowed(
        &self,
        sources: &[ValueIdentity],
        span: Span,
    ) -> Result<()> {
        if sources.iter().any(|id| self.application_view_borrowed(*id)) {
            return self.fail(
                span,
                "B227",
                "dataset仍被不可变view借用，不能写入、移动或传输",
            );
        }
        Ok(())
    }
    pub(super) fn require_view_available(&self, value: &Value, span: Span) -> Result<()> {
        for source in value.proof.borrowed_views() {
            match source {
                Source::Data(id) => self.application_handle_available(id, span)?,
                Source::Unresolved => {
                    return self.fail(span, "B271", "view返回的实际借用来源尚未形成合同");
                }
                Source::Parameter(_) => {}
            }
        }
        Ok(())
    }
    pub(super) fn require_view_return(&self, value: &Value, span: Span) -> Result<()> {
        self.require_view_available(value, span)?;
        self.require_view_return_proof(&value.proof, span)
    }
    pub(super) fn require_view_return_proof(
        &self,
        proof: &bounds::Proof,
        span: Span,
    ) -> Result<()> {
        for source in proof.borrowed_views() {
            if source == Source::Unresolved {
                return self.fail(span, "B271", "view输出的实际借用来源尚未形成合同");
            }
            if let Source::Data(id) = source {
                let borrowed = self.env.values().any(|binding| matches!(binding,
                    Binding::ApplicationData(data) if data.ownership == application::Ownership::Borrowed && data.handle.identity() == id));
                if !borrowed {
                    return self.fail(
                        span,
                        "B227",
                        "返回view不能借用当前函数拥有并将在退出时释放的dataset",
                    );
                }
            }
        }
        Ok(())
    }
    pub(super) fn create_application_view(
        &mut self,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        if self.application.is_none() || explicit.len() != 1 || args.len() != 1 {
            return self.fail(span, "B271", "CPU应用以view<T>(dataset)借用现有存储");
        }
        let GenericArgument::Type(element) = &explicit[0] else {
            return self.fail(span, "B271", "view<T>需要元素类型");
        };
        let mut name = element.clone();
        name.view = true;
        let ty = self.world.ty(
            &name,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        let Some(data) = self.application_reference(&args[0])? else {
            return self.fail(span, "B271", "view借用需要具有实际所有者的dataset位置");
        };
        self.require_same(ty.view.as_deref().unwrap(), &data.ty, span)?;
        self.require_application_initialized(&data, span)?;
        self.application_cpu(&data, span)?;
        let address =
            self.application_address(data.handle.regs[0], data.handle.identity(), false, span)?;
        let mut proof = bounds::Proof::default();
        let pointer = Value {
            proof: bounds::Proof::default(),
            literal: false,
            ty: Ty::vector(Scalar::U32, 2),
            regs: address.to_vec(),
            uniform: false,
            constant: None,
        };
        proof.append(&pointer);
        proof.append(&data.length);
        proof =
            proof.with_view_sources(0, data.sources.iter().copied().map(Source::Data).collect());
        Ok(Some(Value {
            proof,
            literal: false,
            ty,
            regs: vec![address[0], address[1], data.length.regs[0]],
            uniform: false,
            constant: None,
        }))
    }
}
impl Builder<'_> {
    pub(super) fn view_length(view: &Value) -> Value {
        Value {
            proof: view.projected_proof(&[2]).with_domains(Domains::from([
                IndexDomain::ViewLength(view.view_identity()),
            ])),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![view.regs[2]],
            uniform: view.uniform,
            constant: None,
        }
    }
    pub(super) fn view_element(view: &Value, index: Reg) -> Place {
        let ty = view.ty.view.as_deref().unwrap().clone();
        let stride = ir::align_up(ty.size, ty.align);
        let words = ty.components.iter().map(|(_, offset)| offset / 4).collect();
        Place::ViewElement {
            ty,
            address: [view.regs[0], view.regs[1]],
            index,
            stride,
            words,
        }
    }
    pub(super) fn view_place(&mut self, base: &Expr, index: &Expr) -> Result<Option<Place>> {
        let view = if self.is_place(base) {
            let Some(place) = self.place(base)? else {
                return Ok(None);
            };
            if place.ty().array.is_some() {
                return self.array_place(place, index);
            }
            self.read_place(&place, base.span)?
        } else {
            let Some(value) = self.expr(base)? else {
                return Ok(None);
            };
            value
        };
        if view.ty.array.is_some() {
            let place = self.materialize_array(view);
            return self.array_place(place, index);
        }
        self.view_value_place(view, index)
    }
    pub(super) fn view_value_place(&mut self, view: Value, index: &Expr) -> Result<Option<Place>> {
        if view.ty.view.is_none() {
            return self.fail(index.span, "B136", "索引需要 dataset、tile 或不可变 view");
        }
        let held = self.hold_binding(Binding::Value(view.clone()));
        let evaluated = self.expr(index);
        self.env.remove(&held);
        let Some(value) = evaluated? else {
            return Ok(None);
        };
        self.require_same(&Ty::scalar(Scalar::U32), &value.ty, index.span)?;
        if !self.index_in(
            &value,
            IndexDomain::View(view.view_identity()),
            Self::view_length(&view).minimum(),
        ) {
            return self.fail(
                index.span,
                "B137",
                "view 索引有效性无法静态证明；使用该引用值的长度条件或有界游标",
            );
        }
        Ok(Some(Self::view_element(&view, value.regs[0])))
    }
}
