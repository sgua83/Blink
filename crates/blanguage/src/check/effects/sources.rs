//! 写入预分析只借用原声明和词法来源；实际类型仍由World及原泛型实例产生。
use super::*;
pub(super) enum Local<'s> {
    Typed(&'s TypeName),
    Inferred(&'s Expr),
    Range,
    Element(&'s Expr),
    NamedElement(&'s str),
    Payload(&'s Expr, &'s str),
}
pub(super) struct Scope<'b, 'w, 's> {
    pub(super) world: &'b World,
    pub(super) definition: &'b Definition,
    builder: Option<&'b Builder<'w>>,
    pub bindings: Vec<(&'s str, Local<'s>)>,
}
impl<'b, 'w, 's> Scope<'b, 'w, 's> {
    pub fn declared(world: &'b World, definition: &'b Definition) -> Self {
        Self { world, definition, builder: None, bindings: Vec::new() }
    }
    pub fn current(builder: &'b Builder<'w>) -> Self {
        Self { world: builder.world, definition: &builder.definition, builder: Some(builder), bindings: Vec::new() }
    }
    fn declared_type(&self, syntax: &TypeName, definition: &Definition, span: Span) -> Result<Ty> {
        let ty = self.world.ty(syntax, &definition.source, span, &definition.module, &definition.imports)?;
        Ok(if syntax.dataset {
            Ty::owned_dataset(ty, self.world.shape(&syntax.count, &definition.source, span, &definition.module, &definition.imports)?)
        } else { ty })
    }
    fn element(ty: Ty) -> Option<Ty> {
        if let Some(inner) = ty.dataset.or(ty.view).or(ty.array) { Some(*inner) }
        else { ty.components.first().map(|(scalar, _)| Ty::scalar(*scalar)) }
    }
    fn name(&self, name: &str, span: Span, limit: usize) -> Result<Option<Ty>> {
        if let Some(index) = self.bindings[..limit].iter().rposition(|(bound, _)| *bound == name) {
            return Ok(match &self.bindings[index].1 {
                Local::Typed(ty) => Some(self.declared_type(ty, self.definition, span)?),
                Local::Inferred(expr) => self.ty(expr, index)?,
                Local::Range => Some(Ty::scalar(Scalar::U32)),
                Local::Element(expr) => self.ty(expr, index)?.and_then(Self::element),
                Local::NamedElement(name) => self.name(name, span, index)?.and_then(Self::element),
                Local::Payload(expr, variant) => self.ty(expr, index)?.and_then(|ty| {
                    let name = variant.rsplit("::").next().unwrap();
                    ty.variants.iter().find(|item| item.name == name).map(|item| *item.ty.clone())
                }),
            });
        }
        if let Some(builder) = self.builder && let Some(binding) = builder.env.get(name) {
            let ty = match binding {
                Binding::Value(value) => Some(value.ty.clone()),
                Binding::ViewElement { element, .. } => Some(element.place.ty().clone()),
                Binding::ApplicationData(data) => Some(Ty::owned_dataset(data.ty.clone(), data.count)),
                Binding::DataReference(data) => Some(Ty::owned_dataset(data.element_layout().1.clone(), data.count)),
                Binding::Data(view) => Some(Ty::owned_dataset(builder.dataset_layout(view).1.clone(), builder.program.buffers[view.binding].count)),
                Binding::Tile(view) => Some(Ty::owned_dataset(view.ty.clone(), match builder.tiles[view.tile].width { TileWidth::Fixed(count) => Some(count), TileWidth::Parameter => None })),
                other => other.stored_place().map(|place| place.ty().clone()),
            };
            return Ok(ty);
        }
        if let Some(param) = self.definition.function.params.iter().find(|param| param.name == name) {
            return self.declared_type(&param.ty, self.definition, param.span).map(Some);
        }
        if let Some(key) = self.world.resolve(name, &self.definition.module, &self.definition.imports, true)
            && self.world.functions[&key].function.kind == FunctionKind::Constant {
            let definition = &self.world.functions[&key];
            return self.declared_type(&definition.function.result, definition, span).map(Some);
        }
        Ok(None)
    }
    fn bound(&self, name: &str, limit: usize) -> bool {
        self.bindings[..limit].iter().any(|(bound, _)| *bound == name)
            || self.builder.is_some_and(|builder| builder.env.contains_key(name))
            || self.definition.function.params.iter().any(|param| param.name == name)
    }
    pub fn named(&self, name: &str, span: Span) -> Result<Option<Ty>> {
        if self.bound(name, self.bindings.len()) { self.name(name, span, self.bindings.len()) } else { Ok(None) }
    }
    fn returned(&self, ty: Ty, span: Span) -> Result<Option<Ty>> {
        let Some(function) = ty.function else { return Ok(None); };
        if matches!(function.result.name.as_str(), "void" | "never") { return Ok(None); }
        self.declared_type(&function.result, self.definition, span).map(Some)
    }
    fn call(&self, name: &str, explicit: Option<&[GenericArgument]>, args: &[Expr], span: Span, limit: usize) -> Result<Option<Ty>> {
        if explicit.is_none() && self.bound(name, limit) && let Some(ty) = self.name(name, span, limit)? { return self.returned(ty, span); }
        if name == "address_of" && explicit.is_none() { return Ok(Ty::builtin("address")); }
        if name == "c_function" && explicit.is_none() {
            return callback::signature(self.world, self.definition, args, span).map(|(_, ty)| Some(ty));
        }
        if matches!(name, "unsafe_function" | "unsafe_position")
            && let Some([GenericArgument::Type(ty)]) = explicit {
            return self.declared_type(ty, self.definition, span).map(Some);
        }
        if explicit.is_some() && matches!(name, "size_of" | "align_of")
            && self.world.resolve(name, &self.definition.module, &self.definition.imports, false).is_none()
            && self.world.resolve(name, &self.definition.module, &self.definition.imports, true).is_none() {
            return Ok(Some(Ty::scalar(Scalar::U64)));
        }
        if name == "try" && let [value] = args {
            return Ok(self.ty(value, limit)?.and_then(|ty| ty.variants.first().map(|variant| *variant.ty.clone())));
        }
        if Ty::builtin(name).is_some() || name == "cfn"
            || self.world.resolve(name, &self.definition.module, &self.definition.imports, false).is_some() {
            let mut syntax = TypeName::named(name);
            syntax.arguments = explicit.unwrap_or_default().to_vec();
            return self.declared_type(&syntax, self.definition, span).map(Some);
        }
        let Some(key) = self.world.resolve(name, &self.definition.module, &self.definition.imports, true) else { return Ok(None); };
        let types = args.iter().map(|arg| self.ty(arg, limit)).collect::<Result<Vec<_>>>()?;
        let actuals = types.iter().map(|ty| generics::Actual {
            ty: ty.as_ref().map(std::borrow::Cow::Borrowed),
            count: ty.as_ref().and_then(|ty| ty.dataset.as_ref().and(ty.values.first().copied())),
        }).collect::<Vec<_>>();
        let instance = generics::specialize(self.world, &key, &actuals, explicit, self.definition, span)?;
        if matches!(instance.definition.function.result.name.as_str(), "void" | "never") { return Ok(None); }
        self.declared_type(&instance.definition.function.result, &instance.definition, span).map(Some)
    }
    fn ty(&self, expr: &Expr, limit: usize) -> Result<Option<Ty>> {
        Ok(match &expr.kind {
            ExprKind::Name(name) => self.name(name, expr.span, limit)?,
            ExprKind::Number(text) => Some(number(&self.definition.source, expr.span, text)?.0),
            ExprKind::StaticU32(_) => Some(Ty::scalar(Scalar::U32)),
            ExprKind::Bool(_) => Some(Ty::scalar(Scalar::Bool)),
            ExprKind::Member(base, name) => self.ty(base, limit)?.and_then(|ty| ty.field(name).map(|(_, ty)| ty.clone())),
            ExprKind::Index(base, _) => self.ty(base, limit)?.and_then(Self::element),
            ExprKind::Call(name, args) => self.call(name, None, args, expr.span, limit)?,
            ExprKind::Specialize(name, explicit, args) => self.call(name, Some(explicit), args, expr.span, limit)?,
            ExprKind::Cooperate(name, explicit, args) => self.call(name, explicit.as_deref(), args, expr.span, limit)?,
            ExprKind::Invoke(callee, _) => match self.ty(callee, limit)? {
                Some(ty) => self.returned(ty, expr.span)?, None => None,
            },
            ExprKind::Construct(ty, _) => Some(self.declared_type(ty, self.definition, expr.span)?),
            ExprKind::Target(_, base) | ExprKind::Unary(_, base) => self.ty(base, limit)?,
            ExprKind::Binary(op, a, b) => if matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
                Some(Ty::scalar(Scalar::Bool))
            } else { self.ty(if matches!(a.kind, ExprKind::Number(_)) { b } else { a }, limit)? },
            ExprKind::Select(_, yes, _) => self.ty(yes, limit)?,
            ExprKind::FunctionRef(..) => None,
        })
    }
    pub fn expression(&self, expr: &Expr) -> Result<Option<Ty>> { self.ty(expr, self.bindings.len()) }
}
