//! 拥有位置直接引用原资源树；静态、动态赋值和函数输出共用同一位置交接。
use super::*;
use returns::Returned;
use application::Ownership;
use owned_presence::Presence;
fn positions(position:&Position,into:&mut BTreeSet<Position>){match position{Position::Indexed(choices,_)=>for choice in choices.iter(){positions(choice,into);},position=>{into.insert(position.clone());}}}
fn storage(model:&Returned)->Option<Place>{match model{Returned::Dataset(data)=>data.place.clone(),Returned::OwnedValue(value)=>value.place(),_=>None}}
fn at<'a>(model:&'a Returned,path:&[usize])->Option<&'a application::Data>{match model{Returned::Dataset(data)if path.is_empty()=>Some(data),Returned::OwnedValue(value)=>view::owned_data_at(value,path),_=>None}}
fn each_data(model:&mut Returned,path:&mut Vec<usize>,apply:&mut impl FnMut(&mut application::Data,&[usize])){match model{Returned::Dataset(data)=>apply(data,path),Returned::OwnedValue(value)=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{path.push(index);each_data(payload,path,apply);path.pop();}},_=>{}}}
fn set_storage(model:&mut Returned,place:Place){match model{Returned::Dataset(data)=>data.place=Some(place),Returned::OwnedValue(value)=>value.place=Some(place),_=>unreachable!()}}
impl Builder<'_>{
    pub(super) fn owned_place_candidates(&mut self,place:&Place)->Vec<(String,Vec<usize>,Returned)> {
        fn collect(builder:&mut Builder<'_>,model:Returned,path:&mut Vec<usize>,wanted:&Place,selected:&BTreeSet<Position>,into:&mut Vec<(Vec<usize>,Returned)>){
            if storage(&model).is_some_and(|place|place.ty()==wanted.ty()&&selected.contains(&place.owned_positions().unwrap()[0])){into.push((path.clone(),model));return;}
            let Returned::OwnedValue(value)=model else{return};if value.value.ty.has_variants(){return;}
            for index in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(index).1.owns_resources(){path.push(index);let child=builder.unpack_record(&value,index);collect(builder,child,path,wanted,selected,into);path.pop();}}
        }
        let mut selected=BTreeSet::new();positions(&place.owned_positions().unwrap()[0],&mut selected);
        let roots=self.env.iter().filter_map(|(name,binding)|match binding{Binding::OwnedValue(value)=>Some((name.clone(),Returned::OwnedValue(value.clone()))),Binding::ApplicationData(data)if data.place.is_some()=>Some((name.clone(),Returned::Dataset(data.clone()))),_=>None}).collect::<Vec<_>>();
        let mut candidates=Vec::new();for(name,root)in roots{let mut found=Vec::new();collect(self,root,&mut Vec::new(),place,&selected,&mut found);candidates.extend(found.into_iter().map(|(path,value)|(name.clone(),path,value)));}candidates
    }
    pub(super) fn owned_place_model(&mut self,place:&Place)->Returned {
        fn initialization(position:&Position,path:&[usize],candidates:&[(String,Vec<usize>,Returned)])->Position{match position{
            Position::Indexed(choices,index)=>Position::Indexed(choices.iter().map(|choice|initialization(choice,path,candidates)).collect(),*index),
            position=>{let model=&candidates.iter().find(|(_,_,model)|storage(model).unwrap().owned_positions().unwrap()[0]==*position).unwrap().2;at(model,path).and_then(|data|data.initialization.clone()).unwrap_or(Position::Initialized)}
        }}
        let candidates=self.owned_place_candidates(place);let root=place.owned_positions().unwrap()[0].clone();
        let selected=candidates.iter().map(|(_,_,value)|{let when=Presence::selected(&root,&storage(value).unwrap().owned_positions().unwrap()[0]);let mut value=value.clone();self.resolve_owned_model(&mut value,Some(&when));(when,value)}).filter(|(when,_)|self.owned_possible(when)).collect::<Vec<_>>();let mut values=selected.iter().map(|(_,value)|value.clone());let mut model=values.next().unwrap();for other in values{model.join(other);}model.select_presence(&selected);self.resolve_owned_model(&mut model,None);
each_data(&mut model,&mut Vec::new(),&mut|data,path|data.initialization=Some(initialization(&root,path,&candidates)));set_storage(&mut model,place.clone());self.resolve_owned_model(&mut model,None);model
    }
    pub(super) fn mutable_argument(&mut self,expr:&Expr,access:Access)->Result<Option<cooperative::Argument>> {
        fn location(expr:&Expr)->bool{match &expr.kind{ExprKind::Name(_)=>true,ExprKind::Member(base,_)|ExprKind::Index(base,_)=>location(base),_=>false}}
        let(place,owner)=if location(expr){let Some(place)=self.mutable_place(expr,access)?else{return Ok(None)};if !place.ty().owns_resources(){return Ok(Some(cooperative::Argument::Exclusive(place)));}(place,None)}else{
            let result=self.evaluate_returned(expr,None)?;if !self.flow.continues(){return Ok(None);}
            let Some(Returned::OwnedValue(value))=result else{return self.fail(expr.span,"B224","独占临时参数需要实际拥有聚合");};
            let value=self.owned_value_binding(value,true);let place=value.place().unwrap();let owner=self.hold_binding(Binding::OwnedValue(value));(place,Some(owner))
        };
        let Returned::OwnedValue(mut value)=self.owned_place_model(&place)else{unreachable!()};
        if value.ownership==Ownership::Borrowed{return self.fail(expr.span,"B227","read聚合不能提升为独占借用");}
        self.prepare_owned_read(&Returned::OwnedValue(value.clone()));
        if access==Access::InOut{if !value.all_available(){return self.fail(expr.span,"B227","inout聚合需要全部活动拥有资源");}self.load_owned_value(&mut value,expr.span)?;}
        Ok(Some(cooperative::Argument::OwnedReference{value,place,owner}))
    }
    pub(super) fn assign_owned_place(&mut self,place:&Place,value:Returned){
        for(name,path,old)in self.owned_place_candidates(place){
            let mut value=value.clone();let actual=storage(&old).unwrap();let when=Presence::selected(&place.owned_positions().unwrap()[0],&actual.owned_positions().unwrap()[0]);value.join_presence(old,Some(&when));set_storage(&mut value,actual);
            self.store_owned_candidate(name,&path,value);
        }
    }
    fn store_owned_candidate(&mut self,name:String,path:&[usize],value:Returned){
        fn replace(owner:&mut owned_values::OwnedValue,path:&[usize],value:Returned){let target=owner.payloads[path[0]].as_deref_mut().unwrap();if path.len()==1{*target=value;}else{let Returned::OwnedValue(child)=target else{unreachable!()};replace(child,&path[1..],value);}}
        if path.is_empty(){self.env.insert(name,match value{Returned::Dataset(data)=>Binding::ApplicationData(data),Returned::OwnedValue(value)=>Binding::OwnedValue(value),_=>unreachable!()});}else{let Some(Binding::OwnedValue(owner))=self.env.get_mut(&name)else{unreachable!()};replace(owner,path,value);}
    }
    fn move_owned_place(&mut self,place:&Place){
        for(name,path,mut old)in self.owned_place_candidates(place){let actual=storage(&old).unwrap();let when=Presence::selected(&place.owned_positions().unwrap()[0],&actual.owned_positions().unwrap()[0]);self.forget_owned_metadata(&old);old.restrict_owned(&when.not());self.store_owned_candidate(name,&path,old);}
    }
    pub(super) fn take_owned_place(&mut self,place:Place,span:Span)->Result<Returned>{
        let mut value=self.owned_place_model(&place);
        let resources=match &value{Returned::Dataset(data)=>vec![data],Returned::OwnedValue(value)=>value.resources(),_=>Vec::new()};
        for data in resources{if self.application_position_borrowed(data.handle.identity()){return self.fail(span,"B227","dataset的元素仍被游标或位置借用，不能移动拥有值");}}
        match &value{Returned::Dataset(data)=>{self.application_available(data,span)?;if data.ownership!=Ownership::Owned{return self.fail(span,"B227","借用资源不能从拥有位置移动");}},Returned::OwnedValue(value)=>{if !value.all_available(){return self.fail(span,"B227","拥有位置仍有未初始化或已经移动的资源");}},_=>unreachable!()}
        self.prepare_owned_read(&value);self.require_initialized(&place,span)?;let packed=self.read_storage_value(&place,span)?;
        self.move_owned_place(&place);
        fn handoff(value:&mut Returned,old:&Returned){match(value,old){
            (Returned::Dataset(data),Returned::Dataset(old))=>{data.initialization=old.initialization.clone();let identity=ValueIdentity::Register(data.handle.regs[0]);let mut borrow=old.handle.proof.view_borrow(0);if borrow.empty(){borrow.sources.insert(view::Source::Data(old.handle.identity()));}data.handle.proof=bounds::Proof::default().with_identity(identity).with_view_borrow(0,borrow);data.length.proof=bounds::Proof::capture(&data.length).with_domains(Domains::from([IndexDomain::ApplicationLength(identity)]));},
            (Returned::OwnedValue(value),Returned::OwnedValue(old))=>for(index,payload)in value.payloads.iter_mut().enumerate(){if let Some(payload)=payload{handoff(payload,old.payloads[index].as_deref().unwrap());}},_=>{}}}
        let old=value.clone();native_function::rebind_model(&mut value,packed.regs);handoff(&mut value,&old);Ok(value)
    }
    pub(super) fn assign_owned_storage(&mut self,place:Place,op:&str,rhs:&Expr,span:Span)->Result<()> {
        self.require_owned_writable(&place,span)?;if op!="="{return self.fail(span,"B224","拥有内容只支持整份移动赋值");}
        let result=self.evaluate_returned(rhs,Some(place.ty()))?;if !self.flow.continues(){return Ok(());}
        let mut value=result.ok_or_else(||Diagnostic{source:self.definition.source.clone(),span,code:"B228",message:"拥有赋值需要实际值或资源".into()})?;self.check_returned_type(&mut value,place.ty(),span)?;
        self.view_uses.after(span);let old=self.owned_place_model(&place);let packed=self.returned_storage(&value);
        // 覆盖结束旧元数据的生命周期；后续消费者读取实际右值的view关系。
        self.assign_owned_proof(&place,bounds::Proof::capture(&packed),packed.uniform);
        self.drop_returned(old,&view::Borrow::default(),span)?;
        self.write_storage_value(&place,&packed,span)?;self.assign_owned_place(&place,value);self.assign_owned_proof(&place,bounds::Proof::capture(&packed),packed.uniform);self.initialize(&place);Ok(())
    }
}
