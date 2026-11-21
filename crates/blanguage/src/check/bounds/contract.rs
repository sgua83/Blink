//! Temporary public-contract normalization. The resulting facts still belong
//! to the original Clause/Condition; no decision state survives the encoder.
use super::*;
mod native;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
// Typed bounds already use the scalar ordered-bit domain, including floats.
enum Variable { Range(ValueIdentity, Scalar), Atom(Fact) }
impl Variable {
    fn end(self) -> u128 { match self { Self::Range(_, scalar) => u128::from(scalar.bit_mask())+1, Self::Atom(_) => 2 } }
}
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Decision { variable: Variable, cells: Vec<(u128, usize)> }

pub(in crate::check) struct Normalizer {
    types: BTreeMap<ValueIdentity, Option<Scalar>>,
    lengths: BTreeMap<ValueIdentity, ValueIdentity>,
    // Ordered keys and numbered consumers share one immutable decision.
    nodes: Vec<Option<Arc<Decision>>>,
    unique: BTreeMap<Arc<Decision>, usize>,
    operations: HashMap<(bool, usize, usize), usize>,
    negatives: HashMap<usize, usize>,
    // Retain each original input while its pointer is a local memo key.
    clauses: HashMap<usize, (Clause, usize)>,
    emitted: HashMap<usize, Clause>,
}
impl Normalizer {
    pub(in crate::check) fn new() -> Self {
        Self { types: BTreeMap::new(), lengths: BTreeMap::new(), nodes: vec![None,None], unique:BTreeMap::new(),
            operations:HashMap::new(), negatives:HashMap::from([(0,1),(1,0)]), clauses:HashMap::new(),
            emitted:HashMap::from([(0,Clause::impossible()),(1,Clause::default())]) }
    }
    pub(in crate::check) fn storage(&self) -> crate::compiler::ConditionStorage {
        use crate::compiler::{AnalysisStorage, ConditionStorage};
        ConditionStorage {
            decisions: AnalysisStorage::vector(&self.nodes),
            unique: AnalysisStorage::entries::<Arc<Decision>, usize>(self.unique.len(), None),
            operations: AnalysisStorage::entries::<(bool, usize, usize), usize>(
                self.operations.len(), Some(self.operations.capacity())),
            negatives: AnalysisStorage::entries::<usize, usize>(self.negatives.len(), Some(self.negatives.capacity())),
            imported: AnalysisStorage::entries::<usize, (Clause, usize)>(self.clauses.len(), Some(self.clauses.capacity())),
            emitted: AnalysisStorage::entries::<usize, Clause>(self.emitted.len(), Some(self.emitted.capacity())),
        }
    }

    pub(in crate::check) fn scalar(&mut self, id:ValueIdentity, scalar:Scalar) {
        self.types.entry(id).and_modify(|old| { if *old!=Some(scalar) { *old=None; } }).or_insert(Some(scalar));
    }
    pub(in crate::check) fn dataset(&mut self, root:ValueIdentity, length:ValueIdentity) {
        // The original input ABI owns its length. A returned alias must not
        // redefine that input using the length of a projected output view.
        self.lengths.entry(root).or_insert(length);
    }
    fn integer_type(&self,id:ValueIdentity)->Option<Scalar> {
        let scalar=match id { ValueIdentity::DataLength(_)|ValueIdentity::TileLength=>Some(Scalar::U32), _=>self.types.get(&id).copied().flatten() }?;
        scalar.is_integer().then_some(scalar)
    }
    fn node(&mut self,variable:Variable,cells:Vec<(u128,usize)>)->usize {native::node(self,variable,cells)}
    fn pairs(&self,a:usize,b:usize)->(Variable,Vec<(u128,usize,usize)>) {native::pairs(self,a,b)}
    fn operation(&mut self,a:usize,b:usize,or:bool)->usize {native::operation(self,a,b,or)}
    fn not(&mut self,root:usize)->usize {native::not(self,root)}
    fn fact(&mut self,fact:Fact)->usize {native::fact(self,fact)}
    fn operands<'a>(&self,root:&'a Clause,or:bool)->Vec<&'a Clause> {native::operands(self,root,or)}
    fn import(&mut self,root:&Clause)->usize {native::import(self,root)}
    fn emit(&mut self,root:usize)->Clause {native::emit(self,root)}
    pub(in crate::check) fn clause(&mut self,value:&Clause)->Clause { let root=self.import(value);self.emit(root) }
    pub(in crate::check) fn condition(&mut self,value:&Condition)->Condition { Condition {cases:value.cases.each_ref().map(|case|self.clause(case))} }
    pub(in crate::check) fn union(&mut self,a:&Condition,b:&Condition)->Condition {native::union(self,a,b)}
    pub(in crate::check) fn proof(&mut self,proof:&mut Proof) {
        for condition in proof.conditions.iter_mut().flatten(){**condition=self.condition(condition);}
        for borrow in &mut proof.views{borrow.normalize_contract(self);}
        for identity in proof.addresses.iter_mut().flatten(){if let super::super::address::Identity::Storage(borrow)=identity{borrow.normalize_contract(self);}}
    }
    pub(in crate::check) fn origin(&mut self,origin:&mut super::super::address::Origin) {
        use super::super::address::{Identity,Root};
        match &mut origin.root {
            Root::Resource{borrow,..}|Root::Dataset{borrow,..}|Root::Derived(borrow)=>borrow.normalize_contract(self),
            Root::External(Identity::Storage(borrow))=>borrow.normalize_contract(self),_=>{},
        }
    }
}
