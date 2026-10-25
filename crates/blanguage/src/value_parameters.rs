//! u32 const 参数在使用处静态实例化；有限集合继续逐项检查。
use crate::{ast::*, ir::Ty};
use std::collections::BTreeMap;

pub(crate) fn validate(
    values: &[ValueParameter],
    owner: &str,
    params: &[Param],
    source: &Source,
    span: Span,
) -> Result<()> {
    for parameter in values {
        if parameter.choices.as_ref().is_some_and(Vec::is_empty) {
            return source.fail(span, "B235", "const 参数至少需要一个允许的 u32 值");
        }
        if Ty::builtin(&parameter.name).is_some()
            || parameter.name == owner
            || matches!(
                parameter.name.as_str(),
                "void" | "dataset" | "tile" | "view" | "array" | "acceleration" | "texture2d"
            )
            || params.iter().any(|p| p.name == parameter.name)
        {
            return source.fail(
                span,
                "B235",
                "const 参数不能覆盖内置名称、当前声明名或普通参数",
            );
        }
    }
    Ok(())
}
pub(crate) fn combinations(values: &[ValueParameter]) -> Vec<Vec<u32>> {
    let mut result = vec![Vec::new()];
    for parameter in values {
        let Some(choices) = &parameter.choices else {
            return Vec::new();
        };
        result = result
            .into_iter()
            .flat_map(|prefix| {
                choices.iter().map(move |value| {
                    let mut args = prefix.clone();
                    args.push(*value);
                    args
                })
            })
            .collect();
    }
    result
}
pub(crate) fn substitute_shape(shape: &mut Shape, values: &BTreeMap<String, u32>) {
    if let Shape::Constant(name) = shape {
        if let Some(value) = values.get(name) {
            *shape = Shape::Literal(*value);
        }
    }
}
pub(crate) fn substitute_type(ty: &mut TypeName, values: &BTreeMap<String, u32>) {
    if let Some(shape) = &mut ty.count {
        substitute_shape(shape, values);
    }
    for arg in &mut ty.arguments {
        substitute_argument(arg, values);
    }
}
pub(crate) fn substitute_argument(arg: &mut GenericArgument, values: &BTreeMap<String, u32>) {
    match arg {
        GenericArgument::Type(ty)
            if ty.arguments.is_empty()
                && !ty.dataset
                && !ty.view
                && !ty.tile
                && values.contains_key(&ty.name) =>
        {
            *arg = GenericArgument::Value(Shape::Literal(values[&ty.name]));
        }
        GenericArgument::Type(ty) => substitute_type(ty, values),
        GenericArgument::Value(shape) => substitute_shape(shape, values),
    }
}
