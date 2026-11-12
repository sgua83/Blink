//! C函数签名只持有原TypeName名义引用；对象布局与实际C调用仍由World产生。
use crate::{ast::*, ir::{CFunctionType, CallParameter, CallParameterKind, Scalar, Ty}, Source, World};
use std::collections::{BTreeMap, BTreeSet};

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
    pub(crate) fn check_interface_types(&self, definition: &crate::Definition,
        bindings: &std::sync::Arc<crate::generics::Bindings>,
    ) -> crate::Result<()> {
        let Some(declaration) = &definition.interface_declaration else { return Ok(()); };
        let actual = &definition.function;
        let interface = &self.interfaces[&definition.module];
        let bind = |name: &crate::syntax::Type, source: &Source, span: Span| -> crate::Result<TypeName> {
            let name=bindings.type_name(self,name,(source,&definition.module,&definition.imports),span)?;
            self.type_reference(&name, source, span, &definition.module, &definition.imports)
        };
        let mut same = bind(&declaration.result(), &interface.source, declaration.span())?
            == bind(&actual.result(), &definition.source, actual.span())?;
        for (declared, implemented) in declaration.params().iter().zip(actual.params()) {
            same &= bind(&declared.ty(), &interface.source, declared.span())?
                == bind(&implemented.ty(), &definition.source, implemented.span())?;
        }
        if !same {
            return definition.source.fail(actual.span(), "B028", format!("{} 的实际类型或函数值合同与 .h 不一致", actual.name()));
        }
        Ok(())
    }
    // Resolve names and const arguments without asking a signature reference for its record layout.
    /// Contract expressions project nominal fields before requesting a layout;
    /// a callback signature may refer to the containing record itself.
    pub(crate) fn output_path_type(&self, name: &TypeName, fields: &[&str], source: &Source,
        span: Span, module: &str, imports: &[String],
    ) -> crate::Result<Ty> {
        if name.name == "cfn" && !name.dataset && !name.view && !name.tile {
            return source.fail(span, if fields.is_empty() { "B148" } else { "B225" },
                "函数签名没有数值运算或字段；原生地址通过function_address显式取得");
        }
        let resolved = self.type_reference(name, source, span, module, imports)?;
        let name = &resolved;
        if let Some((field, rest)) = fields.split_first() {
            if *field == "length" && (name.dataset || name.name == "array") {
                return if rest.is_empty() { Ok(Ty::scalar(Scalar::U32)) }
                    else { source.fail(span, "B225", "长度值没有后续字段") };
            }
            if let Some(ty) = Ty::builtin(&name.name) {
                let mut ty = ty;
                for field in fields { ty = crate::check::member_type(&ty, field, source, span)?.0; }
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
            let values = record.values().iter().zip(&name.arguments[record.types().len()..]).map(|(parameter, argument)| {
                let shape = argument.shape().unwrap();
                Ok((parameter.name().to_owned(), self.shape(&Some(shape), source, span, module, imports)?.unwrap()))
            }).collect::<crate::Result<_>>()?;
            let bindings=std::sync::Arc::new(crate::generics::Bindings{types,values});
            let selected=bindings.type_name(self,&selected,(record_source,record_module,record_imports),span)?;
            return self.output_path_type(&selected, rest, record_source, span, record_module, record_imports);
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
        self.ty(name, source, span, module, imports)
    }
    pub(crate) fn c_function_reference(
        &self, name: &TypeName, source: &Source, span: Span, module: &str, imports: &[String],
    ) -> crate::Result<CFunctionType> {
        let Some((result, parameters)) = name.arguments.split_first() else {
            return source.fail(span, "B263", "cfn<R,P...>至少需要一个返回类型");
        };
        let GenericArgument::Type(result) = result else { return source.fail(span, "B263", "cfn签名只接受类型，不接受const数值"); };
        let parameters = parameters.iter().map(|parameter| {
            let GenericArgument::Type(parameter) = parameter else { return source.fail(span, "B263", "cfn签名只接受类型，不接受const数值"); };
            let access = if !parameter.dataset && !parameter.view && !parameter.tile {
                match parameter.name.as_str() { "read" => Access::Read, "write" => Access::Write, "inout" => Access::InOut, _ => Access::Value }
            } else { Access::Value };
            let parameter = if access == Access::Value { parameter } else {
                let [GenericArgument::Type(element)] = parameter.arguments.as_slice() else {
                    return source.fail(span, "B263", "cfn位置参数使用read<T>／write<T>／inout<T>");
                };
                element
            };
            Ok((access, parameter))
        });
        self.bind_c_function_reference(
            result, parameters, name.contracts.as_ref().map_or(&[], |value| value.as_slice()),
            (source, span, module, imports),
        )
    }

    pub(crate) fn bind_c_function_reference<'a>(
        &self, result: &TypeName,
        parameters: impl IntoIterator<Item = crate::Result<(Access, &'a TypeName)>>,
        outputs: &[ContractPredicate],
        (source, span, module, imports): (&Source, Span, &str, &[String]),
    ) -> crate::Result<CFunctionType> {
        let result = self.type_reference(result, source, span, module, imports)?;
        let parameters = parameters.into_iter().map(|parameter| {
            let (access, ty) = parameter?;
            Ok((access, self.type_reference(ty, source, span, module, imports)?))
        }).collect::<crate::Result<Vec<_>>>()?;
        let contracts = crate::check::outputs::bind_function_contracts(
            self, &result, &parameters, outputs, source, span, module, imports,
        )?;
        Ok(CFunctionType { result, parameters, contracts })
    }
    pub(crate) fn c_function_signature(&self, function: &CFunctionType, source: &Source, span: Span, module: &str, imports: &[String]) -> crate::Result<(Vec<CallParameter>, Option<Ty>, bool)> {
        let mut parameters = Vec::new();
        for (access, syntax) in &function.parameters {
            let ty = self.ty_inner(syntax, source, span, module, imports, &mut BTreeSet::new())?;
            ordinary(&ty, syntax, source, span)?;
            parameters.push(if *access == Access::Value { CallParameter::value(ty) }
                else { CallParameter::reference(ty, *access) });
        }
        let never = function.result.is_never();
        let result = if never || function.result == TypeName::named("void") { None } else {
            let ty = self.ty_inner(&function.result, source, span, module, imports, &mut BTreeSet::new())?;
            ordinary(&ty, &function.result, source, span)?;
            Some(ty)
        };
        Ok((parameters, result, never))
    }

}
