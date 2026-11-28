//! 局部拥有关系复用原条件及真实控制值；不形成运行时拥有表。
use super::*;
use application::Ownership;
use returns::Returned;
#[repr(C, u32)]
#[derive(Clone)]
pub(super) enum Presence {
    None = 0,
    All = 1,
    Input(Value) = 2,
    Selected(Comparison, ownership::ApplicationIndex, ownership::ApplicationIndex) = 3,
    Not(Box<Presence>) = 4,
    Both(Box<Presence>, Box<Presence>) = 5,
    Either(Box<Presence>, Box<Presence>) = 6,
}
mod host;
pub(super) mod path;

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
    pub(super) fn from_presence(value: Presence) -> Self { host::from_presence(value) }
    pub(super) fn loop_compatible(&self, other: &Self, inputs: &BTreeMap<ValueIdentity,bounds::Proof>, path: &bounds::Clause) -> bool { host::compatible(self, other, inputs, path) }
    pub(super) fn either(a: &Self, b: &Self, choice: Option<&Presence>) -> Self { host::either(a, b, choice) }
    pub(super) fn restricted(&self, when: &Presence) -> Self { host::restricted(self, when) }
}
impl Returned{
    pub(super) fn restrict_owned(&mut self,when:&Presence){match self{Self::Dataset(data)=>data.ownership=data.ownership.restricted(when),Self::OwnedValue(value)=>value.restrict_owned(when),_=>{}}}
}
impl Builder<'_>{
    pub(super) fn resolve_owned_initialization(&self,value:&Position,assuming:Option<&Presence>)->Position{path::initialization(self,value,assuming)}
    pub(super) fn resolve_owned_residency(&self,value:&residency::Residency,assuming:Option<&Presence>)->residency::Residency{path::resolve_residency(self,value,assuming)}
    pub(super) fn owned_possible(&self,value:&Presence)->bool{path::possible(self,value)}
    pub(super) fn resolve_ownership(&self,value:&Ownership,assuming:Option<&Presence>)->Ownership{path::ownership(self,value,assuming)}
    pub(super) fn resolve_owned_model(&self,value:&mut Returned,assuming:Option<&Presence>){path::resolve_model(self,value,assuming)}
    pub(super) fn prepare_owned_read(&mut self,value:&Returned){path::prepare(self,value)}
    pub(super) fn prepare_owned_value(&mut self,value:&owned_values::OwnedValuePayload){path::prepare_value(self,value)}
    pub(super) fn prepare_owned_data(&mut self,value:&application::Data){path::prepare_data(self,value)}
    pub(super) fn prepare_selected_owned_read(&mut self,value:&owned_values::OwnedValuePayload){path::prepare_selected_value(self,value)}
    pub(super) fn forget_owned_metadata(&mut self,value:&Returned){path::forget(self,value)}
    pub(super) fn forget_owned_value(&mut self,value:&owned_values::OwnedValuePayload){path::forget_value(self,value)}
    pub(super) fn presence_register(&mut self, value: &Presence, origin: Option<&Arc<GeneratedOrigin>>) -> Reg {
        host::emit_presence(self, value, origin)
    }
    pub(super) async fn owned_when(&mut self,value:&Presence,branch_origin:Option<Arc<GeneratedOrigin>>,condition_origin:Option<Arc<GeneratedOrigin>>,body:impl std::ops::AsyncFnOnce(&mut Self)->Result<()>)->Result<()>{
        let projected = value.condition();
        match self.condition_truth(&projected,None){
            Some(false)=>Ok(()),
            Some(true)=>body(self).await,
            None=>{
                let condition=self.presence_register(value,condition_origin.as_ref());let outer=std::mem::take(&mut self.code);let initialized=self.initialized.clone();let path=self.path.clone();
                let unselected=self.env.clone();
                self.path=projected.assume(&path,true);
                body(self).await?;
                self.path=path;
                let selected=self.take_application_initialization_state();
                self.join_initialization(value,selected,initialized);
                self.join_environment_selected(unselected,value,&projected);
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
