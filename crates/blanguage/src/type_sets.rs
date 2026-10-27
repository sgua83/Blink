//! 类型集合属于编译期模块定义；按完整名义类型解析并去重，不生成运行时类型对象。
use crate::{World, ast::*, ir::Ty, resolve_name};
use std::collections::{BTreeMap, BTreeSet};

impl TypeFamily {
    pub(crate) fn builtin_members(&self) -> Option<&'static [&'static str]> {
        Some(match self {
            Self::Floating => &["f32", "float2", "float3", "float4"],
            Self::Integer => &[
                "u32", "uint2", "uint3", "uint4", "i32", "int2", "int3", "int4",
            ],
            Self::Numeric => &[
                "f32", "float2", "float3", "float4", "u32", "uint2", "uint3", "uint4", "i32",
                "int2", "int3", "int4",
            ],
            Self::ScalarNumeric => &["f32", "u32", "i32"],
            Self::ScalarInteger => &["u32", "i32"],
            Self::Named(_) => return None,
        })
    }
}
impl World {
    pub(crate) fn family_accepts(
        &self,
        family: &Option<TypeFamily>,
        actual: &Ty,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<bool> {
        match family {
            None => Ok(true),
            Some(family) => Ok(self
                .family_members(family, source, span, module, imports, stack)?
                .iter()
                .any(|(_, member)| member.name == actual.name)),
        }
    }
    pub(crate) fn family_members(
        &self,
        family: &TypeFamily,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<Vec<(TypeName, Ty)>> {
        if let TypeFamily::Named(name) = family {
            let key = resolve_name(name, module, imports, |key| {
                self.type_sets.get(key).is_some_and(|(_, source, owner, _)| {
                    crate::type_visible(source, owner, module, imports)
                })
            })
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B233",
                message: format!("未知或不明确的类型集合 {name}"),
            })?;
            self.set_members(&key, stack)
        } else {
            Ok(family
                .builtin_members()
                .unwrap()
                .iter()
                .map(|name| (TypeName::named(*name), Ty::builtin(name).unwrap()))
                .collect())
        }
    }
    pub(crate) fn set_members(
        &self,
        key: &str,
        stack: &mut BTreeSet<String>,
    ) -> Result<Vec<(TypeName, Ty)>> {
        let (set, source, module, imports) = &self.type_sets[key];
        if set.members.is_empty() {
            return source.fail(set.span, "B233", "类型集合至少需要一个具体成员类型");
        }
        let identity = format!("typeset {key}");
        if !stack.insert(identity.clone()) {
            return source.fail(
                set.span,
                "B233",
                format!("类型集合与记录约束递归引用 {key}"),
            );
        }
        let mut members = BTreeMap::new();
        for member in &set.members {
            if member.dataset || member.view || member.tile {
                return source.fail(
                    set.span,
                    "B233",
                    "类型集合成员必须是普通值类型，不能直接是资源包装",
                );
            }
            let ty = self.ty_inner(member, source, set.span, module, imports, stack)?;
            let name = ty.syntax();
            members.insert(ty.name.clone(), (name, ty));
        }
        stack.remove(&identity);
        Ok(members.into_values().collect())
    }
}
