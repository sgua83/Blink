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
    pub(super) async fn specialized_function(
        &self,
        key: String,
        args: &[Argument],
        span: Span,
    ) -> Result<crate::generics::Instance> {
        unsafe { self.world.call(self.specialized_arguments(key, args, None, span)).await }
    }
    #[inline(never)]
    pub(super) async fn specialized_arguments(
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
        unsafe { self.world.call(specialize(self.world, &key, &types, explicit, (self.scope.source(), self.scope.module(), self.scope.imports()), span)).await }
    }

}
/// 类型与静态 u32 值共用原名义实例；显式调用可给全部实参或只给 const 后缀。
pub(crate) use crate::generics::specialize;
