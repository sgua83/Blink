//! 普通用户拥有值以静态自由函数释放；不登记或复制外部资源。
use super::*;
use returns::Returned;
use owned_values::{OwnedValue,OwnedValuePayload};
use application::Ownership;
impl Builder<'_> {
    pub(super) fn user_owned_function<'a>(definition:&Definition,function:&'a Expr)->Result<&'a str>{
        match &function.kind{ExprKind::Name(name)=>Ok(name),ExprKind::FunctionRef(name,explicit)if explicit.is_empty()=>Ok(name),_=>definition.source.fail(function.span,"B224","释放函数使用静态函数名称，其类型从载荷推导")}
    }
    pub(super) fn user_owned_type(world:&World,definition:&Definition,payload:&Ty,function:&str,span:Span)->Result<Ty>{
        let mut syntax=TypeName::named("owned");syntax.arguments=vec![GenericArgument::Type(payload.syntax()),GenericArgument::Type(TypeName::named(function))];
        world.ty(&syntax,&definition.source,span,&definition.module,&definition.imports)
    }
    pub(super) fn inside_user_owner(&self,expr:&Expr)->bool {
        match &expr.kind {
            ExprKind::Member(base,_)=>self.owned_element_type(base).is_some_and(|ty|ty.cleanup.is_some())||self.inside_user_owner(base),
            ExprKind::Index(base,_)=>self.inside_user_owner(base),_=>false,
        }
    }
    pub(super) fn require_user_owner(&mut self,owner:&mut OwnedValue,span:Span)->Result<()> {
        if owner.value.ty.cleanup.is_some(){
            owner.ownership=self.resolve_ownership(&owner.ownership,None);
            if !owner.ownership.available(){return self.fail(span,"B227","用户拥有值已经移动或尚未初始化，不能访问其载荷");}
            self.prepare_owned_read(&Returned::OwnedValue(owner.clone()));
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
                    self.require_user_owner(&mut owner,expr.span)?;
                    self.unpack_record(&owner,index)
                },
                Returned::Value(value)=>Returned::value(self.record_component(&value,index)),_=>return Ok(()),
            };
        }
        Ok(())
    }
    #[inline(never)]
    pub(super) fn make_user_owned(&mut self,args:&[Expr],span:Span)->Result<Option<Returned>>{
        if args.len()!=2{return self.fail(span,"B157","own(value,F)需要实际载荷和静态释放函数");}
        if self.application.is_none()||self.program.target!=Target::Cpu{return self.fail(span,"B224","用户拥有值由普通CPU应用组织");}
        let function=Self::user_owned_function(&self.definition,&args[1])?;
        let Some(payload)=self.evaluate_returned(&args[0],None)?else{return Ok(None)};
        let value=self.returned_storage(&payload);
        let ty=Self::user_owned_type(self.world,&self.definition,&value.ty,function,span)?;
        let value=self.pack_record(ty,vec![value],span)?;
        let payloads=vec![value.ty.arguments[0].owns_resources().then(||Box::new(payload))];
        Ok(Some(Returned::OwnedValue(Box::new(OwnedValuePayload{value,payloads,ownership:Ownership::Owned,place:None,declaration:0}))))
    }
    #[inline(never)]
    pub(super) fn into_user_inner(&mut self,args:&[Expr],span:Span)->Result<Option<Returned>>{
        if args.len()!=1{return self.fail(span,"B157","into_inner需要一个实际拥有包装");}
        let Some(value)=self.evaluate_returned(&args[0],None)?else{return Ok(None)};
        let Returned::OwnedValue(value)=value else{return self.fail(span,"B224","into_inner需要实际用户拥有包装");};
        if value.value.ty.cleanup.is_none(){return self.fail(span,"B224","into_inner只解除用户拥有包装");}
        Ok(Some(self.unpack_record(&value,0)))
    }
    pub(super) fn drop_user_owned(&mut self,mut value:OwnedValue,retained:&view::Borrow,span:Span)->Result<()>{
        self.load_owned_value(&mut value,span)?;let payload=self.unpack_record(&value,0);
        let resources=match &payload{Returned::Dataset(data)=>vec![data],Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
        for data in resources{self.application_movable(data,retained,span)?;}
        let argument=cooperative::Argument::resource(payload);
        let instance=self.specialized_function(value.value.ty.cleanup.as_ref().unwrap().clone(),std::slice::from_ref(&argument),span)?;
        let origin=std::sync::Arc::new(ir::GeneratedOrigin{reason:ir::GenerationReason::UserOwnedDrop,trigger:self.source_site.clone().map(ir::GenerationAnchor::Syntax)});
        self.invoke_returned(instance,vec![argument],span,Some(origin),None)?;Ok(())
    }
}

