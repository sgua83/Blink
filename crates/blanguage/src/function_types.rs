//! B选择并绑定C函数签名；名义引用与实际C布局沿原World和原容器交接。
mod c_function;
pub(crate) use c_function::named_signature;
use crate::{ast::*, ir::{CFunctionType, CallParameter, Scalar, Ty}, Source, World};

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
    // Resolve names and const arguments without asking a signature reference for its record layout.
    /// Contract expressions project nominal fields before requesting a layout;
    /// a callback signature may refer to the containing record itself.
    pub(crate) async fn output_path_type(&self, name: &TypeName, fields: &[&str], source: &Source,
        span: Span, module: &str, imports: &[String],
    ) -> crate::Result<Ty> {
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return source.fail(span, if fields.is_empty() { "B148" } else { "B225" },
                "函数签名没有数值运算或字段；原生地址通过function_address显式取得");
        }
        let resolved = unsafe{self.call(self.type_reference(name,source,span,module,imports)).await}?;
        let name = &resolved;
        if let Some((field, rest)) = fields.split_first() {
            if *field == "length" && (name.dataset || name.name == "array") {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::U32)) }
                    else { source.fail(span, "B225", "长度值没有后续字段") };
            }
            if let Some(ty) = Ty::builtin(&name.name) {
                let mut ty = ty;
                for field in fields { ty = crate::check::member_type_only(&ty, field, source, span)?; }
                return Ok(ty);
            }
            if name.name == "result" && matches!(*field, "Ok" | "Err") {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::Bool)) }
                    else { source.fail(span, "B225", "变体条件没有后续字段") };
            }
            let (record, record_source, record_module, record_imports) = if name.canonical {
                self.record(&name.name)
            } else {
                let Some(key) = self.resolve(&name.name, module, imports, false).unique() else {
                    return source.fail(span, "B031", format!("未知类型 {}", name.name));
                };
                self.record(&key)
            };
            let selected = match record.contract_member(field) {
                crate::syntax::ContractMember::Field(ty) => ty,
                crate::syntax::ContractMember::Variant => {
                    return if rest.is_empty() { Ok(Ty::scalar(Scalar::Bool)) }
                        else { source.fail(span, "B225", "变体条件没有后续字段") };
                }
                crate::syntax::ContractMember::Missing => {
                    return source.fail(span, "B225", format!("类型 {} 没有字段 {field}", name.name));
                }
            };
            let types = record.types().iter().zip(&name.arguments).map(|(parameter, argument)| {
                let GenericArgument::Type(ty) = argument else {
                    return source.fail(span, "B232", "此记录参数需要类型实参");
                };
                Ok((parameter.name().to_owned(), ty.clone()))
            }).collect::<crate::Result<_>>()?;
            let mut values=std::collections::BTreeMap::new();
            for(parameter,argument)in record.values().iter().zip(&name.arguments[record.types().len()..]){
                let shape=argument.shape().unwrap();
                let value=unsafe{self.call(self.shape(&Some(shape),source,span,module,imports)).await}?.unwrap();
                values.insert(parameter.name().to_owned(),value);
            }
            let bindings=std::sync::Arc::new(crate::generics::Bindings{types,values});
            let selected=unsafe{self.call(bindings.type_name(self,&selected,(record_source,record_module,record_imports),span)).await}?;
            return unsafe{self.call(self.output_path_type(&selected,rest,record_source,span,record_module,record_imports)).await};
        }
        // A literal cannot numerically operate on an ordinary record. Decide
        // that existing type error from its declaration, without expanding a
        // recursive callback just to discover the record's machine layout.
        if let Some(key) = if name.canonical { self.records.contains_key(&name.name).then_some(name.name.clone()) }
            else { self.resolve(&name.name, module, imports, false).unique() } {
            let record = &self.record(&key).0;
            if record.enumeration().is_none() {
                return source.fail(span, "B147", "结构体和资源视图不能作为数值进行运算");
            }
        }
        unsafe{self.call(self.ty(name,source,span,module,imports)).await}
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
