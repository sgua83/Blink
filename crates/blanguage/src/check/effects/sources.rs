//! 写入预分析只借用原声明和词法来源；实际类型仍由World及原泛型实例产生。
use super::*;
#[derive(Clone)]
pub(super) enum Local {
    Typed(crate::syntax::Type),
    Initialized(Option<crate::syntax::Type>, Expr, Block),
    Range,
    Element(Expr),
    NamedElement(String),
    Payload(Expr, String),
}
// A tile binding supplies an element type, never a movable storage value.
enum ExpressionType {
    Value(Ty),
    Tile(Ty),
}
impl ExpressionType {
    fn value(self) -> Option<Ty> {
        match self {
            Self::Value(ty) => Some(ty),
            Self::Tile(_) => None,
        }
    }
}

pub(super) struct Scope<'b, 'w> {
    pub(super) world: &'b World,
    pub(super) lexical: &'b LexicalScope,
    pub(super) builder: Option<&'b Builder<'w>>,
    pub bindings: Vec<(String, Local)>,
    pub(super) writes: Option<&'b LoopWrites>,
}
impl<'b, 'w> Scope<'b, 'w> {
    pub fn declared(world: &'b World, lexical: &'b LexicalScope) -> Self {
        Self { world, lexical, builder: None, bindings: Vec::new(), writes: None }
    }
    pub fn current(builder: &'b Builder<'w>) -> Self {
        Self { world: builder.world, lexical: &builder.scope, builder: Some(builder), bindings: Vec::new(), writes: None }
    }
    fn declared_type(&self, syntax: &TypeName, context: (&Source,&str,&[String]), span: Span) -> Result<Ty> {
        let (source,module,imports)=context;
        let ty = self.world.ty(syntax, source, span, module, imports)?;
        Ok(if syntax.dataset {
            Ty::owned_dataset((ty).syntax(), self.world.shape(&syntax.count, source, span, module, imports)?)
        } else { ty })
    }
    fn element(&self, ty: ExpressionType, span: Span) -> Result<Option<Ty>> {
        let ty = match ty {
            ExpressionType::Value(ty) => ty,
            ExpressionType::Tile(element) => return Ok(Some(element)),
        };
        if let Some(reference)=ty.dataset {
            return self.world.ty(&reference,self.lexical.source(),span,self.lexical.module(),self.lexical.imports()).map(Some);
        }
        Ok(if let Some(inner)=ty.view.or(ty.array){Some(*inner)}
            else{ty.components.first().map(|(scalar,_)|Ty::scalar(*scalar))})
    }
    fn name(&self, name: &str, span: Span, limit: usize) -> Result<Option<ExpressionType>> {
        if self.lexical.bindings().values.contains_key(name) {
            return Ok(Some(ExpressionType::Value(Ty::scalar(Scalar::U32))));
        }
        let context = (self.lexical.source(), self.lexical.module(), self.lexical.imports());
        if let Some(index) = self.bindings[..limit].iter().rposition(|(bound, _)| *bound == name) {
            let value = match &self.bindings[index].1 {
                Local::Typed(ty) | Local::Initialized(Some(ty), ..) => {
                    let ty = self.lexical.bindings().type_name(self.world, ty, context, span)?;
                    Some(self.declared_type(&ty, context, span)?)
                }
                Local::Initialized(None, expr, _) => return self.ty(expr, index),
                Local::Range => Some(Ty::scalar(Scalar::U32)),
                Local::Element(expr) => match self.ty(expr,index)?{Some(ty)=>self.element(ty,span)?,None=>None},
                Local::NamedElement(name) => match self.name(name,span,index)?{Some(ty)=>self.element(ty,span)?,None=>None},
                Local::Payload(expr, variant) => self.value_type(expr, index)?.and_then(|ty| {
                    let name = variant.rsplit("::").next().unwrap();
                    ty.variants.iter().find(|item| item.name == name).map(|item| *item.ty.clone())
                }),
            };
            return Ok(value.map(ExpressionType::Value));
        }
        if let Some(builder) = self.builder && let Some(binding) = builder.env.get(name) {
            let ty = match binding {
                Binding::Value(value) => Some(value.ty.clone()),
                Binding::ViewElement { element, .. } => Some(element.place.ty().clone()),
                Binding::ApplicationData(data) => Some(Ty::owned_dataset((data.ty.clone()).syntax(), data.count)),
                Binding::DataReference(data) => Some(Ty::owned_dataset((data.element_layout().1.clone()).syntax(), data.count)),
                Binding::Data(view) => Some(Ty::owned_dataset((builder.dataset_layout(view).1.clone()).syntax(), builder.program.buffers[view.binding].count)),
                Binding::Tile(view) => return Ok(Some(ExpressionType::Tile(view.ty.clone()))),
                other => other.stored_place().map(|place| place.ty().clone()),
            };
            return Ok(ty.map(ExpressionType::Value));
        }
        if let Some(param) = self.lexical.parameters().find(|param| param.name() == name) {
            let mut ty = param.bound(self.world)?.ty;
            if ty.tile {
                ty.tile = false;
                let (source, module, imports) = context;
                return self.world.ty(&ty, source, param.span(), module, imports)
                    .map(|element| Some(ExpressionType::Tile(element)));
            }
            return self.declared_type(&ty, context, param.span())
                .map(|ty| Some(ExpressionType::Value(ty)));
        }
        if let Some(key) = self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true).unique() {
            let definition = &self.world.functions[&key];
            if definition.function.kind() == FunctionKind::Constant {
                let context = (&definition.source, definition.module.as_str(), definition.imports.as_ref());
                let span = definition.function.span();
                let bindings = Arc::new(crate::generics::Bindings::default());
                let ty = bindings.type_name(self.world, &definition.function.result(), context, span)?;
                return self.declared_type(&ty, context, span)
                    .map(|ty| Some(ExpressionType::Value(ty)));
            }
        }
        Ok(None)
    }
    fn bound(&self, name: &str, limit: usize) -> bool {
        self.lexical.bindings().values.contains_key(name)
            || self.bindings[..limit].iter().any(|(bound, _)| *bound == name)
            || self.builder.is_some_and(|builder| builder.env.contains_key(name))
            || self.lexical.parameters().any(|param| param.name() == name)
    }
    pub fn local_value_write(&self,place:&Expr)->Result<Option<bool>> {
        let Some(name)=assigned_root(place)else{return Ok(None);};
        let Some((_,local))=self.bindings.iter().rev().find(|(bound,_)|*bound==name)else{return Ok(None);};
        let Some(ty)=self.named(&name,place.span())?else{return Ok(Some(false));};
        Ok(Some(matches!(local,Local::Typed(_)|Local::Initialized(..)|Local::Range|Local::Payload(..))
            &&!ty.owns_resources()&&self.builder.unwrap().component_projection(place,&name,&ty,None,None,&|_|Ok(None))?.is_some()))
    }
    pub fn named(&self, name: &str, span: Span) -> Result<Option<Ty>> {
        let limit = self.bindings.len();
        if !self.bound(name, limit) {
            return Ok(None);
        }
        self.name(name, span, limit).map(|ty| ty.and_then(ExpressionType::value))
    }
    fn returned(&self, ty: Ty, span: Span) -> Result<Option<Ty>> {
        let Some(function) = ty.function else { return Ok(None); };
        if matches!(function.result.name.as_str(), "void" | "never") { return Ok(None); }
        self.declared_type(&function.result,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some)
    }
    fn call(&self, name: &str, explicit: Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>, args: &Expressions, span: Span, limit: usize) -> Result<Option<Ty>> {
        if explicit.is_none()&&let Some(target)=self.lexical.bindings().type_call(name){
            return match target{
                crate::generics::TypeCall::Builtin(name)=>self.declared_type(&TypeName::named(name),(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some),
                crate::generics::TypeCall::Construct(ty)=>self.declared_type(ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some),
            };
        }
        let bound_explicit=explicit.map(|arguments|self.lexical.bindings().arguments(self.world,arguments,
            (self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).transpose()?;
        let explicit=bound_explicit.as_deref();
        if name=="dataset" && let Some(explicit)=explicit {
            return Builder::dataset_constructor_type(self.world,self.lexical,explicit,args.len(),span).map(|(ty,count)|Some(Ty::owned_dataset((ty).syntax(),count)));
        }
        if name == "view" && let Some([GenericArgument::Type(element)]) = explicit {
            let mut syntax = element.clone();
            syntax.view = true;
            return self.declared_type(&syntax,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some);
        }
        if explicit.is_none() && self.bound(name, limit) && let Some(ty) = self.name(name, span, limit)? {
            return match ty {
                ExpressionType::Value(ty) => self.returned(ty, span),
                ExpressionType::Tile(_) => Ok(None),
            };
        }
        if name == "address_of" && explicit.is_none() { return Ok(Ty::builtin("address")); }
        if name == "c_function" && explicit.is_none() {
            return callback::signature(self.world, self.lexical, args, span).map(|(_, ty)| Some(ty));
        }
        if matches!(name, "unsafe_function" | "unsafe_position")
            && let Some([GenericArgument::Type(ty)]) = explicit {
            return self.declared_type(ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some);
        }
        if explicit.is_some() && matches!(name, "size_of" | "align_of")
            && self.world.resolve(name, self.lexical.module(), self.lexical.imports(), false).is_missing()
            && self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true).is_missing() {
            return Ok(Some(Ty::scalar(Scalar::U64)));
        }
        if name == "try" && args.len()==1 {
            let value=&args.at(0);
            return Ok(self.value_type(value, limit)?.and_then(|ty| ty.variants.first().map(|variant| *variant.ty.clone())));
        }
        if Ty::builtin(name).is_some() || matches!(name, "array" | "cfn")
            || !self.world.resolve(name, self.lexical.module(), self.lexical.imports(), false).is_missing() {
            let mut syntax = TypeName::named(name);
            syntax.arguments = explicit.unwrap_or_default().to_vec();
            return self.declared_type(&syntax,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span).map(Some);
        }
        match self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true) {
            crate::frontend::NameResolution::Unique(key) => {
                let types = args.iter().map(|arg| self.ty(&arg, limit)).collect::<Result<Vec<_>>>()?;
                let actuals = types.iter().map(|ty| match ty {
                    Some(ExpressionType::Tile(element)) => generics::Actual::value(element),
                    Some(ExpressionType::Value(ty)) => generics::Actual {
                        ty: Some(std::borrow::Cow::Borrowed(ty)),
                        count: ty.dataset.as_ref().and(ty.values.first().copied()),
                    },
                    None => generics::Actual { ty: None, count: None },
                }).collect::<Vec<_>>();
                let instance = generics::specialize(self.world, &key, &actuals, explicit, (self.lexical.source(), self.lexical.module(), self.lexical.imports()), span)?;
                let definition=&instance.definition;
                let context=(&definition.source,definition.module.as_str(),definition.imports.as_ref());
                let result=instance.bindings.type_name(self.world,&definition.function.result(),context,span)?;
                if result.name=="void"||result.is_never(){return Ok(None);}
                return self.declared_type(&result,context,span).map(Some);
            }
            crate::frontend::NameResolution::Ambiguous => return Ok(None),
            crate::frontend::NameResolution::Missing => {}
        }
        if explicit.is_none() && name=="own" && args.len()==2 {
            let value=&args.at(0);
            let function=&args.at(1);
            let function=Builder::user_owned_function(self.lexical.source(),function)?;
            return self.value_type(value,limit)?.map(|ty|Builder::user_owned_type(self.world,self.lexical,&ty,function,span)).transpose();
        }
        if explicit.is_none() && ((name=="resize"&&args.len()==3)||(name=="take_last"&&args.len()==2)) {
            let data=&args.at(0);
            return Ok(self.value_type(data,limit)?.and_then(|ty|ty.dataset.map(|element|Ty::owned_dataset(*element,None))));
        }
        Ok(None)
    }
    fn ty(&self, expr: &Expr, limit: usize) -> Result<Option<ExpressionType>> {
        let value = match expr.kind() {
            ExprKind::Name(name) => return self.name(name, expr.span(), limit),
            ExprKind::Number(text) => Some(number(self.lexical.source(), expr.span(), text)?.0),
            ExprKind::StaticU32(_) => Some(Ty::scalar(Scalar::U32)),
            ExprKind::Bool(_) => Some(Ty::scalar(Scalar::Bool)),
            ExprKind::Member(base, name) => match self.ty(&base, limit)? {
                Some(ExpressionType::Tile(_)) if matches!(name, "length" | "lane") => Some(Ty::scalar(Scalar::U32)),
                Some(ExpressionType::Value(ty)) => ty.field(name).map(|(_, ty)| ty.clone()),
                _ => None,
            },
            ExprKind::Index(base, _) => match self.ty(&base,limit)?{Some(ty)=>self.element(ty,expr.span())?,None=>None},
            ExprKind::Call(name, args) => self.call(name, None, &args, expr.span(), limit)?,
            ExprKind::Specialize(name, explicit, args) => self.call(name, Some(&explicit), &args, expr.span(), limit)?,
            ExprKind::Cooperate(name, explicit, args) => self.call(name, explicit.as_ref(), &args, expr.span(), limit)?,
            ExprKind::Invoke(callee, _) => match self.value_type(&callee, limit)? {
                Some(ty) => self.returned(ty, expr.span())?, None => None,
            },
            ExprKind::Construct(ty, _) => Some(self.declared_type(&self.lexical.bindings().type_name(self.world,&ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),expr.span())?,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),expr.span())?),
            ExprKind::Target(_, base) | ExprKind::Unary(_, base) => self.value_type(&base, limit)?,
            ExprKind::Binary(op, a, b) => if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
                Some(Ty::scalar(Scalar::Bool))
            } else { self.value_type(if matches!(a.kind(), ExprKind::Number(_)) { &b } else { &a }, limit)? },
            ExprKind::Select(_, yes, _) => self.value_type(&yes, limit)?,
            ExprKind::FunctionRef(..) => None,
        };
        Ok(value.map(ExpressionType::Value))
    }
    fn value_type(&self, expr: &Expr, limit: usize) -> Result<Option<Ty>> {
        self.ty(expr, limit).map(|ty| ty.and_then(ExpressionType::value))
    }
    pub fn expression(&self, expr: &Expr) -> Result<Option<Ty>> {
        self.value_type(expr, self.bindings.len())
    }
}
