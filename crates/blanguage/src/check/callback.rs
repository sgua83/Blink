//! 明确C ABI定义的原生符号直接交给调用者；不创建回调宿主或Frame。
use super::*;

fn parameter_name(name: &str, parameters: &[Param]) -> String {
    let (base, tail) = name.split_once('.').map_or((name, ""), |(a, b)| (a, b));
    let Some(index) = parameters.iter().position(|parameter| parameter.name == base) else { return name.into(); };
    if tail.is_empty() { format!("argument{index}") } else { format!("argument{index}.{tail}") }
}
fn condition(value: &OutputCondition, parameters: &[Param]) -> OutputCondition {
    match value {
        OutputCondition::Name(name) => OutputCondition::Name(parameter_name(name, parameters)),
        OutputCondition::Member(base, field) => OutputCondition::Member(Box::new(condition(base, parameters)), field.clone()),
        OutputCondition::Unary(op, value) => OutputCondition::Unary(op.clone(), Box::new(condition(value, parameters))),
        OutputCondition::Binary(op, a, b) => OutputCondition::Binary(op.clone(), Box::new(condition(a, parameters)), Box::new(condition(b, parameters))),
        value => value.clone(),
    }
}
impl Builder<'_> {
    pub(super) fn c_function_value(&mut self, args: &[Expr], span: Span) -> Result<Value> {
        if self.program.target != Target::Cpu || self.application.is_none() {
            return self.fail(span, "B265", "c_function在普通CPU应用或模块中取得C函数符号地址");
        }
        let [Expr { kind: ExprKind::Name(name), .. }] = args else {
            return self.fail(span, "B153", "c_function需要一个明确的B C ABI函数名");
        };
        let Some(key) = self.world.resolve(name, &self.definition.module, &self.definition.imports, true) else {
            return self.fail(span, "B158", format!("未知函数 {name}"));
        };
        let definition = self.world.functions[&key].clone();
        if !definition.function.c_abi || definition.function.external.is_some() {
            return self.fail(span, "B263", "c_function需要B定义的extern C函数，不自动改变普通B函数ABI");
        }
        let f = &definition.function;
        let outputs = f.contracts.iter().filter_map(|contract| {
            if let ContractPredicate::Initialized { value, condition: when } = &contract.predicate {
                Some(ContractPredicate::Initialized { value: parameter_name(value, &f.params), condition: condition(when, &f.params) })
            } else { None }
        }).collect();
        let syntax = CFunctionType {
            result: f.result.clone(),
            parameters: f.params.iter().map(|parameter| (parameter.access, parameter.ty.clone())).collect(),
            outputs,
        }.syntax();
        let ty = self.world.c_function_type(&syntax, &definition.source, f.span, &definition.module, &definition.imports)?;
        let instance = crate::generics::instantiate(self.world, &key, &definition, Vec::new(), &[])?;
        self.native_declaration(&instance)?;
        let reg = self.reg(Scalar::U64);
        self.code.push(Inst::FunctionAddress(reg, instance.name));
        let mut proof = bounds::Proof::default(); proof.mark_nonzero(0);
        Ok(Value { proof, literal: false, ty, regs: vec![reg], uniform: true, constant: None })
    }
}
