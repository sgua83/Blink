//! 局部拥有关系复用原条件及真实控制值；不形成运行时拥有表。
use super::*;
use application::Ownership;
use returns::Returned;
#[derive(Clone)]
pub(super) enum Presence {All,None,Input(Value),Selected(ownership::ApplicationIndex,usize),Not(Box<Presence>),Both(Box<Presence>,Box<Presence>),Either(Box<Presence>,Box<Presence>)}
impl PartialEq for Presence{fn eq(&self,other:&Self)->bool{match(self,other){(Self::All,Self::All)|(Self::None,Self::None)=>true,(Self::Input(a),Self::Input(b))=>a.identity()==b.identity(),(Self::Selected(a,i),Self::Selected(b,j))=>a==b&&i==j,(Self::Not(a),Self::Not(b))=>a==b,(Self::Both(a,b),Self::Both(c,d))|(Self::Either(a,b),Self::Either(c,d))=>a==c&&b==d,_=>false}}}
impl Eq for Presence{}
impl PartialOrd for Presence{fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}}
impl Ord for Presence{fn cmp(&self,other:&Self)->std::cmp::Ordering{
    fn tag(value:&Presence)->u8{match value{Presence::None=>0,Presence::All=>1,Presence::Input(_)=>2,Presence::Selected(..)=>3,Presence::Not(_)=>4,Presence::Both(..)=>5,Presence::Either(..)=>6}}
    tag(self).cmp(&tag(other)).then_with(||match(self,other){(Self::Input(a),Self::Input(b))=>a.identity().cmp(&b.identity()),(Self::Selected(a,i),Self::Selected(b,j))=>(a,i).cmp(&(b,j)),(Self::Not(a),Self::Not(b))=>a.cmp(b),(Self::Both(a,b),Self::Both(c,d))|(Self::Either(a,b),Self::Either(c,d))=>(a,b).cmp(&(c,d)),_=>std::cmp::Ordering::Equal})
}}

