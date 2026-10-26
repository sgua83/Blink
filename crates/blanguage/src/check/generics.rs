//! 从值、位置及资源的名义类型参数推导；原访问权限与资源所有权保持。
use super::*;
use cooperative::Argument;
pub(crate) struct Actual<'a> {
    pub ty: Option<&'a Ty>,
    pub count: Option<u32>,
}
impl<'a> Actual<'a> {
    pub fn value(ty: &'a Ty) -> Self {
        Self {
            ty: Some(ty),
            count: None,
        }
    }
}

impl Builder<'_> {
    pub(super) fn specialized_function(
        &self,
        key: String,
        args: &[Argument],
        span: Span,
    ) -> Result<crate::generics::Instance> {
        self.specialized_arguments(key, args, None, span)
    }
    pub(super) fn specialized_arguments(
        &self,
        key: String,
        args: &[Argument],
        explicit: Option<&[GenericArgument]>,
        span: Span,
    ) -> Result<crate::generics::Instance> {
        let types: Vec<_> = args
            .iter()
            .map(|arg| match arg {
                Argument::Acceleration(_) => Actual {
                    ty: None,
                    count: None,
                },
                Argument::Texture(t) => Actual {
                    ty: None,
                    count: t.shape(self.program),
                },
                Argument::Value(v) => Actual::value(&v.ty),
                Argument::Dataset(v) => Actual {
                    ty: Some(&self.program.buffers[v.binding].ty),
                    count: self.program.buffers[v.binding].count,
                },
                Argument::ApplicationData(data) => Actual {
                    ty: Some(&data.ty),
                    count: data.count,
                },
                Argument::Tile(v) => Actual::value(&v.ty),
                Argument::Atomic(p) | Argument::Exclusive(p) => Actual::value(p.ty()),
            })
            .collect();
        specialize(self.world, &key, &types, explicit, &self.definition, span)
    }
    pub(super) fn specialized_call(
        &mut self,
        name: &str,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        if name == "array"
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
            let mut ty = TypeName::named(name);
            ty.arguments = explicit.to_vec();
            let values = args
                .iter()
                .map(|arg| self.expr(arg))
                .collect::<Result<Vec<_>>>()?;
            return self.record_value(&ty, values, span).map(Some);
        }
        let key = self
            .world
            .resolve(
                name,
                &self.definition.module,
                &self.definition.imports,
                true,
            )
            .ok_or_else(|| Diagnostic {
                source: self.definition.source.clone(),
                span,
                code: "B158",
                message: format!("未知或未导出的泛型函数 {name}"),
            })?;
        let arguments = self.function_arguments(&key, args, span)?;
        let instance = self.specialized_arguments(key, &arguments, Some(explicit), span)?;
        self.invoke_instance(instance, arguments, span)
    }
}
/// 类型与静态 u32 值共用原名义实例；显式调用可给全部实参或只给 const 后缀。
pub(crate) fn specialize(
    world: &World,
    key: &str,
    actuals: &[Actual<'_>],
    explicit: Option<&[GenericArgument]>,
    caller: &Definition,
    span: Span,
) -> Result<crate::generics::Instance> {
    let source = &caller.source;
    let callee = &world.functions[key];
    let f = &callee.function;
    if f.types.is_empty() && f.values.is_empty() && explicit.is_none() {
        return crate::generics::instantiate(world, key, callee, Vec::new(), &[]);
    }
    let (explicit_types, explicit_values) = if let Some(args) = explicit {
        if args.len() == f.types.len() + f.values.len() && !args.is_empty() {
            args.split_at(f.types.len())
        } else if args.len() == f.values.len() && !f.values.is_empty() {
            (&[][..], args)
        } else {
            return source.fail(
                span,
                "B235",
                "泛型实参必须完整给出，或只给全部 const 实参并推导类型",
            );
        }
    } else {
        (&[][..], &[][..])
    };
    let mut types = Vec::new();
    let mut values = Vec::new();
    for (index, parameter) in f.types.iter().enumerate() {
        let actual = if let Some(arg) = explicit_types.get(index) {
            let GenericArgument::Type(ty) = arg else {
                return source.fail(span, "B231", "此泛型位置需要普通值类型");
            };
            if ty.dataset || ty.view || ty.tile {
                return source.fail(span, "B231", "类型实参不能直接是资源包装");
            }
            let actual = world.ty(ty, source, span, &caller.module, &caller.imports)?;
            if !world.family_accepts(
                &parameter.family,
                &actual,
                &callee.source,
                f.span,
                &callee.module,
                &callee.imports,
                &mut BTreeSet::new(),
            )? {
                return source.fail(
                    span,
                    "B231",
                    format!(
                        "类型 {} 不属于参数 {} 的允许集合",
                        actual.name, parameter.name
                    ),
                );
            }
            actual
        } else {
            let mut inferred = None;
            for (formal, actual) in f.params.iter().zip(actuals) {
                if !formal.ty.mentions(&parameter.name) {
                    continue;
                }
                let actual = actual.ty.ok_or_else(|| Diagnostic {
                    source: source.clone(),
                    span,
                    code: "B231",
                    message: "资源句柄不能用于普通值类型参数推导".into(),
                })?;
                infer_type(
                    world,
                    source,
                    &formal.ty,
                    actual,
                    parameter,
                    callee,
                    span,
                    &mut inferred,
                )?;
            }
            inferred.ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B231",
                message: format!(
                    "无法推导类型参数 {}，请显式指定全部类型实参",
                    parameter.name
                ),
            })?
        };
        types.push(actual.syntax());
    }
    for (index, parameter) in f.values.iter().enumerate() {
        let value = if let Some(arg) = explicit_values.get(index) {
            let shape = arg.shape().ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B235",
                message: "const 实参需要 u32 字面量或 const 名称".into(),
            })?;
            world
                .shape(&Some(shape), source, span, &caller.module, &caller.imports)?
                .unwrap()
        } else {
            let mut inferred = None;
            for (formal, actual) in f.params.iter().zip(actuals) {
                let shape = if formal.ty.is_texture_resource() {
                    formal.ty.texture_array_shape()
                } else if formal.ty.dataset {
                    formal.ty.count.clone()
                } else {
                    None
                };
                if shape == Some(Shape::Constant(parameter.name.clone()))
                    && let Some(value) = actual.count
                {
                    merge_value(&mut inferred, value, source, span)?;
                }
                if let Some(actual) = actual.ty {
                    infer_value(
                        world,
                        &formal.ty,
                        actual,
                        &parameter.name,
                        callee,
                        source,
                        span,
                        &mut inferred,
                    )?;
                }
            }
            inferred.ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B235",
                message: format!(
                    "const 参数 {} 无法从静态形状或名义类型推导，请显式指定",
                    parameter.name
                ),
            })?
        };
        if parameter
            .choices
            .as_ref()
            .is_some_and(|choices| !choices.contains(&value))
        {
            return source.fail(
                span,
                "B235",
                format!("const 实参 {value} 不属于 {} 的允许值域", parameter.name),
            );
        }
        values.push(value);
    }
    crate::generics::instantiate(world, key, callee, types, &values)
}
fn merge_value(inferred: &mut Option<u32>, value: u32, source: &Source, span: Span) -> Result<()> {
    if inferred.is_some_and(|previous| previous != value) {
        return source.fail(span, "B235", "const 参数的重复静态推导不一致");
    }
    *inferred = Some(value);
    Ok(())
}
fn infer_type(
    world: &World,
    source: &Source,
    formal: &TypeName,
    actual: &Ty,
    parameter: &TypeParameter,
    callee: &Definition,
    span: Span,
    inferred: &mut Option<Ty>,
) -> Result<()> {
    if !formal.mentions(&parameter.name) {
        return Ok(());
    }
    let actual = if formal.view {
        actual.view.as_deref().ok_or_else(|| Diagnostic {
            source: source.clone(),
            span,
            code: "B231",
            message: "view 类型参数需要不可变 view 实参或位置".into(),
        })?
    } else {
        actual
    };
    if formal.name == parameter.name {
        if !formal.arguments.is_empty() {
            return source.fail(span, "B232", "类型参数已代表完整类型，不能再附加实参");
        }
        if !world.family_accepts(
            &parameter.family,
            actual,
            &callee.source,
            span,
            &callee.module,
            &callee.imports,
            &mut BTreeSet::new(),
        )? {
            return source.fail(
                span,
                "B231",
                format!(
                    "类型 {} 不满足参数 {} 的普通值类型或有限集合约束",
                    actual.name, parameter.name
                ),
            );
        }
        if let Some(previous) = inferred {
            if previous != actual {
                return source.fail(
                    span,
                    "B124",
                    format!("类型不匹配：需要 {}，实际为 {}", previous.name, actual.name),
                );
            }
        } else {
            *inferred = Some(actual.clone());
        }
        return Ok(());
    }
    if formal.name == "array" {
        let Some(element) = &actual.array else {
            return source.fail(span, "B238", "array 类型推导需要相同数组种类");
        };
        let Some(GenericArgument::Type(formal_element)) = formal.arguments.first() else {
            return source.fail(span, "B238", "array 首参数需要值类型");
        };
        if formal.arguments.len() != 2 {
            return source.fail(span, "B238", "array 需要元素类型和固定长度");
        }
        return infer_type(
            world,
            source,
            formal_element,
            element,
            parameter,
            callee,
            span,
            inferred,
        );
    }
    let base = world
        .resolve(&formal.name, &callee.module, &callee.imports, false)
        .ok_or_else(|| Diagnostic {
            source: callee.source.clone(),
            span,
            code: "B031",
            message: format!("未知或不明确的记录类型 {}", formal.name),
        })?;
    let expected = crate::generics::key(
        &base,
        &actual
            .arguments
            .iter()
            .map(|t| t.name.clone())
            .chain(actual.values.iter().map(u32::to_string))
            .collect::<Vec<_>>(),
    );
    if actual.name != expected
        || formal.arguments.len() != actual.arguments.len() + actual.values.len()
    {
        return source.fail(
            span,
            "B232",
            format!("不能从 {} 推导记录 {} 的类型参数", actual.name, formal.name),
        );
    }
    for (formal, actual) in formal.arguments.iter().zip(&actual.arguments) {
        let GenericArgument::Type(formal) = formal else {
            return source.fail(span, "B232", "记录类型位置需要类型实参");
        };
        infer_type(
            world, source, formal, actual, parameter, callee, span, inferred,
        )?;
    }
    Ok(())
}

