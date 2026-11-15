//! 局部拥有关系复用原条件及真实控制值；不形成运行时拥有表。
use super::*;
use application::Ownership;
use returns::Returned;
#[derive(Clone)]
pub(super) enum Presence {All,None,Input(Value),Selected(Comparison,ownership::ApplicationIndex,ownership::ApplicationIndex),Not(Box<Presence>),Both(Box<Presence>,Box<Presence>),Either(Box<Presence>,Box<Presence>)}
mod host;

impl PartialEq for Presence { fn eq(&self, other: &Self) -> bool { host::compare(self, other).is_eq() } }
impl Eq for Presence {}
impl PartialOrd for Presence { fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) } }
impl Ord for Presence { fn cmp(&self, other: &Self) -> std::cmp::Ordering { host::compare(self, other) } }
impl Presence {
    pub(super) fn uses_index(&self, index: ownership::ApplicationIndex) -> bool { host::uses_index(self, index) }
    pub(super) fn indices(operator: Comparison, a: ownership::ApplicationIndex, b: ownership::ApplicationIndex) -> Self { host::relation(operator, a, b) }
    fn loop_condition(&self, inputs: &BTreeMap<ValueIdentity,bounds::Proof>) -> bounds::Condition { host::loop_condition(self, inputs) }
    pub(super) fn input(value: &Value) -> Self { host::from_value(value) }
    pub(super) fn not(self) -> Self { host::negate(self) }
    pub(super) fn and(self, other: Self) -> Self { host::combine(self, other, false) }
    pub(super) fn or(self, other: Self) -> Self { host::combine(self, other, true) }
    pub(super) fn selected(position: &Position, wanted: &Position) -> Self { host::selected_presence(position, wanted) }
    pub(super) fn condition(&self) -> bounds::Condition { host::condition(self) }
}
impl Ownership {
    pub(super) fn may_own(&self) -> bool { host::may_own(self) }
    pub(super) fn available(&self) -> bool { host::available(self) }
    pub(super) fn presence(&self) -> Presence { host::presence(self) }
    fn from_presence(value: Presence) -> Self { host::from_presence(value) }
    pub(super) fn loop_compatible(&self, other: &Self, inputs: &BTreeMap<ValueIdentity,bounds::Proof>, path: &bounds::Clause) -> bool { host::compatible(self, other, inputs, path) }
    pub(super) fn either(a: &Self, b: &Self, choice: Option<&Presence>) -> Self { host::either(a, b, choice) }
    pub(super) fn restricted(&self, when: &Presence) -> Self { host::restricted(self, when) }
}
impl Returned{
    pub(super) fn select_presence(&mut self,candidates:&[(Presence,Returned)]){
        fn mode(values:&[(Presence,&Ownership)])->Ownership{
            if values.iter().all(|(_,value)|*value==values[0].1){return values[0].1.clone();}
            if values.iter().any(|(_,value)|**value==Ownership::Borrowed){return Ownership::Moved;}
            let present=values.iter().fold(Presence::None,|all,(when,value)|all.or(when.clone().and(value.presence())));Ownership::from_presence(present)
        }
        fn restore(target:&mut Returned,values:&[(Presence,&Returned)]){match target{
            Returned::Dataset(target)=>{let values=values.iter().filter_map(|(when,value)|if let Returned::Dataset(data)=value{Some((when.clone(),data))}else{None}).collect::<Vec<_>>();target.ownership=mode(&values.iter().map(|(when,data)|(when.clone(),&data.ownership)).collect::<Vec<_>>());target.root.proof.select_view_borrows(&values.iter().map(|(when,data)|(when.clone().and(data.ownership.presence()).condition(),view::resource_proof(data))).collect::<Vec<_>>());target.residency=residency::Residency::Absent;for(when,data)in values.iter().filter(|(_,data)|data.ownership.may_own()||data.ownership==Ownership::Borrowed){let candidate=residency::Residency::selected(when.clone(),data.residency.clone(),residency::Residency::Absent);target.residency.join(&candidate);}},
            Returned::OwnedValue(target)=>{let values=values.iter().filter_map(|(when,value)|if let Returned::OwnedValue(value)=value{Some((when.clone(),value))}else{None}).collect::<Vec<_>>();target.ownership=mode(&values.iter().map(|(when,value)|(when.clone(),&value.ownership)).collect::<Vec<_>>());target.value.proof.select_view_borrows(&values.iter().map(|(when,value)|(when.condition(),value.value.proof.borrow_state())).collect::<Vec<_>>());for(index,payload)in target.payloads.iter_mut().enumerate(){if let Some(payload)=payload{let children=values.iter().filter_map(|(when,value)|value.payloads[index].as_deref().map(|child|(when.clone(),child))).collect::<Vec<_>>();restore(payload,&children);}}},_=>{}}}
        restore(self,&candidates.iter().map(|(when,value)|(when.clone(),value)).collect::<Vec<_>>());
    }
    pub(super) fn restrict_owned(&mut self,when:&Presence){match self{Self::Dataset(data)=>data.ownership=data.ownership.restricted(when),Self::OwnedValue(value)=>value.restrict_owned(when),_=>{}}}
    pub(super) fn join_presence(&mut self,other:Self,choice:&Presence){
        match choice {
            Presence::All => return,
            Presence::None => { *self=other; return; }
            _ => {}
        }
        fn restore(target:&mut Returned,a:&Returned,b:&Returned,choice:&Presence){match(target,a,b){(Returned::Dataset(target),Returned::Dataset(a),Returned::Dataset(b))=>{target.ownership=Ownership::either(&a.ownership,&b.ownership,Some(choice));let ai=a.initialization.clone().unwrap_or(Position::Initialized);let bi=b.initialization.clone().unwrap_or(Position::Initialized);let initialization=if a.ownership==Ownership::Moved{bi}else if b.ownership==Ownership::Moved{ai}else{Position::selected(choice.clone(),ai,bi)};target.initialization=match initialization{Position::Initialized=>None,position=>Some(position)};if a.ownership==Ownership::Moved{target.residency=b.residency.clone();}else if b.ownership==Ownership::Moved{target.residency=a.residency.clone();}if let(Some(target),Some(a),Some(b))=(target.elements.known_mut(),a.elements.known(),b.elements.known()){restore(target,a,b,choice);}},(Returned::OwnedValue(target),Returned::OwnedValue(a),Returned::OwnedValue(b))=>{target.ownership=Ownership::either(&a.ownership,&b.ownership,Some(choice));for(index,payload)in target.payloads.iter_mut().enumerate(){if let(Some(target),Some(a),Some(b))=(payload.as_deref_mut(),a.payloads[index].as_deref(),b.payloads[index].as_deref()){restore(target,a,b,choice);}}},_=>{}}}
        let old=self.clone();self.join(other.clone(),false,Some(&choice.condition()));restore(self,&old,&other,choice);
    }
}
impl Builder<'_>{
    pub(super) fn resolve_owned_initialization(&self,value:&Position,assuming:Option<&Presence>)->Position{match value{
        Position::Conditional(when,a,b)=>match self.condition_truth(&when.condition(),assuming.map(Presence::condition).as_ref()){Some(true)=>self.resolve_owned_initialization(a,assuming),Some(false)=>self.resolve_owned_initialization(b,assuming),None=>{let yes=assuming.cloned().unwrap_or(Presence::All).and(when.as_ref().clone());let no=assuming.cloned().unwrap_or(Presence::All).and(when.as_ref().clone().not());Position::selected(when.as_ref().clone(),self.resolve_owned_initialization(a,Some(&yes)),self.resolve_owned_initialization(b,Some(&no))) }},
        Position::Both(a,b)=>Position::both(self.resolve_owned_initialization(a,assuming),self.resolve_owned_initialization(b,assuming)),
        Position::Indexed(choices,index)=>{
            for(i,choice)in choices.iter().enumerate(){let selected=Presence::indices(Comparison::Equal,*index,ownership::ApplicationIndex::Constant(i as u32));if self.condition_truth(&selected.condition(),assuming.map(Presence::condition).as_ref())==Some(true){return self.resolve_owned_initialization(choice,assuming);}}
            let choices=choices.iter().enumerate().map(|(i,choice)|{let selected=Presence::indices(Comparison::Equal,*index,ownership::ApplicationIndex::Constant(i as u32));match self.condition_truth(&selected.condition(),assuming.map(Presence::condition).as_ref()){Some(false)=>Position::Initialized,_=>{let when=assuming.cloned().unwrap_or(Presence::All).and(selected);self.resolve_owned_initialization(choice,Some(&when))}}}).collect::<Vec<_>>();if choices.iter().all(|choice|choice==&choices[0]){choices[0].clone()}else{Position::Indexed(choices.into(),*index)}
        },
        position=>position.clone(),
    }}
    pub(super) fn resolve_owned_residency(&self,value:&residency::Residency,assuming:Option<&Presence>)->residency::Residency{value.resolve_selected(assuming,&|when,assuming|self.condition_truth(when,assuming.map(Presence::condition).as_ref()))}
    pub(super) fn owned_possible(&self,value:&Presence)->bool{self.condition_truth(&value.condition(),None)!=Some(false)}
    pub(super) fn resolve_ownership(&self,value:&Ownership,assuming:Option<&Presence>)->Ownership{match value{Ownership::Conditional(when)=>match self.condition_truth(&when.condition(),assuming.map(Presence::condition).as_ref()){Some(true)=>Ownership::Owned,Some(false)=>Ownership::Moved,None=>value.clone()},_=>value.clone()}}
    pub(super) fn resolve_owned_model(&self,value:&mut Returned,assuming:Option<&Presence>){match value{Returned::Dataset(data)=>{data.ownership=self.resolve_ownership(&data.ownership,assuming);data.residency=self.resolve_owned_residency(&data.residency,assuming);data.initialization=data.initialization.as_ref().map(|scope|self.resolve_owned_initialization(scope,assuming));},Returned::OwnedValue(value)=>{value.ownership=self.resolve_ownership(&value.ownership,assuming);for payload in value.payloads.iter_mut().flatten(){self.resolve_owned_model(payload,assuming);}},_=>{}}}
    pub(super) fn prepare_owned_read(&mut self,value:&Returned){match value{Returned::Dataset(data)if data.ownership.available()=>{if let Some(place)=&data.place{self.initialize(place);}},Returned::OwnedValue(value)=>{if value.value.ty.has_owned_presence(){if value.ownership.available(){if let Some(place)=value.place(){self.initialize(&place);}}}else{for i in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(i).1.owns_resources(){let child=self.unpack_record(value,i);self.prepare_owned_read(&child);}}}},_=>{}}}
    pub(super) fn forget_owned_metadata(&mut self,value:&Returned){
        fn intersects(position:&Position,changed:&BTreeSet<Position>)->bool{changed.contains(position)||matches!(position,Position::Indexed(choices,_)if choices.iter().any(|choice|intersects(choice,changed)))}
        match value{Returned::Dataset(data)=>{if let Some(place)=&data.place{let changed=place.owned_positions().unwrap().into_iter().collect();self.initialized.retain(|position|!intersects(position,&changed));}},Returned::OwnedValue(value)=>{if value.value.ty.has_owned_presence(){if let Some(place)=value.place(){let changed=place.owned_positions().unwrap().into_iter().collect();self.initialized.retain(|position|!intersects(position,&changed));}}else{for i in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(i).1.owns_resources(){let child=self.unpack_record(value,i);self.forget_owned_metadata(&child);}}}},_=>{}}
    }
    pub(super) fn presence_register(&mut self,value:&Presence,span:Span,origin:Option<&Arc<GeneratedOrigin>>)->Result<Reg>{
        match value{
            Presence::All=>Ok(self.constant_with_origin(Scalar::Bool,1,origin.cloned()).regs[0]),
            Presence::None=>Ok(self.constant_with_origin(Scalar::Bool,0,origin.cloned()).regs[0]),
            Presence::Input(value)=>Ok(value.regs[0]),
            Presence::Selected(operator,index,element)=>{
                let actual=match index{ownership::ApplicationIndex::Constant(at)=>self.constant_with_origin(Scalar::U32,Bits::from(*at),origin.cloned()).regs[0],ownership::ApplicationIndex::Value(_,reg)=>*reg};
                let rhs=match element{ownership::ApplicationIndex::Constant(at)=>self.constant_with_origin(Scalar::U32,Bits::from(*at),origin.cloned()).regs[0],ownership::ApplicationIndex::Value(_,reg)=>*reg};let dst=self.reg(Scalar::Bool);
                let kind=InstKind::Binary{dst,op:BinaryOp::Compare(*operator),a:actual,b:rhs};
                if let Some(origin)=origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}
                Ok(dst)
            },
            Presence::Not(value)=>{
                let input=self.presence_register(value,span,origin)?;let dst=self.reg(Scalar::Bool);let kind=InstKind::Unary{dst,op:UnaryOp::LogicalNot,a:input};
                if let Some(origin)=origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}
                Ok(dst)
            },
            Presence::Both(a,b)|Presence::Either(a,b)=>{
                let any=matches!(value,Presence::Either(..));let condition=self.presence_register(a,span,origin)?;let(slots,_)=self.local_slots(&Ty::scalar(Scalar::Bool));
                let outer=std::mem::take(&mut self.code);let rhs=self.presence_register(b,span,origin)?;let kind=InstKind::StoreLocal(slots[0],rhs);
                if let Some(origin)=origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}
                let right=std::mem::replace(&mut self.code,outer);let bit=self.constant_with_origin(Scalar::Bool,Bits::from(any),origin.cloned()).regs[0];let kind=InstKind::StoreLocal(slots[0],bit);
                let fixed=vec![match origin{Some(origin)=>Inst::generated(kind,origin.clone()),None=>Inst::new(kind)}];let(yes,no)=if any{(fixed,right)}else{(right,fixed)};
                let kind=InstKind::If{condition,yes,no};
                if let Some(origin)=origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}
                let dst=self.reg(Scalar::Bool);let kind=InstKind::LoadLocal(dst,slots[0]);
                if let Some(origin)=origin{self.code.push(Inst::generated(kind,origin.clone()));}else{self.emit(kind);}
                Ok(dst)
            }
        }
    }
    pub(super) fn owned_when(&mut self,value:&Presence,span:Span,branch_origin:Option<Arc<GeneratedOrigin>>,condition_origin:Option<Arc<GeneratedOrigin>>,body:impl FnOnce(&mut Self)->Result<()>)->Result<()>{
        match self.condition_truth(&value.condition(),None){
            Some(false)=>Ok(()),
            Some(true)=>body(self),
            None=>{
                let condition=self.presence_register(value,span,condition_origin.as_ref())?;let outer=std::mem::take(&mut self.code);let initialized=self.initialized.clone();let path=self.path.clone();
                let mut unselected=self.env.clone();
                self.path=value.condition().assume(&path,true);
                body(self)?;
                self.path=path;
                let selected=self.take_application_initialization_state();
                self.join_initialization(value,selected,initialized);
                self.join_environment_selected(&mut unselected,value);
                let yes=std::mem::replace(&mut self.code,outer);let kind=InstKind::If{condition,yes,no:Vec::new()};
                if let Some(origin)=branch_origin{self.code.push(Inst::generated(kind,origin));}else{self.emit(kind);}
                Ok(())
            }
        }
    }
}

impl owned_values::OwnedValuePayload {
    pub(super) fn restrict_owned(&mut self,when:&Presence){
        if self.value.ty.has_owned_presence(){self.ownership=self.ownership.restricted(when);}
        else{for payload in self.payloads.iter_mut().flatten(){payload.restrict_owned(when);}}
    }
}
