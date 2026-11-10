//! u32 const 参数在使用处静态实例化；有限集合继续逐项检查。
use crate::{ast::*, ir::Ty};
use std::collections::BTreeMap;

pub(crate) fn substitute_output_condition(condition:&mut OutputCondition,values:&BTreeMap<String,u32>){
    match condition{
        OutputCondition::Name(name)if name!="returned"=>if let Some(value)=values.get(name){*condition=OutputCondition::StaticU32(*value);},
        OutputCondition::Member(base,_)|OutputCondition::Unary(_,base)=>substitute_output_condition(base,values),
        OutputCondition::Binary(_,a,b)=>{substitute_output_condition(a,values);substitute_output_condition(b,values);},
        _=>{}
    }
}

pub(crate) fn validate(
    values: &crate::syntax::List<crate::syntax::wire::ValueParameterNode>,
    owner: &str,
    params: impl Iterator<Item=crate::syntax::Param>+Clone,
    source: &Source,
    span: Span,
) -> Result<()> {
    for parameter in values {
        if parameter.choices().is_some_and(<[u32]>::is_empty) {
            return source.fail(span, "B235", "const 参数至少需要一个允许的 u32 值");
        }
        if Ty::builtin(parameter.name()).is_some()
            || parameter.name() == owner
            || matches!(
                parameter.name(),
                "void" | "dataset" | "tile" | "view" | "array" | "acceleration" | "texture2d" | "texture3d"
            )
            || params.clone().any(|p| p.name() == parameter.name())
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
pub(crate) fn combinations(values: &crate::syntax::List<crate::syntax::wire::ValueParameterNode>) -> Vec<Vec<u32>> {
    let mut result = vec![Vec::new()];
    for parameter in values {
        let Some(choices) = parameter.choices() else {
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
pub(crate) fn substitute_shape(shape: &mut Shape, values: &BTreeMap<String,u32>, source:&Source)->Result<()> {
    match shape {
        Shape::Constant(name)=>if let Some(value)=values.get(name){*shape=Shape::Literal(*value);},
        Shape::Expression(expression)=>crate::generics::substitute_expr(expression,&BTreeMap::new(),values,source)?,
        Shape::Literal(_)=>{},
    }
    Ok(())
}
pub(crate) fn substitute_type(ty: &mut TypeName, values: &BTreeMap<String, u32>, source:&Source)->Result<()> {
    if let Some(outputs) = &mut ty.contracts {
        for output in outputs.iter_mut() {
            if let ContractPredicate::Initialized { condition, .. } = output {
                substitute_output_condition(condition, values);
            }
        }
    }
    if let Some(shape) = &mut ty.count {
        substitute_shape(shape, values,source)?;
    }
    for arg in &mut ty.arguments {
        substitute_argument(arg, values,source)?;
    }
    Ok(())
}
pub(crate) fn substitute_argument(arg: &mut GenericArgument, values: &BTreeMap<String, u32>, source:&Source)->Result<()> {
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
        GenericArgument::Type(ty) => substitute_type(ty, values,source)?,
        GenericArgument::Value(shape) => substitute_shape(shape, values,source)?,
    }
    Ok(())
}
