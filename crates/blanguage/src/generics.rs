//! 从声明绑定具体类型与静态值；具体实例沿原入口检查和 IR 执行。
use crate::{World, ast::*, ir::Ty};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn is_open(types: &[TypeParameter], values: &[ValueParameter]) -> bool {
    types.iter().any(|p| p.family.is_none()) || values.iter().any(|p| p.choices.is_none())
}
pub(crate) fn key(base: &str, types: &[String]) -> String {
    format!("{base}<{}>", types.join(","))
}
#[derive(Clone)]
pub(crate) struct Instance {
    pub name: String,
    pub definition: std::sync::Arc<crate::Definition>,
}
pub(crate) fn instantiate(
    world: &World,
    base: &str,
    definition: &std::sync::Arc<crate::Definition>,
    arguments: Vec<TypeName>,
    values: &[u32],
) -> Result<Instance> {
    let function = &definition.function;
    if function.types.is_empty() && function.values.is_empty() {
        return Ok(Instance {
            name: base.into(),
            definition: definition.clone(),
        });
    }
    let mut names = arguments
        .iter()
        .map(|ty| {
            world
                .ty(
                    ty,
                    &definition.source,
                    function.span,
                    &definition.module,
                    &definition.imports,
                )
                .map(|ty| ty.name)
        })
        .collect::<Result<Vec<_>>>()?;
    let replacements: BTreeMap<_, _> = function
        .types
        .iter()
        .zip(arguments)
        .map(|(p, t)| (p.name.clone(), t))
        .collect();
    let value_map: BTreeMap<_, _> = function
        .values
        .iter()
        .zip(values)
        .map(|(p, v)| (p.name.clone(), *v))
        .collect();
    let mut concrete = (**definition).clone();
    let mut contracts = Vec::new();
    for contract in &function.contracts {
        if contract.kind == ContractKind::Requires
            && let ContractPredicate::Nonzero(name) = &contract.predicate
            && let Some(value) = value_map.get(name)
        {
            if *value == 0 {
                return definition.source.fail(
                    contract.span,
                    "B226",
                    format!("const 参数 {name}=0 不满足 requires nonzero({name})"),
                );
            }
            continue;
        }
        contracts.push(contract.clone());
    }
    concrete.function.contracts = contracts;
    concrete.function.types.clear();
    concrete.function.values.clear();
    substitute_type(
        &mut concrete.function.result,
        &replacements,
        &definition.source,
        function.span,
    )?;
    crate::value_parameters::substitute_type(&mut concrete.function.result, &value_map);
    for p in &mut concrete.function.params {
        substitute_type(&mut p.ty, &replacements, &definition.source, p.span)?;
        crate::value_parameters::substitute_type(&mut p.ty, &value_map);
    }
    if let Some(body) = &mut concrete.function.body {
        substitute_block(body, &replacements, &value_map, &definition.source)?;
    }
    names.extend(values.iter().map(u32::to_string));
    Ok(Instance {
        name: key(base, &names),
        definition: std::sync::Arc::new(concrete),
    })
}
pub(crate) fn validate_declaration(definition: &crate::Definition) -> Result<()> {
    let f = &definition.function;
    crate::check::validate_signature(definition)?;
    validate_parameters(&f.types, &f.name, &definition.source, f.span)?;
    crate::value_parameters::validate(&f.values, &f.name, &f.params, &definition.source, f.span)
}
pub(crate) fn instances(
    world: &World,
    base: &str,
    definition: &std::sync::Arc<crate::Definition>,
) -> Result<Vec<Instance>> {
    let f = &definition.function;
    let values = crate::value_parameters::combinations(&f.values);
    let mut result = Vec::new();
    for arguments in combinations(
        world,
        &f.types,
        &definition.source,
        f.span,
        &definition.module,
        &definition.imports,
    )? {
        for values in &values {
            result.push(instantiate(
                world,
                base,
                definition,
                arguments.clone(),
                values,
            )?);
        }
    }
    Ok(result)
}
pub(crate) fn validate_parameters(
    types: &[TypeParameter],
    owner: &str,
    source: &Source,
    span: Span,
) -> Result<()> {
    for parameter in types {
        if Ty::builtin(&parameter.name).is_some()
            || matches!(
                parameter.name.as_str(),
                "void" | "dataset" | "view" | "tile" | "array" | "acceleration" | "texture2d"
            )
            || parameter.name == owner
        {
            return source.fail(span, "B231", "类型参数不能覆盖内置类型或当前声明名");
        }
    }
    Ok(())
}
pub(crate) fn combinations(
    world: &World,
    types: &[TypeParameter],
    source: &Source,
    span: Span,
    module: &str,
    imports: &[String],
) -> Result<Vec<Vec<TypeName>>> {
    let mut result = vec![Vec::new()];
    for parameter in types {
        let Some(family) = &parameter.family else {
            result.clear();
            continue;
        };
        let members =
            world.family_members(family, source, span, module, imports, &mut BTreeSet::new())?;
        result = result
            .into_iter()
            .flat_map(|prefix| {
                members.iter().map(move |(name, _)| {
                    let mut args = prefix.clone();
                    args.push(name.clone());
                    args
                })
            })
            .collect();
    }
    Ok(result)
}
pub(crate) fn substitute_type(
    ty: &mut TypeName,
    replacements: &BTreeMap<String, TypeName>,
    source: &Source,
    span: Span,
) -> Result<()> {
    for argument in &mut ty.arguments {
        if let GenericArgument::Type(ty) = argument {
            substitute_type(ty, replacements, source, span)?;
        }
    }
    if let Some(actual) = replacements.get(&ty.name) {
        if !ty.arguments.is_empty() {
            return source.fail(
                span,
                "B232",
                "类型参数已代表完整具体类型，不能再附加类型实参",
            );
        }
        let (dataset, view, tile, count) = (ty.dataset, ty.view, ty.tile, ty.count.clone());
        *ty = actual.clone();
        ty.dataset = dataset;
        ty.view = view;
        ty.tile = tile;
        ty.count = count;
    }
    Ok(())
}
fn substitute_reference(
    name: &str,
    explicit: &mut [GenericArgument],
    types: &BTreeMap<String, TypeName>,
    values: &BTreeMap<String, u32>,
    source: &Source,
    span: Span,
) -> Result<()> {
    if types.contains_key(name) {
        return source.fail(span, "B232", "类型参数已代表完整类型，不能再附加实参");
    }
    if values.contains_key(name) {
        return source.fail(span, "B235", "const 参数不是可调用函数");
    }
    for arg in explicit {
        if let GenericArgument::Type(ty) = arg {
            substitute_type(ty, types, source, span)?;
        }
        crate::value_parameters::substitute_argument(arg, values);
    }
    Ok(())
}
fn substitute_expr(
    expr: &mut Expr,
    types: &BTreeMap<String, TypeName>,
    values: &BTreeMap<String, u32>,
    source: &Source,
) -> Result<()> {
    match &mut expr.kind {
        ExprKind::Name(name) if values.contains_key(name) => {
            expr.kind = ExprKind::StaticU32(values[name]);
        }
        ExprKind::Construct(ty, args) => {
            substitute_type(ty, types, source, expr.span)?;
            crate::value_parameters::substitute_type(ty, values);
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::Call(name, args) => {
            if values.contains_key(name) {
                return source.fail(expr.span, "B235", "const 参数不是可调用函数");
            }
            for arg in args.iter_mut() {
                substitute_expr(arg, types, values, source)?;
            }
            if let Some(actual) = types.get(name) {
                if actual.arguments.is_empty()
                    && Ty::builtin(&actual.name).is_some_and(|t| t.fields.is_empty())
                {
                    name.clone_from(&actual.name);
                } else {
                    expr.kind = ExprKind::Construct(actual.clone(), std::mem::take(args));
                }
            }
        }
        ExprKind::Specialize(name, explicit, args)
        | ExprKind::Cooperate(name, Some(explicit), args) => {
            substitute_reference(name, explicit, types, values, source, expr.span)?;
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::FunctionRef(name, explicit) => {
            substitute_reference(name, explicit, types, values, source, expr.span)?;
        }
        ExprKind::Cooperate(name, None, args) => {
            if values.contains_key(name) {
                return source.fail(expr.span, "B235", "const 参数不是可调用函数");
            }
            for arg in args {
                substitute_expr(arg, types, values, source)?;
            }
        }
        ExprKind::Member(base, _) | ExprKind::Unary(_, base) => {
            substitute_expr(base, types, values, source)?
        }
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            substitute_expr(a, types, values, source)?;
            substitute_expr(b, types, values, source)?;
        }
        ExprKind::Select(a, b, c) => {
            substitute_expr(a, types, values, source)?;
            substitute_expr(b, types, values, source)?;
            substitute_expr(c, types, values, source)?;
        }
        _ => {}
    }
    Ok(())
}
fn substitute_block(
    body: &mut [Stmt],
    types: &BTreeMap<String, TypeName>,
    values: &BTreeMap<String, u32>,
    source: &Source,
) -> Result<()> {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Declare(_, name)
            | StmtKind::Let(_, name, _)
            | StmtKind::For(name, _, _, _)
            | StmtKind::Parallel(name, _, _)
                if values.contains_key(name) =>
            {
                return source.fail(stmt.span, "B235", "局部绑定不能覆盖本函数的 const 参数");
            }
            _ => {}
        }
        match &mut stmt.kind {
            StmtKind::Declare(ty, _) => {
                substitute_type(ty, types, source, stmt.span)?;
                crate::value_parameters::substitute_type(ty, values);
            }
            StmtKind::Let(annotation, _, expr) => {
                if let Some(ty) = annotation {
                    substitute_type(ty, types, source, stmt.span)?;
                    crate::value_parameters::substitute_type(ty, values);
                }
                substitute_expr(expr, types, values, source)?;
            }
            StmtKind::Assign(a, _, b) => {
                substitute_expr(a, types, values, source)?;
                substitute_expr(b, types, values, source)?;
            }
            StmtKind::Expr(e) | StmtKind::Seek(_, e) | StmtKind::Return(Some(e)) => {
                substitute_expr(e, types, values, source)?
            }
            StmtKind::If(c, a, b) => {
                substitute_expr(c, types, values, source)?;
                substitute_block(a, types, values, source)?;
                substitute_block(b, types, values, source)?;
            }
            StmtKind::While(c, b) => {
                substitute_expr(c, types, values, source)?;
                substitute_block(b, types, values, source)?;
            }
            StmtKind::For(_, a, b, body) => {
                substitute_expr(a, types, values, source)?;
                if let Some(b) = b {
                    substitute_expr(b, types, values, source)?;
                }
                substitute_block(body, types, values, source)?;
            }
            StmtKind::Parallel(_, domain, body) => {
                if let Some(c) = &mut domain.count {
                    substitute_expr(c, types, values, source)?;
                }
                if let Some(s) = &mut domain.selection {
                    substitute_expr(s, types, values, source)?;
                }
                substitute_block(body, types, values, source)?;
            }
            _ => {}
        }
    }
    Ok(())
}