fn infer_value(
    world: &World,
    formal: &TypeName,
    actual: &Ty,
    name: &str,
    callee: &Definition,
    source: &Source,
    span: Span,
    inferred: &mut Option<u32>,
) -> Result<()> {
    let actual = if formal.view {
        let Some(actual) = actual.view.as_deref() else {
            return Ok(());
        };
        actual
    } else {
        actual
    };
    if formal.arguments.is_empty() {
        return Ok(());
    }
    if formal.name == "array" {
        let Some(element) = &actual.array else {
            return source.fail(span, "B238", "array 值推导需要相同数组种类");
        };
        if formal.arguments.len() != 2 {
            return source.fail(span, "B238", "array 需要元素类型和固定长度");
        }
        if let GenericArgument::Type(formal_element) = &formal.arguments[0] {
            infer_value(
                world,
                formal_element,
                element,
                name,
                callee,
                source,
                span,
                inferred,
            )?;
        }
        if formal.arguments[1].shape() == Some(Shape::Constant(name.into())) {
            let value = actual.values[0];
            if inferred.is_some_and(|previous| previous != value) {
                return source.fail(span, "B235", "array 的重复长度推导不一致");
            }
            *inferred = Some(value);
        }
        return Ok(());
    }
    let base = world
        .resolve(&formal.name, &callee.module, &callee.imports, false)
        .ok_or_else(|| Diagnostic {
            source: callee.source.clone(),
            span,
            code: "B031",
            message: format!("未知记录 {}", formal.name),
        })?;
    let expected = crate::generics::key(
        &base,
        &actual
            .arguments
            .iter()
            .map(|ty| ty.name.clone())
            .chain(actual.values.iter().map(u32::to_string))
            .collect::<Vec<_>>(),
    );
    if expected != actual.name
        || formal.arguments.len() != actual.arguments.len() + actual.values.len()
    {
        return source.fail(span, "B232", "无法从不同名义记录推导 const 参数");
    }
    for (arg, ty) in formal.arguments.iter().zip(&actual.arguments) {
        if let GenericArgument::Type(arg) = arg {
            infer_value(world, arg, ty, name, callee, source, span, inferred)?;
        }
    }
    for (arg, value) in formal.arguments[actual.arguments.len()..]
        .iter()
        .zip(&actual.values)
    {
        if arg.shape() == Some(Shape::Constant(name.into())) {
            if let Some(previous) = *inferred {
                if previous != *value {
                    return source.fail(
                        span,
                        "B235",
                        format!("const 参数 {name} 的名义实参不一致：{previous} 与 {value}"),
                    );
                }
            } else {
                *inferred = Some(*value);
            }
        }
    }
    Ok(())
}
