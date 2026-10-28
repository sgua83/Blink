//! 独占聚合借用沿实际Place交接，拥有内容仍由原资源树描述。
use super::*;
use owned_values::OwnedValue;
use returns::Returned;
use application::Ownership;
fn positions(position:&Position,into:&mut BTreeSet<Position>){match position{Position::Indexed(choices,_)=>for choice in choices.iter(){positions(choice,into);},position=>{into.insert(position.clone());}}}
impl Builder<'_>{
    fn owned_place_candidates(&mut self,place:&Place)->Vec<(String,Vec<usize>,OwnedValue)> {
        fn collect(builder:&mut Builder<'_>,value:OwnedValue,path:&mut Vec<usize>,wanted:&Place,selected:&BTreeSet<Position>,into:&mut Vec<(Vec<usize>,OwnedValue)>){
            if value.value.ty==*wanted.ty() && value.place().is_some_and(|place|selected.contains(&place.owned_positions().unwrap()[0])){into.push((path.clone(),value));return;}
            if value.value.ty.has_variants(){return;}
            for index in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(index).1.owns_resources() && let Returned::OwnedValue(child)=builder.unpack_record(&value,index){path.push(index);collect(builder,child,path,wanted,selected,into);path.pop();}}
        }
        let mut selected=BTreeSet::new();positions(&place.owned_positions().unwrap()[0],&mut selected);
        let roots=self.env.iter().filter_map(|(name,binding)|match binding{Binding::OwnedValue(value)=>Some((name.clone(),value.clone())),_=>None}).collect::<Vec<_>>();
        let mut candidates=Vec::new();
        for(name,root)in roots {let mut found=Vec::new();collect(self,root,&mut Vec::new(),place,&selected,&mut found);candidates.extend(found.into_iter().map(|(path,value)|(name.clone(),path,value)));}
        candidates
    }
    pub(super) fn mutable_argument(&mut self,expr:&Expr,access:Access)->Result<Option<cooperative::Argument>> {
        fn location(expr:&Expr)->bool{match &expr.kind{ExprKind::Name(_)=>true,ExprKind::Member(base,_)|ExprKind::Index(base,_)=>location(base),_=>false}}
        let(place,owner)=if location(expr){let Some(place)=self.mutable_place(expr,access)?else{return Ok(None)};if !place.ty().owns_resources(){return Ok(Some(cooperative::Argument::Exclusive(place)));}(place,None)}else{
            let result=self.evaluate_returned(expr,None)?;if !self.flow.continues(){return Ok(None);}
            let Some(Returned::OwnedValue(value))=result else{return self.fail(expr.span,"B224","独占临时参数需要实际拥有聚合");};
            let value=self.owned_value_binding(value,true);let place=value.place().unwrap();let owner=self.hold_binding(Binding::OwnedValue(value));(place,Some(owner))
        };
        let mut candidates=self.owned_place_candidates(&place).into_iter();let mut value=candidates.next().unwrap().2;
        for(_,_,other)in candidates{value.join(other);}
        if value.ownership==Ownership::Borrowed{return self.fail(expr.span,"B227","read聚合不能提升为独占借用");}
        value.place=Some(place.clone());
        if access==Access::InOut {if !value.all_available(){return self.fail(expr.span,"B227","inout聚合需要全部活动拥有资源");}self.load_owned_value(&mut value,expr.span)?;}
        Ok(Some(cooperative::Argument::OwnedReference{value,place,owner}))
    }
    pub(super) fn assign_owned_reference(&mut self,place:&Place,value:OwnedValue){
        fn at<'a>(value:&'a mut OwnedValue,path:&[usize])->&'a mut OwnedValue{if path.is_empty(){return value;}let Some(Returned::OwnedValue(child))=value.payloads[path[0]].as_deref_mut()else{unreachable!()};at(child,&path[1..])}
        for(name,path,old)in self.owned_place_candidates(place){
            let mut value=value.clone();let actual=old.place().unwrap();
            if place.components_in(&actual).is_none(){value.join(old);}
            value.place=Some(actual);
            let Some(Binding::OwnedValue(root))=self.env.get_mut(&name)else{unreachable!()};*at(root,&path)=value;
        }
    }
}
