//! 明确C ABI定义的原生符号直接交给调用者；不创建回调宿主或Frame。
use super::*;

fn parameter_name(name: &str, parameters: &crate::syntax::List<crate::syntax::wire::ParamNode>) -> String {
    let (base, tail) = name.split_once('.').map_or((name, ""), |(a, b)| (a, b));
    let Some(index) = parameters.iter().position(|parameter| parameter.name() == base) else { return name.into(); };
    if tail.is_empty() { format!("argument{index}") } else { format!("argument{index}.{tail}") }
}
fn condition(value: &OutputCondition, parameters: &crate::syntax::List<crate::syntax::wire::ParamNode>) -> OutputCondition {
    match value {
        OutputCondition::Name(name) => OutputCondition::Name(parameter_name(name, parameters)),
        OutputCondition::Member(base, field) => OutputCondition::Member(Box::new(condition(base, parameters)), field.clone()),
        OutputCondition::Unary(op, value) => OutputCondition::Unary(op.clone(), Box::new(condition(value, parameters))),
        OutputCondition::Binary(op, a, b) => OutputCondition::Binary(op.clone(), Box::new(condition(a, parameters)), Box::new(condition(b, parameters))),
        value => value.clone(),
    }
}
pub(super) fn signature(world: &World, caller: &LexicalScope, args: &Expressions, span: Span) -> Result<(crate::generics::Instance, Ty)> {
    if args.len()!=1 {
        return caller.source().fail(span, "B153", "c_function需要一个明确的C ABI函数引用");
    }
    let reference=&args.at(0);
    let (name, explicit) = match reference.kind() {
        ExprKind::Name(name) => (name, None),
        ExprKind::FunctionRef(name, arguments) => (name, Some(arguments)),
        _ => return caller.source().fail(reference.span(), "B153", "c_function需要C ABI函数名及具体类型／const实参"),
    };
    signature_named(world,caller,name,span,reference.span(),||{
        explicit.as_ref().map(|arguments|caller.bindings().arguments(world,arguments,
            (caller.source(),caller.module(),caller.imports()),reference.span()).map(std::borrow::Cow::Owned)).transpose()
    })
}
pub(super) fn signature_named<'a>(world:&World,caller:&LexicalScope,name:&str,span:Span,reference_span:Span,
    arguments:impl FnOnce()->Result<Option<std::borrow::Cow<'a,[GenericArgument]>>>)->Result<(crate::generics::Instance,Ty)> {
    let Some(key) = world.resolve(name, caller.module(), caller.imports(), true).unique() else {
        return caller.source().fail(span, "B158", format!("未知函数 {name}"));
    };
    let definition = world.functions[&key].clone();
    if !definition.function.c_abi() {
        return caller.source().fail(span, "B263", "c_function需要明确的extern C声明或定义，不自动改变普通B函数ABI");
    }
    let explicit=arguments()?;
    let instance = generics::specialize(world, &key, &[], explicit.as_deref(), (caller.source(), caller.module(), caller.imports()), reference_span)?;
    let definition = &instance.definition;
    let f = &definition.function;
    let contracts: Vec<_> = instance.contracts().filter_map(|contract| {
        match &contract.predicate {
            ContractPredicate::Nonzero(value) if contract.kind == ContractKind::Requires =>
                Some(ContractPredicate::Nonzero(parameter_name(value, &f.params()))),
            ContractPredicate::Memory(access) if contract.kind==ContractKind::Ensures=>Some(ContractPredicate::Memory(*access)),
            ContractPredicate::Initialized { value, condition: when } =>
                Some(ContractPredicate::Initialized { value: parameter_name(value, &f.params()), condition: condition(when, &f.params()) }),
            _ => None,
        }
    }).collect();
    let context: (&Source, &str, &[String]) = (&definition.source, &definition.module, &definition.imports);
    let result = instance.bindings.type_name(world, &f.result(), context, f.span())?;
    let parameters = f.params().iter().map(|parameter| {
        let ty = instance.bindings.type_name(world, &parameter.ty(), context, parameter.span())?;
        Ok((parameter.access(), ty))
    }).collect::<Result<Vec<_>>>()?;
    let ty = world.bind_c_function_reference(
        &result, parameters.iter().map(|(access, ty)| Ok((*access, ty))), &contracts,
        (context.0, f.span(), context.1, context.2),
    )?.into_ty();
    Ok((instance, ty))
}
impl Builder<'_> {
    pub(super) fn c_function_value(&mut self, args: &Expressions, span: Span) -> Result<Value> {
        self.c_function_from(span,|builder|signature(builder.world,&builder.scope,args,span))
    }
    pub(super) fn c_function_from(&mut self,span:Span,
        signature:impl FnOnce(&mut Self)->Result<(crate::generics::Instance,Ty)>)->Result<Value> {
        if self.program.target != Target::Cpu || !self.application_entry() {
            return self.fail(span, "B265", "c_function在普通CPU应用或模块中取得C函数符号地址");
        }
        let (instance, ty) = signature(self)?;
        let target = if let Some(name) = &instance.definition.function.external() {
            CallTarget::External(ExternalName{library:name.library().into(),symbol:name.symbol().into()})
        } else {
            self.native_declaration(&instance)?;
            CallTarget::Defined(instance.name)
        };
        let reg = self.reg(Scalar::U64);
        let first = self.code.len();
        self.emit(InstKind::FunctionAddress(reg, target));
        if let Some(declaration) = instance.definition.declaration_origin() {
            let declaration = Arc::new(declaration);
            for instruction in &mut self.code[first..] { instruction.reference_declaration(declaration.clone()); }
        }
        let mut proof = bounds::Proof::default(); proof.mark_nonzero(0);
        Ok(Value { proof, literal: false, ty, regs: vec![reg], uniform: true, constant: None })
    }
}
