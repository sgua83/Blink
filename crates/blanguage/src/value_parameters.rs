//! u32 const 参数在使用处静态实例化；有限集合继续逐项检查。
use crate::ast::*;

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
        if crate::builtin_types::Builtin::recognize(parameter.name()).is_present()
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
