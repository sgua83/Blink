//! C函数签名只持有原TypeName名义引用；对象布局与实际C调用仍由World产生。
use crate::{ast::*, ir::{CFunctionType, CallParameter, Scalar, Ty}, Source, World};
use std::collections::BTreeSet;

fn ordinary(ty: &Ty, syntax: &TypeName, source: &Source, span: Span) -> crate::Result<()> {
    if syntax.dataset || ty.owns_resources() {
        return source.fail(span, "B224", "cfn普通C签名不能交接Blink拥有资源");
    }
    if ty.contains_views() || ty.components.is_empty() {
        return source.fail(span, "B263", "cfn需要普通非空C布局，不能交接Blink资源引用");
    }
    Ok(())
}
fn type_name(ty: &TypeName) -> String {
    let shape = |value: &Shape| match value { Shape::Literal(value) => value.to_string(), Shape::Constant(name) => name.clone() };
    let mut name = ty.name.clone();
    if !ty.arguments.is_empty() {
        name.push('<');
        name.push_str(&ty.arguments.iter().map(|argument| match argument {
            GenericArgument::Type(ty) => type_name(ty),
            GenericArgument::Value(value) => shape(value),
        }).collect::<Vec<_>>().join(","));
        name.push('>');
    }
    for (present, prefix) in [(ty.dataset, "dataset"), (ty.view, "view"), (ty.tile, "tile")] {
        if present {
            name = if let Some(count) = &ty.count { format!("{prefix}<{name},{}>", shape(count)) }
                else { format!("{prefix}<{name}>") };
        }
    }
    if let Some(outputs) = &ty.outputs {
        for output in outputs.iter() {
            if let ContractPredicate::Initialized { value, condition } = output {
                name.push_str(&format!(" ensures initialized({value},{})", condition_name(condition)));
            }
        }
    }
    name
}
fn condition_name(condition: &OutputCondition) -> String {
    match condition {
        OutputCondition::Name(name) | OutputCondition::Number(name) => name.clone(),
        OutputCondition::Bool(value) => value.to_string(),
        OutputCondition::StaticU32(value) => value.to_string(),
        OutputCondition::Member(base, field) => format!("{}.{}", condition_name(base), field),
        OutputCondition::Unary(op, value) => format!("({op}{})", condition_name(value)),
        OutputCondition::Binary(op, a, b) => format!("({}{op}{})", condition_name(a), condition_name(b)),
        OutputCondition::Constant(value) => format!("{}[{}]", value.ty.name,
            value.bits.iter().map(|bits| format!("{bits:016x}")).collect::<Vec<_>>().join(",")),
    }
}
impl CFunctionType {
    pub(crate) fn syntax(&self) -> TypeName {
        let mut syntax = TypeName::named("cfn");
        syntax.arguments.push(GenericArgument::Type(self.result.clone()));
        syntax.arguments.extend(self.parameters.iter().map(|(access, ty)| {
            let wrapper = match access {
                Access::Value => None, Access::Read => Some("read"), Access::Write => Some("write"),
                Access::InOut => Some("inout"), Access::Atomic => Some("atomic"),
            };
            let ty = if let Some(wrapper) = wrapper {
                let mut syntax = TypeName::named(wrapper);
                syntax.arguments.push(GenericArgument::Type(ty.clone()));
                syntax
            } else { ty.clone() };
            GenericArgument::Type(ty)
        }));
        if !self.outputs.is_empty() { syntax.outputs = Some(Box::new(self.outputs.clone())); }
        syntax
    }
}
impl World {
    // Resolve names and const arguments without asking a signature reference for its record layout.
    fn c_signature_type(&self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<TypeName> {
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return Ok(self.c_function_type(name, source, span, module, imports)?.syntax());
        }
        let mut syntax = name.clone();
        syntax.canonical = false;
        let (types, count, code) = match name.name.as_str() {
            "array" => (1, 2, "B238"),
            "result" => (2, 2, "B269"),
            "owned" => (1, 2, "B224"),
            "void" | "never" => (0, 0, "B228"),
            _ => {
                if let Some(builtin) = Ty::builtin(&name.name) {
                    syntax.name = builtin.name;
                    (0, 0, "B232")
                } else {
                    let key = if name.canonical {
                        self.records.get_key_value(&name.name).map(|(key, _)| key.clone())
                    } else { self.resolve(&name.name, module, imports, false) };
                    let Some(key) = key else { return source.fail(span, "B031", format!("未知或不明确的类型 {}", name.name)); };
                    let record = &self.record(&key).0;
                    syntax.name = key;
                    syntax.canonical = true;
                    (record.types.len(), record.types.len() + record.values.len(), "B232")
                }
            }
        };
        if name.arguments.len() != count {
            return source.fail(span, code, format!("类型 {} 需要 {count} 个类型及const实参，实际为 {}", syntax.name, name.arguments.len()));
        }
        syntax.arguments.clear();
        for (index, argument) in name.arguments.iter().enumerate() {
            let argument = if name.name == "owned" && index == 1 {
                let GenericArgument::Type(function) = argument else {
                    return source.fail(span, "B224", "owned<T,F>需要载荷类型与静态释放函数");
                };
                if function.dataset || function.tile || function.view || !function.arguments.is_empty() {
                    return source.fail(span, "B224", "释放函数由载荷推导泛型，声明处使用函数名称");
                }
                let key = if function.canonical && self.functions.contains_key(&function.name) { Some(function.name.clone()) }
                    else { self.resolve(&function.name, module, imports, true) };
                let Some(key) = key else { return source.fail(span, "B158", format!("未知或未导出的释放函数 {}", function.name)); };
                let mut function = TypeName::named(key);
                function.canonical = true;
                GenericArgument::Type(function)
            } else if index < types {
                let GenericArgument::Type(ty) = argument else { return source.fail(span, code, "此位置需要类型实参"); };
                GenericArgument::Type(self.c_signature_type(ty, source, span, module, imports)?)
            } else {
                let value = argument.shape().ok_or_else(|| crate::Diagnostic {
                    source: source.clone(), span, code: "B235", message: "const实参需要u32字面量或const名称".into(),
                })?;
                let value = self.shape(&Some(value), source, span, module, imports)?.unwrap();
                GenericArgument::Value(Shape::Literal(value))
            };
            syntax.arguments.push(argument);
        }
        if let Some(count) = &name.count {
            syntax.count = Some(Shape::Literal(self.shape(&Some(count.clone()), source, span, module, imports)?.unwrap()));
        }
        Ok(syntax)
    }
    pub(crate) fn c_function_type(&self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<Ty> {
        let Some((result, parameters)) = name.arguments.split_first() else {
            return source.fail(span, "B263", "cfn<R,P...>至少需要一个返回类型");
        };
        let GenericArgument::Type(result) = result else { return source.fail(span, "B263", "cfn签名只接受类型，不接受const数值"); };
        let result = self.c_signature_type(result, source, span, module, imports)?;
        let mut parsed = Vec::new();
        for parameter in parameters {
            let GenericArgument::Type(parameter) = parameter else { return source.fail(span, "B263", "cfn签名只接受类型，不接受const数值"); };
            let access = if !parameter.dataset && !parameter.view && !parameter.tile {
                match parameter.name.as_str() { "read" => Access::Read, "write" => Access::Write, "inout" => Access::InOut, _ => Access::Value }
            } else { Access::Value };
            let parameter = if access == Access::Value { parameter } else {
                let [GenericArgument::Type(element)] = parameter.arguments.as_slice() else {
                    return source.fail(span, "B263", "cfn位置参数使用read<T>／write<T>／inout<T>");
                };
                element
            };
            parsed.push((access, self.c_signature_type(parameter, source, span, module, imports)?));
        }
        let outputs = crate::check::outputs::bind_function_outputs(
            self, &parsed, name.outputs.as_ref().map(|value| value.as_slice()).unwrap_or(&[]),
            source, span, module, imports)?;
        let function = CFunctionType { result, parameters: parsed, outputs };
        let mut ty = Ty::scalar(Scalar::U64);
        ty.name = type_name(&function.syntax());
        ty.function = Some(Box::new(function));
        Ok(ty)
    }
    pub(crate) fn c_function_signature(&self, function: &CFunctionType, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<(Vec<CallParameter>, Option<Ty>, bool)> {
        let mut parameters = Vec::new();
        for (access, syntax) in &function.parameters {
            let ty = self.ty_inner(syntax, source, span, module, imports, &mut BTreeSet::new())?;
            ordinary(&ty, syntax, source, span)?;
            parameters.push(if *access == Access::Value { CallParameter::Value(ty) }
                else { CallParameter::Reference { ty, access: *access } });
        }
        let never = function.result.is_never();
        let result = if never || function.result == TypeName::named("void") { None } else {
            let ty = self.ty_inner(&function.result, source, span, module, imports, &mut BTreeSet::new())?;
            ordinary(&ty, &function.result, source, span)?;
            Some(ty)
        };
        Ok((parameters, result, never))
    }
    pub(crate) fn validate_c_functions(&self, ty: &Ty, source: &Source, span: Span, module: &str, imports: &[String], consumed: &mut BTreeSet<String>) -> crate::Result<()> {
        if let Some(function) = &ty.function && consumed.insert(ty.name.clone()) {
            let (parameters, result, _) = self.c_function_signature(function, source, span, module, imports)?;
            crate::check::outputs::validate_function_outputs(
                &parameters, result.as_ref(), &function.outputs, source, span)?;
            for parameter in parameters {
                let ty = match parameter { CallParameter::Value(ty) | CallParameter::Reference { ty, .. } => ty };
                self.validate_c_functions(&ty, source, span, module, imports, consumed)?;
            }
            if let Some(result) = result { self.validate_c_functions(&result, source, span, module, imports, consumed)?; }
        }
        for nested in ty.arguments.iter().chain(ty.fields.iter().map(|(_, _, ty)| ty.as_ref()))
            .chain(ty.variants.iter().map(|variant| variant.ty.as_ref()))
            .chain(ty.array.as_deref()).chain(ty.view.as_deref()).chain(ty.dataset.as_deref()) {
            self.validate_c_functions(nested, source, span, module, imports, consumed)?;
        }
        Ok(())
    }
}
