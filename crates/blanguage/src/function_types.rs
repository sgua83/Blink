//! C函数地址的签名复用原类型参数；访问包装只作签名元数据，不分配存储。
use crate::{ast::*, ir::{Scalar, Ty}, Source, World};
use std::collections::BTreeSet;

fn marker(name: String, arguments: Vec<Ty>) -> Ty {
    let mut ty = Ty::scalar(Scalar::U64);
    ty.name = name;
    ty.arguments = arguments;
    ty.size = 0;
    ty.align = 1;
    ty.components.clear();
    ty
}
fn ordinary(ty: &Ty, syntax: &TypeName, source: &Source, span: Span) -> crate::Result<()> {
    if syntax.dataset || ty.owns_resources() {
        return source.fail(span, "B224", "cfn普通C签名不能交接Blink拥有资源");
    }
    if ty.contains_views() || ty.components.is_empty() {
        return source.fail(span, "B263", "cfn需要普通非空C布局，不能交接Blink资源引用");
    }
    Ok(())
}
impl World {
    pub(crate) fn c_function_type(&self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String], stack: &mut BTreeSet<String>) -> crate::Result<Ty> {
        if name.arguments.is_empty() {
            return source.fail(span, "B263", "cfn<R,P...>至少需要一个返回类型");
        }
        let mut types = Vec::new();
        for (index, argument) in name.arguments.iter().enumerate() {
            let GenericArgument::Type(argument) = argument else {
                return source.fail(span, "B263", "cfn签名只接受类型，不接受const数值");
            };
            if index == 0 && matches!(argument.name.as_str(), "void" | "never")
                && argument.arguments.is_empty() && !argument.dataset && !argument.view && !argument.tile && argument.count.is_none() {
                types.push(marker(argument.name.clone(), Vec::new()));
                continue;
            }
            if index != 0 && matches!(argument.name.as_str(), "read" | "write" | "inout")
                && !argument.dataset && !argument.view && !argument.tile {
                let [GenericArgument::Type(element)] = argument.arguments.as_slice() else {
                    return source.fail(span, "B263", "cfn位置参数使用read<T>／write<T>／inout<T>");
                };
                let ty = self.ty_inner(element, source, span, module, imports, stack)?;
                ordinary(&ty, element, source, span)?;
                types.push(marker(format!("{}<{}>", argument.name, ty.name), vec![ty]));
            } else {
                let ty = self.ty_inner(argument, source, span, module, imports, stack)?;
                ordinary(&ty, argument, source, span)?;
                types.push(ty);
            }
        }
        let mut ty = Ty::scalar(Scalar::U64);
        ty.name = format!("cfn<{}>", types.iter().map(|ty| ty.name.as_str()).collect::<Vec<_>>().join(","));
        ty.arguments = types;
        Ok(ty)
    }
}
