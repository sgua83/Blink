//! 类型集合属于编译期模块定义；按完整名义类型解析并去重，不生成运行时类型对象。
use crate::{World, ast::*, generics::TypeArgument, ir::Ty};
use crate::syntax::{FamilyKind, wire};
use std::collections::{BTreeMap, BTreeSet};

#[repr(u32)]
enum NamedSetUse { Validate, Match, Enumerate }

#[allow(dead_code)] // The native B cursor produces these actions.
#[repr(u32)]
enum NamedSetNeed { Empty, Enter, Recursive, Wrapped, Bind, Done }

#[repr(C)]
struct NamedSetCursor {
    storage: wire::StorageView,
    members: wire::Slice,
    next: u64,
    usage: NamedSetUse,
    phase: u32,
    matched: u32,
}

#[repr(C)]
struct NamedSetStep { need: NamedSetNeed, matched: u32, member: wire::TypeRef }

#[derive(Clone, Copy)]
#[repr(C)]
struct Name { bytes: *const u8, length: u64 }

impl Name {
    fn of(value: &str) -> Self { Self { bytes: value.as_ptr(), length: value.len() as u64 } }
}

unsafe extern "C" {
    #[link_name = "bootstrap.type_families::named_set_next"]
    fn named_set_next(cursor: &mut NamedSetCursor, entered: u32) -> NamedSetStep;
    #[link_name = "bootstrap.type_families::named_set_member"]
    fn named_set_member(cursor: &mut NamedSetCursor, expected: Name, actual: Name) -> u32;
}

enum FamilyUse<'a> {
    Validate,
    Match(&'a str),
    Enumerate(&'a mut BTreeMap<String, TypeName>),
}


impl World {
    pub(crate) async fn family_accepts(
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
            FamilyKind::Named(name) => self.named_family(name, (source, span, module, imports), stack,
                FamilyUse::Match(&actual.name)).await,
            _ => Ok(family.contains_builtin(&actual.name)),
        }
    }

    pub(crate) async fn family_arguments(
        &self,
        family: &crate::syntax::TypeFamily,
        source: &Source,
        span: Span,
        module: &str,
        imports: &[String],
        stack: &mut BTreeSet<String>,
    ) -> Result<Vec<TypeArgument>> {
        match family.kind() {
            FamilyKind::Named(name) => {
                let mut members = BTreeMap::new();
                self.named_family(name, (source, span, module, imports), stack, FamilyUse::Enumerate(&mut members)).await?;
                Ok(members.into_iter().map(|(name, syntax)| TypeArgument { name, syntax }).collect())
            }
            _ => Ok(family.builtin_names().iter().map(|name| TypeArgument {
                name: name.to_owned(), syntax: TypeName::named(name),
            }).collect()),
        }
    }

    pub(crate) async fn validate_named_family(
        &self, name: &str, context: (&Source, Span, &str, &[String]), stack: &mut BTreeSet<String>,
    ) -> Result<()> {
        self.named_family(name, context, stack, FamilyUse::Validate).await.map(|_| ())
    }

    pub(crate) async fn validate_type_set(&self, key: &str, stack: &mut BTreeSet<String>) -> Result<()> {
        unsafe { self.call(self.named_type_set(key, stack, FamilyUse::Validate)).await }.map(|_| ())
    }

    async fn named_family(
        &self,
        name: &str,
        (source, span, module, imports): (&Source, Span, &str, &[String]),
        stack: &mut BTreeSet<String>,
        usage: FamilyUse<'_>,
    ) -> Result<bool> {
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
        unsafe { self.call(self.named_type_set(&key, stack, usage)).await }
    }

    async fn named_type_set(
        &self, key: &str, stack: &mut BTreeSet<String>, mut usage: FamilyUse<'_>,
    ) -> Result<bool> {
        if let Some(inputs) = &self.build_declarations {
            inputs.borrow_mut().insert(crate::build::Declaration::TypeSet(key.into()));
        }
        let (set, source, module, imports) = &self.type_sets[key];
        let span = set.span();
        let members = set.members();
        let (mode, expected) = match &usage {
            FamilyUse::Validate => (NamedSetUse::Validate, ""),
            FamilyUse::Match(name) => (NamedSetUse::Match, *name),
            FamilyUse::Enumerate(_) => (NamedSetUse::Enumerate, ""),
        };
        let expected = Name::of(expected);
        let mut cursor = NamedSetCursor { storage: members.source_view().storage(), members: members.source_slice(),
            next: 0, usage: mode, phase: 0, matched: 0 };
        let mut identity = String::new();
        let mut entered = 0;
        let mut bindings = None;
        loop {
            let step = unsafe { named_set_next(&mut cursor, entered) };
            match step.need {
                NamedSetNeed::Empty => return source.fail(span, "B233", "类型集合至少需要一个具体成员类型"),
                NamedSetNeed::Enter => {
                    identity = format!("typeset {key}");
                    entered = u32::from(stack.insert(identity.clone()));
                }
                NamedSetNeed::Recursive => return source.fail(span, "B233", format!("类型集合与记录约束递归引用 {key}")),
                NamedSetNeed::Wrapped => return source.fail(span, "B233", "类型集合成员必须是普通值类型，不能直接是资源包装"),
                NamedSetNeed::Bind => {
                    let member = set.child(step.member.offset);
                    let bindings = bindings.get_or_insert_with(|| std::sync::Arc::new(crate::generics::Bindings::default()));
                    let member = unsafe { self.call(bindings.type_name(self, &member, (source, module, imports), span)).await }?;
                    let ty = unsafe { self.call(self.ty_inner(&member, source, span, module, imports, stack)).await }?;
                    if unsafe { named_set_member(&mut cursor, expected, Name::of(&ty.name)) } != 0 {
                        let FamilyUse::Enumerate(output) = &mut usage else { unreachable!() };
                        let syntax = ty.syntax();
                        output.insert(ty.name, syntax);
                    }
                }
                NamedSetNeed::Done => {
                    stack.remove(&identity);
                    return Ok(step.matched != 0);
                }
            }
        }
    }
}

impl crate::Declarations {
    pub(crate) fn resolve_type_set(&self, name: &str, module: &str, imports: &[String]) -> Option<String> {
        self.type_sets.resolve(name, module, imports).unique()
    }
}
