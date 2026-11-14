//! 从值、位置及资源的名义类型参数推导；原访问权限与资源所有权保持。
use super::*;
use cooperative::Argument;
pub(crate) struct Actual<'a> {
    pub ty: Option<std::borrow::Cow<'a,Ty>>,
    pub count: Option<u32>,
}
impl<'a> Actual<'a> {
    pub fn dataset(ty:&Ty,count:Option<u32>)->Self{Self{ty:Some(std::borrow::Cow::Owned(Ty::owned_dataset(ty.syntax(),count))),count}}

    pub fn value(ty: &'a Ty) -> Self {
        Self {
            ty: Some(std::borrow::Cow::Borrowed(ty)),
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
    #[inline(never)]
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
                Argument::OwnedValue(v) | Argument::OwnedReference{value:v,..} => Actual::value(&v.value.ty),
                Argument::Value(v) => Actual::value(&v.ty),
                Argument::Dataset(v) => Actual {
                    ty: Some(std::borrow::Cow::Borrowed(self.dataset_layout(v).1)),
                    count: self.program.buffers[v.binding].count,
                },
                Argument::BorrowedData(reference) => Actual::dataset(reference.element_layout().1,reference.count),
                Argument::ApplicationData(data) => Actual::dataset(&data.ty,data.count),
                Argument::Scoped{value,..}=>match value {returns::Returned::Dataset(data)=>Actual::dataset(&data.ty,data.count),returns::Returned::OwnedValue(value)=>Actual::value(&value.value.ty),returns::Returned::Value(value)=>Actual::value(&value.ty),returns::Returned::Unit=>unreachable!()},
                Argument::Tile(v) => Actual::value(&v.ty),
                Argument::Atomic(p) => Actual::value(p.ty()),
                Argument::Reference(owner) => Actual::value(owner.place.ty()),
            })
            .collect();
        specialize(self.world, &key, &types, explicit, (self.scope.source(), self.scope.module(), self.scope.imports()), span)
    }
    pub(super) fn specialized_call(
        &mut self,
        name: &str,
        explicit: &[GenericArgument],
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        if matches!(name, "Ok" | "Err") {
            return self.explicit_result(name, explicit, args, span);
        }
        if matches!(name, "array" | "owned")
            || !self
                .world
                .resolve(
                    name,
                    self.scope.module(),
                    self.scope.imports(),
                    false,
                )
                .is_missing()
        {
            let mut ty = TypeName::named(name);
            ty.arguments = explicit.to_vec();
            return self.record_expression(&ty, args, span);
        }
        let key = self.world.resolve(
            name, self.scope.module(), self.scope.imports(), true,
        ).unique();
        if key.is_none() && matches!(name, "size_of" | "align_of") {
            let ([GenericArgument::Type(syntax)], 0) = (explicit, args.len()) else {
                return self.fail(span, "B153", format!("{name}<T>()需要一个明确类型且不接收值实参"));
            };
            let ty = self.expected_storage(syntax, span)?;
            let bytes = if name == "size_of" { ty.size } else { ty.align };
            return Ok(Some(self.constant(Scalar::U64, Bits::from(bytes))));
        }
        let key = key.ok_or_else(|| Diagnostic {
                source: self.scope.source().clone(),
                span,
                code: "B158",
                message: format!("未知或未导出的泛型函数 {name}"),
            })?;
        let Some(arguments) = self.function_arguments(&key, args, span)? else {
            return Ok(None);
        };
        let instance = self.specialized_arguments(key, &arguments, Some(explicit), span)?;
        self.invoke_instance(instance, arguments, span, Some(args))
    }
}
/// 类型与静态 u32 值共用原名义实例；显式调用可给全部实参或只给 const 后缀。
pub(crate) use crate::generics::specialize;
