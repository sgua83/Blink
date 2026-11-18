//! 普通用户拥有值以静态自由函数释放；不登记或复制外部资源。
use super::*;
use returns::Returned;
use owned_values::{OwnedValue,OwnedValuePayload};
use application::Ownership;
mod cleanup;
pub(super) use cleanup::cleanup_borrow;
impl Builder<'_> {
    pub(super) fn user_owned_function<'a>(source:&Source,function:&'a Expr)->Result<&'a str>{
        match function.kind(){ExprKind::Name(name)=>Ok(name),ExprKind::FunctionRef(name,explicit)if explicit.is_empty()=>Ok(name),_=>source.fail(function.span(),"B224","释放函数使用静态函数名称，其类型从载荷推导")}
    }
    pub(super) async fn user_owned_type(world:&World,scope:&LexicalScope,payload:&Ty,function:&str,span:Span)->Result<Ty>{
        let mut syntax=TypeName::named("owned");syntax.arguments=vec![GenericArgument::Type(payload.syntax()),GenericArgument::Type(TypeName::named(function))];
        unsafe { world.call(world.ty(&syntax,scope.source(),span,scope.module(),scope.imports())).await }
    }
    pub(super) fn inside_user_owner(&self,expr:&Expr)->bool {
        match expr.kind() {
            ExprKind::Member(base,_)=>self.owned_element_type(&base).is_some_and(|ty|ty.cleanup.is_some())||self.inside_user_owner(&base),
            ExprKind::Index(base,_)=>self.inside_user_owner(&base),_=>false,
        }
    }
    pub(super) fn require_user_owner(&mut self,owner:&mut OwnedValue,span:Span)->Result<()> {
        if owner.value.ty.cleanup.is_some(){
            owner.ownership=self.resolve_ownership(&owner.ownership,None);
            if !owner.ownership.available(){return self.fail(span,"B227","用户拥有值已经移动或尚未初始化，不能访问其载荷");}
            self.prepare_owned_value(&owner);
        }
        Ok(())
    }
    pub(super) fn require_user_projection(&mut self,expr:&Expr)->Result<()> {
        let Some(projection)=self.owned_projection(expr)else{return Ok(())};
        let Some(Binding::OwnedValue(root))=self.env.get(&projection.root).cloned()else{unreachable!()};
        let mut value=Returned::OwnedValue(root);
        for index in projection.path {
            value=match value {
                Returned::OwnedValue(mut owner)=>{
                    self.require_user_owner(&mut owner,expr.span())?;
                    self.unpack_record(&owner,index)
                },
                Returned::Value(value)=>Returned::value(self.record_component(&value,index)),_=>return Ok(()),
            };
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) async fn make_user_owned(&mut self,args:&Expressions,span:Span)->Result<Option<Returned>>{
        if args.len()!=2{return self.fail(span,"B157","own(value,F)需要实际载荷和静态释放函数");}
        if self.application.is_none()||self.program.target!=Target::Cpu{return self.fail(span,"B224","用户拥有值由普通CPU应用组织");}
        let function_reference=args.at(1);
        let function=Self::user_owned_function(self.scope.source(),&function_reference)?;
        let Some(payload)=unsafe { self.world.call(self.evaluate_returned(&args.at(0),None)).await }?else{return Ok(None)};
        let value=self.returned_storage(&payload);
        let ty=unsafe { self.world.call(Self::user_owned_type(self.world,&self.scope,&value.ty,function,span)).await }?;
        let value=self.pack_record(ty,vec![value],span)?;
        let payloads=vec![value.ty.arguments[0].owns_resources().then(||Box::new(payload))];
        Ok(Some(Returned::OwnedValue(Box::new(OwnedValuePayload{value,payloads,ownership:Ownership::Owned,place:None,declaration:0}))))
    }
    #[inline(never)]
    pub(super) async fn into_user_inner(&mut self,args:&Expressions,span:Span)->Result<Option<Returned>>{
        if args.len()!=1{return self.fail(span,"B157","into_inner需要一个实际拥有包装");}
        let Some(value)=unsafe { self.world.call(self.evaluate_returned(&args.at(0),None)).await }?else{return Ok(None)};
        let Returned::OwnedValue(value)=value else{return self.fail(span,"B224","into_inner需要实际用户拥有包装");};
        if value.value.ty.cleanup.is_none(){return self.fail(span,"B224","into_inner只解除用户拥有包装");}
        Ok(Some(self.unpack_record(&value,0)))
    }


}
