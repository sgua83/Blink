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
    Texture(Option<u32>),
}
impl ExpressionType {
    fn value(self) -> Option<Ty> {
        match self {
            Self::Value(ty) => Some(ty),
            Self::Tile(_) | Self::Texture(_) => None,
        }
    }
}

pub(super) struct Scope<'b, 'w> {
    pub(super) world: &'b World,
    pub(super) lexical: &'b LexicalScope,
    pub(super) builder: Option<&'b Builder<'w>>,
    pub(super) binding_uses: Option<&'b std::cell::RefCell<BindingUses>>,
    pub bindings: Vec<(String, Local)>,
    pub(super) writes: Option<&'b LoopWrites>,
}
impl<'b, 'w> Scope<'b, 'w> {
    pub fn declared(world: &'b World, lexical: &'b LexicalScope, uses: &'b std::cell::RefCell<BindingUses>) -> Self {
        Self { world, lexical, builder: None, binding_uses: Some(uses), bindings: Vec::new(), writes: None }
    }
    pub fn current(builder: &'b Builder<'w>) -> Self {
        Self { world: builder.world, lexical: &builder.scope, builder: Some(builder), binding_uses: None, bindings: Vec::new(), writes: None }
    }
    async fn declared_type(&self, syntax: &TypeName, context: (&Source,&str,&[String]), span: Span) -> Result<Ty> {
        let (source,module,imports)=context;
        let ty = unsafe { self.world.call(self.world.ty(syntax, source, span, module, imports)).await }?;
        Ok(if syntax.dataset {
            Ty::owned_dataset((ty).syntax(), unsafe { self.world.call(self.world.shape(&syntax.count, source, span, module, imports)).await }?)
        } else { ty })
    }
    async fn element(&self, ty: ExpressionType, span: Span) -> Result<Option<Ty>> {
        let ty = match ty {
            ExpressionType::Value(ty) => ty,
            ExpressionType::Tile(element) => return Ok(Some(element)),
            ExpressionType::Texture(_) => return Ok(None),
        };
        if let Some(reference)=ty.dataset {
            return unsafe { self.world.call(self.world.ty(&reference,self.lexical.source(),span,self.lexical.module(),self.lexical.imports())).await }.map(Some);
        }
        if let Some(mut element) = ty.view {
            element.complete_value_layout();
            Ok(Some(*element))
        } else if let Some(element) = ty.array {
            Ok(Some(*element))
        } else {
            Ok(ty.components.first().map(|(scalar, _)| Ty::scalar(*scalar)))
        }
    }
    async fn name(&self, name: &str, span: Span, limit: usize) -> Result<Option<ExpressionType>> {
        if self.lexical.bindings().value(name).is_some() {
            return Ok(Some(ExpressionType::Value(Ty::scalar(Scalar::U32))));
        }
        let context = (self.lexical.source(), self.lexical.module(), self.lexical.imports());
        if let Some(index) = self.bindings[..limit].iter().rposition(|(bound, _)| *bound == name) {
            let value = match &self.bindings[index].1 {
                Local::Typed(ty) | Local::Initialized(Some(ty), ..) => {
                    let ty = unsafe { self.world.call(self.lexical.bindings().type_name(self.world, ty, context, span)).await }?;
                    Some(unsafe { self.world.call(self.declared_type(&ty, context, span)).await }?)
                }
                Local::Initialized(None, expr, _) => return unsafe { self.world.call(self.ty(expr, index)).await },
                Local::Range => Some(Ty::scalar(Scalar::U32)),
                Local::Element(expr) => match unsafe { self.world.call(self.ty(expr,index)).await }?{Some(ty)=>unsafe { self.world.call(self.element(ty,span)).await }?,None=>None},
                Local::NamedElement(name) => match unsafe { self.world.call(self.name(name,span,index)).await }?{Some(ty)=>unsafe { self.world.call(self.element(ty,span)).await }?,None=>None},
                Local::Payload(expr, variant) => unsafe { self.world.call(self.value_type(expr, index)).await }?.and_then(|ty| {
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
                Binding::Texture(resource) => return Ok(Some(ExpressionType::Texture(resource.shape(builder.program)))),
                other => other.stored_place().map(|place| place.ty().clone()),
            };
            return Ok(ty.map(ExpressionType::Value));
        }
        if let Some(param) = self.lexical.parameters().find(|param| param.name() == name) {
            let mut ty = unsafe { self.world.call(param.bound(self.world)).await }?.ty;
            if let Some(texture) = ty.texture_type() {
                return unsafe { self.world.call(texture::shape(self.world, texture, context, param.span())).await }
                    .map(|count| Some(ExpressionType::Texture(count)));
            }
            if ty.is_acceleration() {
                return Ok(None);
            }
            if ty.tile {
                ty.tile = false;
                let (source, module, imports) = context;
                return unsafe { self.world.call(self.world.ty(&ty, source, param.span(), module, imports)).await }
                    .map(|element| Some(ExpressionType::Tile(element)));
            }
            return unsafe { self.world.call(self.declared_type(&ty, context, param.span())).await }
                .map(|ty| Some(ExpressionType::Value(ty)));
        }
        if let Some(key) = self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true).unique() {
            let definition = &self.world.functions[&key];
            if definition.function.kind() == FunctionKind::Constant {
                let context = (&definition.source, definition.module.as_str(), definition.imports.as_ref());
                let span = definition.function.span();
                let bindings = Arc::new(crate::generics::Bindings::default());
                let ty = unsafe { self.world.call(bindings.type_name(self.world, &definition.function.result(), context, span)).await }?;
                return unsafe { self.world.call(self.declared_type(&ty, context, span)).await }
                    .map(|ty| Some(ExpressionType::Value(ty)));
            }
        }
        Ok(None)
    }
    fn bound(&self, name: &str, limit: usize) -> bool {
        self.lexical.bindings().value(name).is_some()
            || self.bindings[..limit].iter().any(|(bound, _)| *bound == name)
            || self.builder.is_some_and(|builder| builder.env.contains_key(name))
            || self.lexical.parameters().any(|param| param.name() == name)
    }
    pub async fn local_value_write(&self,place:&Expr)->Result<Option<bool>> {
        let Some(name)=assigned_root(place)else{return Ok(None);};
        let Some((_,local))=self.bindings.iter().rev().find(|(bound,_)|*bound==name)else{return Ok(None);};
        let Some(ty)=unsafe { self.world.call(self.named(&name,place.span())).await }?else{return Ok(Some(false));};
        Ok(Some(matches!(local,Local::Typed(_)|Local::Initialized(..)|Local::Range|Local::Payload(..))
            &&!ty.owns_resources()&&unsafe { self.world.call(self.builder.unwrap().component_projection(place,&name,&ty,None,None,&|_|Ok(None))).await }?.is_some()))
    }
    pub async fn named(&self, name: &str, span: Span) -> Result<Option<Ty>> {
        let limit = self.bindings.len();
        if !self.bound(name, limit) {
            return Ok(None);
        }
        unsafe { self.world.call(self.name(name, span, limit)).await }.map(|ty| ty.and_then(ExpressionType::value))
    }
    async fn returned(&self, ty: Ty, span: Span) -> Result<Option<Ty>> {
        let Some(function) = ty.function else { return Ok(None); };
        if matches!(function.result.name.as_str(), "void" | "never") { return Ok(None); }
        unsafe { self.world.call(self.declared_type(&function.result,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some)
    }
    async fn call(&self, name: &str, explicit: Option<&crate::syntax::List<crate::syntax::wire::ArgumentNode>>, args: &Expressions, span: Span, limit: usize) -> Result<Option<Ty>> {
        if explicit.is_none()&&let Some(target)=self.lexical.bindings().type_call(name){
            return match target{
                crate::generics::TypeCall::Builtin(name)=>unsafe { self.world.call(self.declared_type(&TypeName::named(name),(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some),
                crate::generics::TypeCall::Construct(ty)=>unsafe { self.world.call(self.declared_type(ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some),
            };
        }
        let bound_explicit=match explicit {
            Some(arguments)=>Some(unsafe { self.world.call(self.lexical.bindings().arguments(self.world,arguments,
                (self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }?),
            None=>None,
        };
        let explicit=bound_explicit.as_deref();
        if name=="dataset" && let Some(explicit)=explicit {
            return unsafe { self.world.call(Builder::dataset_constructor_type(self.world,self.lexical,explicit,args.len(),span)).await }.map(|(ty,count)|Some(Ty::owned_dataset((ty).syntax(),count)));
        }
        if name == "view" && let Some([GenericArgument::Type(element)]) = explicit {
            let mut syntax = element.clone();
            syntax.view = true;
            return unsafe { self.world.call(self.declared_type(&syntax,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some);
        }
        if explicit.is_none() && self.bound(name, limit) && let Some(ty) = unsafe { self.world.call(self.name(name, span, limit)).await }? {
            return match ty {
                ExpressionType::Value(ty) => unsafe { self.world.call(self.returned(ty, span)).await },
                ExpressionType::Tile(_) | ExpressionType::Texture(_) => Ok(None),
            };
        }
        if name == "address_of" && explicit.is_none() { return Ok(Ty::builtin("address")); }
        if name == "c_function" && explicit.is_none() {
            return unsafe { self.world.call(callback::signature(self.world, self.lexical, args, span)).await }.map(|(_, ty)| Some(ty));
        }
        if matches!(name, "unsafe_function" | "unsafe_position")
            && let Some([GenericArgument::Type(ty)]) = explicit {
            return unsafe { self.world.call(self.declared_type(ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some);
        }
        if explicit.is_some() && matches!(name, "size_of" | "align_of")
            && self.world.resolve(name, self.lexical.module(), self.lexical.imports(), false).is_missing()
            && self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true).is_missing() {
            return Ok(Some(Ty::scalar(Scalar::U64)));
        }
        if name == "try" && args.len()==1 {
            let value=&args.at(0);
            return Ok(unsafe { self.world.call(self.value_type(value, limit)).await }?.and_then(|ty| ty.variants.first().map(|variant| *variant.ty.clone())));
        }
        if Ty::builtin(name).is_some() || matches!(name, "array" | "cfn")
            || !self.world.resolve(name, self.lexical.module(), self.lexical.imports(), false).is_missing() {
            let mut syntax = TypeName::named(name);
            syntax.arguments = explicit.unwrap_or_default().to_vec();
            return unsafe { self.world.call(self.declared_type(&syntax,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),span)).await }.map(Some);
        }
        match self.world.resolve(name, self.lexical.module(), self.lexical.imports(), true) {
            crate::frontend::NameResolution::Unique(key) => {
                let actuals = unsafe { self.world.call(self.call_actuals(args, limit)).await }?;
                let instance = unsafe { self.world.call(generics::specialize(self.world, &key, &actuals, explicit, (self.lexical.source(), self.lexical.module(), self.lexical.imports()), span)).await }?;
                let definition=&instance.definition;
                let context=(&definition.source,definition.module.as_str(),definition.imports.as_ref());
                let result=unsafe { self.world.call(instance.bindings.type_name(self.world,&definition.function.result(),context,span)).await }?;
                if result.name=="void"||result.is_never(){return Ok(None);}
                return unsafe { self.world.call(self.declared_type(&result,context,span)).await }.map(Some);
            }
            crate::frontend::NameResolution::Ambiguous => return Ok(None),
            crate::frontend::NameResolution::Missing => {}
        }
        if explicit.is_none() && name=="own" && args.len()==2 {
            let value=&args.at(0);
            let function=&args.at(1);
            let function=Builder::user_owned_function(self.lexical.source(),function)?;
            return match unsafe { self.world.call(self.value_type(value,limit)).await }? {
                Some(ty)=>unsafe { self.world.call(Builder::user_owned_type(self.world,self.lexical,&ty,function,span)).await }.map(Some),
                None=>Ok(None),
            };
        }
        if explicit.is_none() && ((name=="resize"&&args.len()==3)||(name=="take_last"&&args.len()==2)) {
            let data=&args.at(0);
            return Ok(unsafe { self.world.call(self.value_type(data,limit)).await }?.and_then(|ty|ty.dataset.map(|element|Ty::owned_dataset(*element,None))));
        }
        Ok(None)
    }
    async fn ty(&self, expr: &Expr, limit: usize) -> Result<Option<ExpressionType>> {
        let value = match expr.kind() {
            ExprKind::Name(name) => return unsafe { self.world.call(self.name(name, expr.span(), limit)).await },
            ExprKind::Number(text) => Some(number(self.lexical.source(), expr.span(), text)?.0),
            ExprKind::StaticU32(_) => Some(Ty::scalar(Scalar::U32)),
            ExprKind::Bool(_) => Some(Ty::scalar(Scalar::Bool)),
            ExprKind::Member(base, name) => match unsafe { self.world.call(self.ty(&base, limit)).await }? {
                Some(ExpressionType::Tile(_)) if matches!(name, "length" | "lane") => Some(Ty::scalar(Scalar::U32)),
                Some(ExpressionType::Texture(Some(_))) if name == "length" => Some(Ty::scalar(Scalar::U32)),
                Some(ExpressionType::Value(ty)) => ty.field(name).map(|(_, ty)| ty.clone()),
                _ => None,
            },
            ExprKind::Index(base, _) => match unsafe { self.world.call(self.ty(&base,limit)).await }?{Some(ty)=>unsafe { self.world.call(self.element(ty,expr.span())).await }?,None=>None},
            ExprKind::Call(name, args) => unsafe { self.world.call(self.call(name, None, &args, expr.span(), limit)).await }?,
            ExprKind::Specialize(name, explicit, args) => unsafe { self.world.call(self.call(name, Some(&explicit), &args, expr.span(), limit)).await }?,
            ExprKind::Cooperate(name, explicit, args) => unsafe { self.world.call(self.call(name, explicit.as_ref(), &args, expr.span(), limit)).await }?,
            ExprKind::Invoke(callee, _) => match unsafe { self.world.call(self.value_type(&callee, limit)).await }? {
                Some(ty) => unsafe { self.world.call(self.returned(ty, expr.span())).await }?, None => None,
            },
            ExprKind::Construct(ty, _) => Some(unsafe { self.world.call(self.declared_type(&unsafe { self.world.call(self.lexical.bindings().type_name(self.world,&ty,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),expr.span())).await }?,(self.lexical.source(),self.lexical.module(),self.lexical.imports()),expr.span())).await }?),
            ExprKind::Target(_, base) | ExprKind::Unary(_, base) => unsafe { self.world.call(self.value_type(&base, limit)).await }?,
            ExprKind::Binary(op, a, b) => if matches!(op, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||") {
                Some(Ty::scalar(Scalar::Bool))
            } else { unsafe { self.world.call(self.value_type(if matches!(a.kind(), ExprKind::Number(_)) { &b } else { &a }, limit)).await }? },
            ExprKind::Select(_, yes, _) => unsafe { self.world.call(self.value_type(&yes, limit)).await }?,
            ExprKind::FunctionRef(..) => None,
        };
        Ok(value.map(ExpressionType::Value))
    }
    pub(super) async fn call_actuals(&self, args: &Expressions, limit: usize) -> Result<Vec<generics::Actual<'static>>> {
        let mut actuals=Vec::new();
        for arg in args {
            let (ty, count) = match unsafe { self.world.call(self.ty(&arg, limit)).await }? {
                Some(ExpressionType::Value(ty)) => {
                    let count = ty.dataset.as_ref().and(ty.values.first().copied());
                    (Some(ty), count)
                }
                Some(ExpressionType::Tile(element)) => (Some(element), None),
                Some(ExpressionType::Texture(count)) => (None, count),
                None => (None, None),
            };
            actuals.push(generics::Actual { ty: ty.map(std::borrow::Cow::Owned), count });
        }
        Ok(actuals)
    }
    async fn value_type(&self, expr: &Expr, limit: usize) -> Result<Option<Ty>> {
        unsafe { self.world.call(self.ty(expr, limit)).await }.map(|ty| ty.and_then(ExpressionType::value))
    }
    pub async fn expression(&self, expr: &Expr) -> Result<Option<Ty>> {
        unsafe { self.world.call(self.value_type(expr, self.bindings.len())).await }
    }
}