// 释放调用是作用域内的真实后续消费者；只复用原Proof及实际拥有条件。
pub(super) fn cleanup_borrow(builder:&Builder<'_>,binding:&Binding,omitted:&BTreeSet<usize>)->Result<view::Borrow> {
    let mut result=view::Borrow::default();
    match binding{
        Binding::OwnedValue(value)=>value.cleanup_borrow(builder,omitted,&mut result)?,
        Binding::ApplicationData(value)=>data_contents(builder,value,&mut result)?,
        _=>{},
    }
    Ok(result)
}

fn data_contents(builder:&Builder<'_>,data:&application::Data,into:&mut view::Borrow)->Result<()>{
    if !data.ownership.may_own()||data.length.maximum()==0{return Ok(());}
    let mut borrowed=view::Borrow::default();
    match data.elements.known(){
        Some(Returned::OwnedValue(value))=>value.cleanup_borrow(builder,&BTreeSet::new(),&mut borrowed)?,
        Some(Returned::Dataset(child))=>data_contents(builder,child,&mut borrowed)?,
        Some(Returned::Value(_)|Returned::Unit)=>{},
        None if builder.world.stored_borrows(&data.ty,&builder.definition.source,builder.definition.function.span,&builder.definition.module,&builder.definition.imports,&mut BTreeSet::new(),true)?=>borrowed=view::Borrow::source(view::Source::Unresolved),
        None=>{},
    }
    let borrowed=if let Ownership::Conditional(when)=&data.ownership{borrowed.restricted(&when.condition().assume(&bounds::Clause::default(),true))}else{borrowed};
    into.union(&borrowed);
    Ok(())
}

impl OwnedValuePayload {
    pub(super) fn cleanup_views(&self,omitted:&BTreeSet<usize>,into:&mut view::Borrow,map:&impl Fn(usize,view::Borrow)->view::Borrow){
        fn visit(value:&OwnedValuePayload,root:&Value,offset:u32,path:&bounds::Clause,omitted:&BTreeSet<usize>,into:&mut view::Borrow,map:&impl Fn(usize,view::Borrow)->view::Borrow){
            if !value.ownership.may_own()||!value.value.ty.contains_cleanup(){return;}
            let path=if let Ownership::Conditional(when)=&value.ownership{when.condition().assume(path,true)}else{path.clone()};
            if value.value.ty.cleanup.is_some(){
                for component in view_contracts::components(&value.value.ty){
                    let at=offset+value.value.ty.components[component].1;let index=root.ty.components.iter().position(|(_,word)|*word==at).unwrap();
                    if !omitted.contains(&index){into.union(&map(index,root.proof.view_borrow(index).restricted(&path)));}
                }
                return;
            }
            for(index,payload)in value.payloads.iter().enumerate(){if let Some(Returned::OwnedValue(child))=payload.as_deref(){
                let at=if value.value.ty.has_variants(){ir::align_up(4,value.value.ty.align)}else{value.value.ty.aggregate_part(index).0};
                visit(child,root,offset+at,&path,omitted,into,map);
            }}
        }
        let value=self;
        if !value.ownership.may_own(){return;}
        if value.value.ty.contains_borrows(){visit(value,&value.value,0,&bounds::Clause::default(),omitted,into,map);}
    }
    pub(super) fn cleanup_borrow(&self,builder:&Builder<'_>,omitted:&BTreeSet<usize>,into:&mut view::Borrow)->Result<()>{
        let value=self;
        if !value.ownership.may_own(){return Ok(());}
        value.cleanup_views(omitted,into,&|_,borrow|borrow);
        fn children(builder:&Builder<'_>,value:&OwnedValuePayload,into:&mut view::Borrow)->Result<()>{
            if !value.ownership.may_own(){return Ok(());}
            for payload in value.payloads.iter().flatten(){match payload.as_ref(){
                Returned::Dataset(child)=>data_contents(builder,child,into)?,
                Returned::OwnedValue(child)=>children(builder,child,into)?,
                Returned::Value(_)|Returned::Unit=>{},
            }}
            Ok(())
        }
        let mut nested=view::Borrow::default();children(builder,value,&mut nested)?;
        let nested=if let Ownership::Conditional(when)=&value.ownership{nested.restricted(&when.condition().assume(&bounds::Clause::default(),true))}else{nested};
        into.union(&nested);
        Ok(())
    }
}