impl Presence{
    pub(super) fn uses_index(&self,index:ownership::ApplicationIndex)->bool{match self{Self::Selected(at,_)=>*at==index,Self::Input(value)=>match index{ownership::ApplicationIndex::Value(id,_)=>self.condition().mentions(id)||value.identity()==id,_=>false},Self::Not(value)=>value.uses_index(index),Self::Both(a,b)|Self::Either(a,b)=>a.uses_index(index)||b.uses_index(index),_=>false}}
    pub(super) fn input(value:&Value)->Self{match value.constant.as_deref(){Some([0])=>Self::None,Some([_])=>Self::All,_=>Self::Input(value.clone())}}
    pub(super) fn not(self)->Self{match self{Self::All=>Self::None,Self::None=>Self::All,Self::Not(value)=>*value,Self::Both(a,b)=>a.not().or(b.not()),Self::Either(a,b)=>a.not().and(b.not()),value=>Self::Not(Box::new(value))}}
    fn opposite(&self,other:&Self)->bool{matches!(self,Self::Not(value)if value.as_ref()==other)||matches!(other,Self::Not(value)if value.as_ref()==self)}
    pub(super) fn and(self,other:Self)->Self{if self==other{return self;}if self.opposite(&other){return Self::None;}match(self,other){(Self::None,_)|(_,Self::None)=>Self::None,(Self::All,value)|(value,Self::All)=>value,(a,b)=>Self::Both(Box::new(a),Box::new(b))}}
    pub(super) fn or(self,other:Self)->Self{if self==other{return self;}if self.opposite(&other){return Self::All;}match(self,other){(Self::All,_)|(_,Self::All)=>Self::All,(Self::None,value)|(value,Self::None)=>value,(a,b)=>Self::Either(Box::new(a),Box::new(b))}}
    pub(super) fn selected(position:&Position,wanted:&Position)->Self{match position{Position::Indexed(choices,index)=>choices.iter().enumerate().fold(Self::None,|all,(i,choice)|{let selected=match index{ownership::ApplicationIndex::Constant(at)=>if *at as usize==i{Self::All}else{Self::None},_=>Self::Selected(*index,i)};all.or(selected.and(Self::selected(choice,wanted)))}),position=>if position==wanted{Self::All}else{Self::None}}}
    pub(super) fn condition(&self)->bounds::Condition{match self{Self::All=>bounds::Condition::constant(true),Self::None=>bounds::Condition::constant(false),Self::Input(value)=>bounds::Condition::of(value),Self::Selected(index,element)=>bounds::Condition::selected(*index,*element),Self::Not(value)=>value.condition().negate(),Self::Both(a,b)=>a.condition().logical(b.condition(),false),Self::Either(a,b)=>a.condition().logical(b.condition(),true)}}
}
impl Ownership{
    pub(super) fn may_own(&self)->bool{matches!(self,Self::Owned|Self::Conditional(_))}
    pub(super) fn available(&self)->bool{matches!(self,Self::Owned|Self::Borrowed)}
    fn presence(&self)->Presence{match self{Self::Moved=>Presence::None,Self::Conditional(value)=>value.as_ref().clone(),_=>Presence::All}}
    fn from_presence(value:Presence)->Self{match value{Presence::All=>Self::Owned,Presence::None=>Self::Moved,value=>Self::Conditional(Box::new(value))}}
    pub(super) fn either(a:&Self,b:&Self,choice:Option<&Presence>)->Self{if a==b{return a.clone();}if matches!(a,Self::Borrowed)||matches!(b,Self::Borrowed){return Self::Moved;}let p=if let Some(choice)=choice{choice.clone().and(a.presence()).or(choice.clone().not().and(b.presence()))}else{a.presence().and(b.presence())};Self::from_presence(p)}
    pub(super) fn restricted(&self,when:&Presence)->Self{if matches!(self,Self::Borrowed){Self::Borrowed}else{Self::from_presence(self.presence().and(when.clone()))}}
}
impl Returned{
    pub(super) fn select_presence(&mut self,candidates:&[(Presence,Returned)]){
        fn mode(values:&[(Presence,&Ownership)])->Ownership{
            if values.iter().all(|(_,value)|*value==values[0].1){return values[0].1.clone();}
            if values.iter().any(|(_,value)|**value==Ownership::Borrowed){return Ownership::Moved;}
            let present=values.iter().fold(Presence::None,|all,(when,value)|all.or(when.clone().and(value.presence())));Ownership::from_presence(present)
        }
        fn restore(target:&mut Returned,values:&[(Presence,&Returned)]){match target{
            Returned::Dataset(target)=>{let values=values.iter().filter_map(|(when,value)|if let Returned::Dataset(data)=value{Some((when.clone(),data))}else{None}).collect::<Vec<_>>();target.ownership=mode(&values.iter().map(|(when,data)|(when.clone(),&data.ownership)).collect::<Vec<_>>());target.residency=residency::Residency::Absent;for(when,data)in values.iter().filter(|(_,data)|data.ownership.may_own()||data.ownership==Ownership::Borrowed){let candidate=residency::Residency::selected(when.clone(),data.residency.clone(),residency::Residency::Absent);target.residency.join(&candidate);}},
            Returned::OwnedValue(target)=>{let values=values.iter().filter_map(|(when,value)|if let Returned::OwnedValue(value)=value{Some((when.clone(),value))}else{None}).collect::<Vec<_>>();target.ownership=mode(&values.iter().map(|(when,value)|(when.clone(),&value.ownership)).collect::<Vec<_>>());for(index,payload)in target.payloads.iter_mut().enumerate(){if let Some(payload)=payload{let children=values.iter().filter_map(|(when,value)|value.payloads[index].as_deref().map(|child|(when.clone(),child))).collect::<Vec<_>>();restore(payload,&children);}}},_=>{}}}
        restore(self,&candidates.iter().map(|(when,value)|(when.clone(),value)).collect::<Vec<_>>());
    }
    pub(super) fn restrict_owned(&mut self,when:&Presence){match self{Self::Dataset(data)=>data.ownership=data.ownership.restricted(when),Self::OwnedValue(value)=>{if value.value.ty.has_variants(){value.ownership=value.ownership.restricted(when);}else{for payload in value.payloads.iter_mut().flatten(){payload.restrict_owned(when);}}},_=>{}}}
    pub(super) fn join_presence(&mut self,other:Self,choice:Option<&Presence>){
        fn restore(target:&mut Returned,a:&Returned,b:&Returned,choice:Option<&Presence>){match(target,a,b){(Returned::Dataset(target),Returned::Dataset(a),Returned::Dataset(b))=>{target.ownership=Ownership::either(&a.ownership,&b.ownership,choice);let ai=a.initialization.clone().unwrap_or(Position::Initialized);let bi=b.initialization.clone().unwrap_or(Position::Initialized);target.initialization=Some(if a.ownership==Ownership::Moved{bi}else if b.ownership==Ownership::Moved{ai}else if let Some(choice)=choice{Position::selected(choice.clone(),ai,bi)}else{Position::both(ai,bi)});if let Some(choice)=choice{target.residency=residency::Residency::selected(choice.clone(),a.residency.clone(),b.residency.clone());}if a.ownership==Ownership::Moved{target.residency=b.residency.clone();}else if b.ownership==Ownership::Moved{target.residency=a.residency.clone();}},(Returned::OwnedValue(target),Returned::OwnedValue(a),Returned::OwnedValue(b))=>{target.ownership=Ownership::either(&a.ownership,&b.ownership,choice);for(index,payload)in target.payloads.iter_mut().enumerate(){if let(Some(target),Some(a),Some(b))=(payload.as_deref_mut(),a.payloads[index].as_deref(),b.payloads[index].as_deref()){restore(target,a,b,choice);}}},_=>{}}}
        let old=self.clone();self.join(other.clone());restore(self,&old,&other,choice);
    }
}
impl Builder<'_>{
    pub(super) fn resolve_owned_initialization(&self,value:&Position,assuming:Option<&Presence>)->Position{match value{
        Position::Conditional(when,a,b)=>match self.condition_truth(&when.condition(),assuming.map(Presence::condition).as_ref()){Some(true)=>self.resolve_owned_initialization(a,assuming),Some(false)=>self.resolve_owned_initialization(b,assuming),None=>{let yes=assuming.cloned().unwrap_or(Presence::All).and(when.as_ref().clone());let no=assuming.cloned().unwrap_or(Presence::All).and(when.as_ref().clone().not());Position::selected(when.as_ref().clone(),self.resolve_owned_initialization(a,Some(&yes)),self.resolve_owned_initialization(b,Some(&no))) }},
        Position::Both(a,b)=>Position::both(self.resolve_owned_initialization(a,assuming),self.resolve_owned_initialization(b,assuming)),
        Position::Indexed(choices,index)=>{
            for(i,choice)in choices.iter().enumerate(){let selected=Presence::Selected(*index,i);if self.condition_truth(&selected.condition(),assuming.map(Presence::condition).as_ref())==Some(true){return self.resolve_owned_initialization(choice,assuming);}}
            let choices=choices.iter().enumerate().map(|(i,choice)|{let selected=Presence::Selected(*index,i);match self.condition_truth(&selected.condition(),assuming.map(Presence::condition).as_ref()){Some(false)=>Position::Initialized,_=>{let when=assuming.cloned().unwrap_or(Presence::All).and(selected);self.resolve_owned_initialization(choice,Some(&when))}}}).collect::<Vec<_>>();if choices.iter().all(|choice|choice==&choices[0]){choices[0].clone()}else{Position::Indexed(choices.into(),*index)}
        },
        position=>position.clone(),
    }}
    pub(super) fn resolve_owned_residency(&self,value:&residency::Residency,assuming:Option<&Presence>)->residency::Residency{value.resolve_selected(assuming,&|when,assuming|self.condition_truth(when,assuming.map(Presence::condition).as_ref()))}
    pub(super) fn owned_possible(&self,value:&Presence)->bool{self.condition_truth(&value.condition(),None)!=Some(false)}
    pub(super) fn resolve_ownership(&self,value:&Ownership,assuming:Option<&Presence>)->Ownership{match value{Ownership::Conditional(when)=>match self.condition_truth(&when.condition(),assuming.map(Presence::condition).as_ref()){Some(true)=>Ownership::Owned,Some(false)=>Ownership::Moved,None=>value.clone()},_=>value.clone()}}
    pub(super) fn resolve_owned_model(&self,value:&mut Returned,assuming:Option<&Presence>){match value{Returned::Dataset(data)=>{data.ownership=self.resolve_ownership(&data.ownership,assuming);data.residency=self.resolve_owned_residency(&data.residency,assuming);data.initialization=data.initialization.as_ref().map(|scope|self.resolve_owned_initialization(scope,assuming));},Returned::OwnedValue(value)=>{value.ownership=self.resolve_ownership(&value.ownership,assuming);for payload in value.payloads.iter_mut().flatten(){self.resolve_owned_model(payload,assuming);}},_=>{}}}
    pub(super) fn prepare_owned_read(&mut self,value:&Returned){match value{Returned::Dataset(data)if data.ownership.available()=>{if let Some(place)=&data.place{self.initialize(place);}},Returned::OwnedValue(value)=>{if value.value.ty.has_variants(){if value.ownership.available(){if let Some(place)=value.place(){self.initialize(&place);}}}else{for i in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(i).1.owns_resources(){let child=self.unpack_record(value,i);self.prepare_owned_read(&child);}}}},_=>{}}}
    pub(super) fn forget_owned_metadata(&mut self,value:&Returned){
        fn intersects(position:&Position,changed:&BTreeSet<Position>)->bool{changed.contains(position)||matches!(position,Position::Indexed(choices,_)if choices.iter().any(|choice|intersects(choice,changed)))}
        match value{Returned::Dataset(data)=>{if let Some(place)=&data.place{let changed=place.owned_positions().unwrap().into_iter().collect();self.initialized.retain(|position|!intersects(position,&changed));}},Returned::OwnedValue(value)=>{if value.value.ty.has_variants(){if let Some(place)=value.place(){let changed=place.owned_positions().unwrap().into_iter().collect();self.initialized.retain(|position|!intersects(position,&changed));}}else{for i in 0..value.value.ty.aggregate_len(){if value.value.ty.aggregate_part(i).1.owns_resources(){let child=self.unpack_record(value,i);self.forget_owned_metadata(&child);}}}},_=>{}}
    }
    fn presence_register(&mut self,value:&Presence,span:Span)->Result<Reg>{
        match value{Presence::All=>Ok(self.constant(Scalar::Bool,1).regs[0]),Presence::None=>Ok(self.constant(Scalar::Bool,0).regs[0]),Presence::Input(value)=>Ok(value.regs[0]),Presence::Selected(index,element)=>{
            let actual=match index{ownership::ApplicationIndex::Constant(n)=>return Ok(self.constant(Scalar::Bool,u32::from(*n as usize==*element)).regs[0]),ownership::ApplicationIndex::Value(_,reg)=>*reg};let rhs=self.constant(Scalar::U32,*element as u32).regs[0];let dst=self.reg(Scalar::Bool);self.code.push(Inst::Binary{dst,op:BinaryOp::Compare(Comparison::Equal),a:actual,b:rhs});Ok(dst)
        },Presence::Not(value)=>{let input=self.presence_register(value,span)?;let dst=self.reg(Scalar::Bool);self.code.push(Inst::Unary{dst,op:UnaryOp::LogicalNot,a:input});Ok(dst)},Presence::Both(a,b)|Presence::Either(a,b)=>{
            let any=matches!(value,Presence::Either(..));let condition=self.presence_register(a,span)?;let(slots,_)=self.local_slots(&Ty::scalar(Scalar::Bool));let outer=std::mem::take(&mut self.code);let rhs=self.presence_register(b,span)?;self.code.push(Inst::StoreLocal(slots[0],rhs));let right=std::mem::replace(&mut self.code,outer);let bit=self.constant(Scalar::Bool,u32::from(any)).regs[0];let fixed=vec![Inst::StoreLocal(slots[0],bit)];let(yes,no)=if any{(fixed,right)}else{(right,fixed)};self.code.push(Inst::If{condition,yes,no});let dst=self.reg(Scalar::Bool);self.code.push(Inst::LoadLocal(dst,slots[0]));Ok(dst)
        }}
    }
    pub(super) fn owned_when(&mut self,value:&Presence,span:Span,body:impl FnOnce(&mut Self)->Result<()>)->Result<()>{
        match self.condition_truth(&value.condition(),None){Some(false)=>Ok(()),Some(true)=>{let initialized=self.initialized.clone();body(self)?;self.initialized=initialized;Ok(())},None=>{let condition=self.presence_register(value,span)?;let outer=std::mem::take(&mut self.code);let initialized=self.initialized.clone();body(self)?;self.initialized=initialized;let yes=std::mem::replace(&mut self.code,outer);self.code.push(Inst::If{condition,yes,no:Vec::new()});Ok(())}}
    }
}
