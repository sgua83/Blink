//! C函数签名只持有原TypeName名义引用；对象布局与实际C调用仍由World产生。
use crate::{ast::*, ir::{CFunctionType, CallParameter, Scalar, Ty}, Source, World};
use std::collections::{BTreeMap, BTreeSet};

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
    let shape = |value: &Shape| match value { Shape::Literal(value) => value.to_string(), Shape::Constant(name) => name.clone(), Shape::Expression(expression)=>expression_name(expression) };
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
    if let Some(outputs) = &ty.contracts {
        for output in outputs.iter() {
            if let ContractPredicate::Nonzero(value) = output {
                name.push_str(&format!(" requires nonzero({value})"));
            } else if let ContractPredicate::Initialized { value, condition } = output {
                name.push_str(&format!(" ensures initialized({value},{})", condition_name(condition)));
            }
        }
    }
    name
}
pub(crate) fn expression_name(expression:&Expr)->String {
    let arguments=|values:&[Expr]|values.iter().map(expression_name).collect::<Vec<_>>().join(",");
    let generic=|values:&[GenericArgument]|values.iter().map(|value|match value {
        GenericArgument::Type(ty)=>type_name(ty),
        GenericArgument::Value(Shape::Literal(value))=>value.to_string(),
        GenericArgument::Value(Shape::Constant(name))=>name.clone(),
        GenericArgument::Value(Shape::Expression(value))=>expression_name(value),
    }).collect::<Vec<_>>().join(",");
    match &expression.kind {
        ExprKind::Number(value)|ExprKind::Name(value)=>value.clone(),
        ExprKind::StaticU32(value)=>value.to_string(),ExprKind::Bool(value)=>value.to_string(),
        ExprKind::Member(base,field)=>format!("{}.{}",expression_name(base),field),
        ExprKind::Index(base,index)=>format!("{}[{}]",expression_name(base),expression_name(index)),
        ExprKind::Unary(op,value)=>format!("({op}{})",expression_name(value)),
        ExprKind::Binary(op,a,b)=>format!("({}{op}{})",expression_name(a),expression_name(b)),
        ExprKind::Select(a,b,c)=>format!("({}?{}:{})",expression_name(a),expression_name(b),expression_name(c)),
        ExprKind::Call(name,args)=>format!("{name}({})",arguments(args)),
        ExprKind::Invoke(callee,args)=>format!("{}({})",expression_name(callee),arguments(args)),
        ExprKind::Construct(ty,args)=>format!("{}({})",type_name(ty),arguments(args)),
        ExprKind::Specialize(name,types,args)=>format!("{name}<{}>({})",generic(types),arguments(args)),
        ExprKind::FunctionRef(name,types)=>format!("{name}<{}>",generic(types)),
        ExprKind::Cooperate(name,types,args)=>format!("cooperate {name}{}({})",types.as_ref().map(|types|format!("<{}>",generic(types))).unwrap_or_default(),arguments(args)),
        ExprKind::Target(target,value)=>format!("target({},{})",match target {Target::Cpu=>"cpu",Target::Gpu=>"gpu",Target::Compute=>"compute"},expression_name(value)),
    }
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
impl World {
    pub(crate) fn check_interface_types(&self, definition: &crate::Definition, actual: &Function,
        types: &BTreeMap<String, TypeName>, values: &BTreeMap<String, u32>,
    ) -> crate::Result<()> {
        if !definition.public || definition.compiled.is_some() { return Ok(()); }
        let interface = &self.interfaces[&definition.module];
        let Some(declaration) = interface.functions.iter().find(|function| {
            function.name == actual.name && function.kind != FunctionKind::Constant && function.external.is_none()
        }) else { return Ok(()); };
        let bind = |name: &TypeName, source: &Source, span: Span| -> crate::Result<TypeName> {
            let mut name = name.clone();
            crate::generics::substitute_type(&mut name, types, source, span)?;
            crate::value_parameters::substitute_type(&mut name, values,source)?;
            self.interface_type(&name, source, span, &definition.module, &definition.imports)
        };
        let mut same = bind(&declaration.result, &interface.source, declaration.span)?
            == self.interface_type(&actual.result, &definition.source, actual.span, &definition.module, &definition.imports)?;
        for (declared, implemented) in declaration.params.iter().zip(&actual.params) {
            same &= bind(&declared.ty, &interface.source, declared.span)?
                == self.interface_type(&implemented.ty, &definition.source, implemented.span, &definition.module, &definition.imports)?;
        }
        if !same {
            return definition.source.fail(actual.span, "B028", format!("{} 的实际类型或函数值合同与 .h 不一致", actual.name));
        }
        Ok(())
    }
    fn interface_type(&self, name: &TypeName, source: &Source, span: Span, module: &str,
        imports: &[String],
    ) -> crate::Result<TypeName> {
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return Ok(self.c_function_type(name, source, span, module, imports)?.syntax());
        }
        let mut name = name.clone();
        for argument in &mut name.arguments {
            if let GenericArgument::Type(ty) = argument {
                *ty = self.interface_type(ty, source, span, module, imports)?;
            }
        }
        Ok(name)
    }

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
                    source: source.clone(), span, code: "B235", message: "const实参需要u32编译期表达式".into(),
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
    /// Contract expressions project nominal fields before requesting a layout;
    /// a callback signature may refer to the containing record itself.
    pub(crate) fn output_path_type(&self, name: &TypeName, fields: &[&str], source: &Source,
        span: Span, module: &str, imports: &[String],
    ) -> crate::Result<Ty> {
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return source.fail(span, if fields.is_empty() { "B148" } else { "B225" },
                "函数签名没有数值运算或字段；原生地址通过function_address显式取得");
        }
        let resolved = self.c_signature_type(name, source, span, module, imports)?;
        let name = &resolved;
        if let Some((field, rest)) = fields.split_first() {
            if *field == "length" && (name.dataset || name.name == "array") {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::U32)) }
                    else { source.fail(span, "B225", "长度值没有后续字段") };
            }
            if let Some(ty) = Ty::builtin(&name.name) {
                let mut ty = ty;
                for field in fields { ty = crate::check::member_type(&ty, field, source, span)?.0; }
                return Ok(ty);
            }
            if name.name == "result" && matches!(*field, "Ok" | "Err") {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::Bool)) }
                    else { source.fail(span, "B225", "变体条件没有后续字段") };
            }
            let key = if name.canonical { self.records.get_key_value(&name.name).map(|(key, _)| key.clone()) }
                else { self.resolve(&name.name, module, imports, false) };
            let Some(key) = key else { return source.fail(span, "B031", format!("未知类型 {}", name.name)); };
            let (record, record_source, record_module, record_imports) = self.record(&key);
            if record.variants.iter().any(|variant| variant.name == *field) {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::Bool)) }
                    else { source.fail(span, "B225", "变体条件没有后续字段") };
            }
            let selected = record.fields.iter().find(|candidate| candidate.name == *field)
                .ok_or_else(|| crate::Diagnostic { source: source.clone(), span, code: "B225",
                    message: format!("类型 {} 没有字段 {field}", name.name) })?;
            let types = record.types.iter().zip(&name.arguments).map(|(parameter, argument)| {
                let GenericArgument::Type(ty) = argument else {
                    return source.fail(span, "B232", "此记录参数需要类型实参");
                };
                Ok((parameter.name.clone(), ty.clone()))
            }).collect::<crate::Result<_>>()?;
            let values = record.values.iter().zip(&name.arguments[record.types.len()..]).map(|(parameter, argument)| {
                let shape = argument.shape().unwrap();
                Ok((parameter.name.clone(), self.shape(&Some(shape), source, span, module, imports)?.unwrap()))
            }).collect::<crate::Result<_>>()?;
            let mut selected = selected.ty.clone();
            crate::generics::substitute_type(&mut selected, &types, record_source, span)?;
            crate::value_parameters::substitute_type(&mut selected, &values,record_source)?;
            return self.output_path_type(&selected, rest, record_source, span, record_module, record_imports);
        }
        // A literal cannot numerically operate on an ordinary record. Decide
        // that existing type error from its declaration, without expanding a
        // recursive callback just to discover the record's machine layout.
        if let Some(key) = if name.canonical { self.records.contains_key(&name.name).then_some(name.name.clone()) }
            else { self.resolve(&name.name, module, imports, false) } {
            let record = &self.record(&key).0;
            if record.enumeration.is_none() {
                return source.fail(span, "B147", "结构体和资源视图不能作为数值进行运算");
            }
        }
        self.ty(name, source, span, module, imports)
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
        let contracts = crate::check::outputs::bind_function_contracts(
            self, &result, &parsed, name.contracts.as_ref().map(|value| value.as_slice()).unwrap_or(&[]),
            source, span, module, imports)?;
        let function = CFunctionType { result, parameters: parsed, contracts };
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
            crate::check::outputs::validate_function_contracts(
                &parameters, result.as_ref(), &function.contracts, source, span)?;
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
