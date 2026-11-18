//! B选择并绑定C函数签名；名义引用与实际C布局沿原World和原容器交接。
mod c_function;
pub(crate) use c_function::named_signature;
use crate::{ast::*, ir::{CFunctionType, CallParameter, Ty}, Source, World};

impl World {
    pub(crate) async fn check_interface_types(&self, definition: &crate::Definition,
        bindings: &std::sync::Arc<crate::generics::Bindings>,
    ) -> crate::Result<()> {
        let Some(declaration) = &definition.interface_declaration else { return Ok(()); };
        let actual = &definition.function;
        let interface = &self.interfaces[&definition.module];
        let bind = async |name: &crate::syntax::Type, source: &Source, span: Span| -> crate::Result<TypeName> {
            let name=unsafe{self.call(bindings.type_name(self,name,(source,&definition.module,&definition.imports),span)).await}?;
            unsafe{self.call(self.type_reference(&name,source,span,&definition.module,&definition.imports)).await}
        };
        let mut same = unsafe{self.call(bind(&declaration.result(),&interface.source,declaration.span())).await}?
            == unsafe{self.call(bind(&actual.result(),&definition.source,actual.span())).await}?;
        for (declared, implemented) in declaration.params().iter().zip(actual.params()) {
            same &= unsafe{self.call(bind(&declared.ty(),&interface.source,declared.span())).await}?
                == unsafe{self.call(bind(&implemented.ty(),&definition.source,implemented.span())).await}?;
        }
        if !same {
            return definition.source.fail(actual.span(), "B028", format!("{} 的实际类型或函数值合同与 .h 不一致", actual.name()));
        }
        Ok(())
    }
    pub(crate) async fn c_function_reference(
        &self,name:&TypeName,source:&Source,span:Span,module:&str,imports:&[String],
    )->crate::Result<CFunctionType> {
        unsafe{self.call(c_function::reference(self,name,source,span,module,imports)).await}
    }
    pub(crate) async fn bind_c_function_reference(
        &self,result:TypeName,parameters:Vec<(Access,TypeName)>,outputs:&[ContractPredicate],
        context:(&Source,Span,&str,&[String]),
    )->crate::Result<CFunctionType> {
        unsafe{self.call(c_function::declaration(self,result,parameters,outputs,context)).await}
    }
    pub(crate) async fn c_function_signature(&self,function:&CFunctionType,source:&Source,span:Span,module:&str,imports:&[String])
        ->crate::Result<(Vec<CallParameter>,Option<Ty>,bool)> {
        unsafe{self.call(c_function::signature(self,function,source,span,module,imports)).await}
    }
}
