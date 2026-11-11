//! 类型集合属于编译期模块定义；按完整名义类型解析并去重，不生成运行时类型对象。
use crate::{World, ast::*, ir::Ty};
use std::collections::{BTreeMap, BTreeSet};

use crate::syntax::FamilyKind;

impl World {
    pub(crate) fn family_accepts(
        &self,
        family: &Option<crate::syntax::TypeFamily>,
        actual: &Ty,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<bool> {
        let Some(family) = family else { return Ok(true); };
        match family.kind() {
            FamilyKind::Named(name) => Ok(self.named_family_members(name, source, span, module, imports, stack)?
                .contains_key(&actual.name)),
            _ => Ok(family.builtin_names().iter().any(|name| name == actual.name)),
        }
    }

    pub(crate) fn family_arguments(
        &self,
        family: &crate::syntax::TypeFamily,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<Vec<TypeName>> {
        match family.kind() {
            FamilyKind::Named(name) => Ok(self.named_family_members(name, source, span, module, imports, stack)?
                .into_values().collect()),
            _ => Ok(family.builtin_names().iter().map(TypeName::named).collect()),
        }
    }

    fn named_family_members(
        &self,
        name: &str,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<BTreeMap<String, TypeName>> {
        if let Some(inputs) = &self.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::TypeSetName {
                name: name.into(), module: module.into(), imports: imports.to_vec(),
            });
        }
        let key = self.declarations.resolve_type_set(name, module, imports)
            .ok_or_else(|| Diagnostic {
                source: source.clone(),
                span,
                code: "B233",
                message: format!("未知或不明确的类型集合 {name}"),
            })?;
        self.set_members(&key, stack)
    }

    pub(crate) fn set_members(
        &self,
        key: &str,
        stack: &mut BTreeSet<String>,
    ) -> Result<BTreeMap<String, TypeName>> {
        if let Some(inputs) = &self.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::TypeSet(key.into()));
        }
        let (set, source, module, imports) = &self.type_sets[key];
        if set.members().is_empty() {
            return source.fail(set.span(), "B233", "类型集合至少需要一个具体成员类型");
        }
        let identity = format!("typeset {key}");
        if !stack.insert(identity.clone()) {
            return source.fail(
                set.span(),
                "B233",
                format!("类型集合与记录约束递归引用 {key}"),
            );
        }
        let mut members = BTreeMap::new();
        let bindings=std::sync::Arc::new(crate::generics::Bindings::default());
        for member in &set.members() {
            if member.dataset() || member.view() || member.tile() {
                return source.fail(
                    set.span(),
                    "B233",
                    "类型集合成员必须是普通值类型，不能直接是资源包装",
                );
            }
            let member=bindings.type_name(self,&member,(source,module,imports),set.span())?;
            let ty = self.ty_inner(&member, source, set.span(), module, imports, stack)?;
            let name = ty.syntax();
            members.insert(ty.name, name);
        }
        stack.remove(&identity);
        Ok(members)
    }
}

impl crate::Declarations {
    pub(crate) fn resolve_type_set(&self, name: &str, module: &str, imports: &[String]) -> Option<String> {
        crate::names::resolve(name,module,imports,|name| {
            self.type_sets.get_key_value(name).map(|(key,(_,source,owner,_))|crate::names::Declaration {
                name:key,owner,public:source.path.extension().is_some_and(|extension|extension=="h"),
            })
        }).unique()
    }
}
