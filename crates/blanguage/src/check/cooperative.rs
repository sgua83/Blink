//! 函数借用现有数据集、独占／原子位置或 tile；静态绑定实际来源与访问模式。
use super::*;
pub(super) enum Argument {
    BorrowedData(application::DataReference),
    Acceleration(usize),
    Texture(texture::TextureReference),
    Value(Value),
    Dataset(DatasetView),
    ApplicationData(application::Data),
    Scoped { value: returns::Returned, owner: String },
    OwnedValue(owned_values::OwnedValue),
    OwnedReference{value:owned_values::OwnedValue,place:Place,owner:Option<String>},
    Tile(TileView),
    Atomic(Place),
    Reference(OwnedBinding),
}
impl Argument {
    pub(super) fn resource(value:returns::Returned)->Self {
        match value {returns::Returned::Dataset(data)=>Self::ApplicationData(data),returns::Returned::OwnedValue(value)=>Self::OwnedValue(value),returns::Returned::Value(value)=>Self::Value(*value),returns::Returned::Unit=>unreachable!()}
    }

}
pub(super) async fn parameter_type(world: &World, param: &Param, context: (&Source,&str,&[String],FunctionKind), target: Target) -> Result<Ty> {
    let(source,module,imports,kind)=context;
    let declared=&param.ty;
    let mut name = declared.clone();
    name.tile = false;
    let ty = unsafe { world.call(world.ty(
        &name,
        source,
        param.span,
        module,
        imports,
    )).await }?;
    match call_execution::parameter_error(param,&ty,kind,target){
        call_execution::ParameterError::None=>{},
        call_execution::ParameterError::Owned=>return source.fail(param.span,"B224","拥有聚合通过普通CPU函数按值交接或read／write／inout借用；其他访问与执行目标尚未实现"),
        call_execution::ParameterError::Tile=>return source.fail(param.span,"B207","tile 借用只能包含普通值，不能包含资源 view"),
        call_execution::ParameterError::Atomic=>return source.fail(param.span,"B223","原子借用参数不能包含资源 view"),
    }
    Ok(ty)
}
#[derive(Clone,Copy)]
pub(super) enum CallParameters<'a>{
    Declaration(&'a Definition),
    CFunction(&'a [Param]),
}
impl CallParameters<'_>{
    pub(super) fn len(self)->usize{match self{
        Self::Declaration(definition)=>definition.function.params().len(),
        Self::CFunction(parameters)=>parameters.len(),
    }}
    pub(super) fn access(self,index:usize)->Access{match self{
        Self::Declaration(definition)=>definition.function.params().at(index).access(),
        Self::CFunction(parameters)=>parameters[index].access,
    }}
    pub(super) fn form(self,index:usize)->DeclaredArgumentForm{match self{
        Self::Declaration(definition)=>declared_argument_form(&definition.function.params().at(index).ty()),
        // The cfn producer has already checked its ordinary C ABI layout.
        Self::CFunction(_)=>DeclaredArgumentForm::Ordinary,
    }}
    pub(super) async fn expected(self,world:&World,caller:&LexicalScope,index:usize)->Result<Option<Ty>>{
        match self{
            Self::Declaration(definition)=>{
                if !definition.function.types().is_empty()||!definition.function.values().is_empty(){return Ok(None);}
                let parameter=definition.function.params().at(index);
                // This declaration has no generic formals; its lexical names
                // resolve in its own module, never through the caller's bindings.
                let bindings=Arc::new(crate::generics::Bindings::default());
                let syntax=unsafe { world.call(bindings.type_name(world,&parameter.ty(),
                    (&definition.source,&definition.module,&definition.imports),parameter.span())).await }?;
                unsafe { world.call(world.ty(&syntax,&definition.source,parameter.span(),&definition.module,&definition.imports)).await }.map(Some)
            }
            Self::CFunction(parameters)=>{
                let parameter=&parameters[index];
                unsafe { world.call(world.ty(&parameter.ty,caller.source(),parameter.span,caller.module(),caller.imports())).await }.map(Some)
            }
        }
    }
    pub(super) fn declares_output(self,index:usize)->bool{
        match self{
            Self::Declaration(definition)=>{
                let parameter=definition.function.params().at(index);
                definition.function.contracts().iter().any(|contract|
                    matches!(contract.predicate().kind(),crate::syntax::PredicateKind::Initialized{value,..}
                        if value.split('.').next()==Some(parameter.name())))
            }
            Self::CFunction(_)=>false,
        }
    }
}

impl<'a> Builder<'a> {
    #[inline(never)]
    pub(super) async fn function_arguments(&mut self, key: &str, args: &Expressions, span: Span) -> Result<Option<Vec<Argument>>> {
        let callee = self.world.functions[key].clone();
        unsafe { self.world.call(self.call_arguments(CallParameters::Declaration(&callee), args, span)).await }
    }
    pub(super) async fn call_arguments(&mut self,parameters:CallParameters<'_>,args:&Expressions,span:Span)->Result<Option<Vec<Argument>>> {
        unsafe{self.world.call(call_arguments::build(self,parameters,args.len(),span,|index|args.at(index).span(),
            async |builder,index,parameter|unsafe{builder.world.call(call_arguments::source(builder,parameters,index,parameter,&args.at(index))).await})).await}
    }
    pub(super) fn check_argument_aliases(&self, parameter: impl Fn(usize)->(Access,bool), arguments: &[Argument], span: impl Fn(usize) -> Span) -> Result<()> {
        if let Some(index)=call_aliases::conflict(self,parameter,arguments){
            return self.fail(span(index),"B227","同一次调用的独占借用不能重叠，也不能与同一存储的只读或原子借用重叠");
        }
        Ok(())
    }
    pub(super) async fn invoke_instance(
        &mut self,
        instance: crate::generics::Instance,
        args: Vec<Argument>,
        span: Span,
        syntax: Option<&Expressions>,
    ) -> Result<Option<Value>> {
        let result = unsafe { self.world.call(self.invoke_returned(instance, args, span, None, syntax)).await }?;
        self.ordinary_return(result, span)
    }
}
