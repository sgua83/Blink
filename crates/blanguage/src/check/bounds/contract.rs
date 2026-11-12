//! Temporary public-contract normalization. The resulting facts still belong
//! to the original Clause/Condition; no decision state survives the encoder.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Variable { Integer(ValueIdentity, Scalar), Atom(Fact) }
impl Variable {
    fn end(self) -> u128 { match self { Self::Integer(_, scalar) => u128::from(scalar.bit_mask())+1, Self::Atom(_) => 2 } }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Decision { variable: Variable, cells: Vec<(u128, usize)> }

pub(in crate::check) struct Normalizer {
    types: BTreeMap<ValueIdentity, Option<Scalar>>,
    lengths: BTreeMap<ValueIdentity, ValueIdentity>,
    nodes: Vec<Option<Decision>>,
    unique: BTreeMap<Decision, usize>,
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
    fn node(&mut self, variable:Variable, cells:Vec<(u128,usize)>)->usize {
        let mut merged:Vec<(u128,usize)>=Vec::new();
        for (end,value) in cells {
            if let Some(previous)=merged.last_mut() && previous.1==value { previous.0=end; }
            else { merged.push((end,value)); }
        }
        if merged.len()==1 { return merged[0].1; }
        let decision=Decision {variable,cells:merged};
        if let Some(&index)=self.unique.get(&decision) { return index; }
        let index=self.nodes.len();self.nodes.push(Some(decision.clone()));self.unique.insert(decision,index);index
    }
    fn atom(&mut self, fact:Fact)->usize { self.node(Variable::Atom(fact),vec![(1,0),(2,1)]) }
    fn cut(&mut self,id:ValueIdentity,scalar:Scalar,upper:Bits)->usize {
        if upper>=scalar.bit_mask() { return 1; }
        self.node(Variable::Integer(id,scalar),vec![(u128::from(upper)+1,1),(u128::from(scalar.bit_mask())+1,0)])
    }
    fn pairs(&self,a:usize,b:usize)->(Variable,Vec<(u128,usize,usize)>) {
        let left=self.nodes[a].as_ref().unwrap();let right=self.nodes[b].as_ref().unwrap();
        if left.variable<right.variable { return (left.variable,left.cells.iter().map(|&(end,value)|(end,value,b)).collect()); }
        if right.variable<left.variable { return (right.variable,right.cells.iter().map(|&(end,value)|(end,a,value)).collect()); }
        let mut pairs=Vec::new();let(mut i,mut j)=(0,0);
        while i<left.cells.len() && j<right.cells.len() {
            let end=left.cells[i].0.min(right.cells[j].0);
            pairs.push((end,left.cells[i].1,right.cells[j].1));
            if left.cells[i].0==end {i+=1;}if right.cells[j].0==end {j+=1;}
        }
        (left.variable,pairs)
    }
    fn operation(&mut self,a:usize,b:usize,or:bool)->usize {
        let key=|a:usize,b:usize|(or,a.min(b),a.max(b));let root=key(a,b);let mut pending=vec![(root,false)];
        while let Some((current,ready))=pending.pop() {
            if self.operations.contains_key(&current){continue;}
            let(_,a,b)=current;
            let simple=if a==b {Some(a)} else if or {
                if a==1||b==1 {Some(1)}else if a==0 {Some(b)}else{None}
            } else if a==0 {Some(0)}else if a==1 {Some(b)}else{None};
            if let Some(result)=simple {self.operations.insert(current,result);continue;}
            let(variable,pairs)=self.pairs(a,b);
            if !ready {
                pending.push((current,true));
                pending.extend(pairs.iter().rev().map(|&(_,a,b)|(key(a,b),false)));
            } else {
                let cells=pairs.into_iter().map(|(end,a,b)|(end,self.operations[&key(a,b)])).collect();
                let result=self.node(variable,cells);self.operations.insert(current,result);
            }
        }
        self.operations[&root]
    }
    fn not(&mut self,root:usize)->usize {
        let mut pending=vec![(root,false)];
        while let Some((index,ready))=pending.pop() {
            if self.negatives.contains_key(&index){continue;}
            let decision=self.nodes[index].as_ref().unwrap().clone();
            if !ready {
                pending.push((index,true));pending.extend(decision.cells.iter().rev().map(|&(_,child)|(child,false)));
            } else {
                let cells=decision.cells.into_iter().map(|(end,child)|(end,self.negatives[&child])).collect();
                let inverse=self.node(decision.variable,cells);self.negatives.insert(index,inverse);self.negatives.insert(inverse,index);
            }
        }
        self.negatives[&root]
    }
    fn minimum(&mut self,id:ValueIdentity,scalar:Scalar,lower:Bits)->usize {
        if lower==0 {return 1;}
        let cut=self.cut(id,scalar,lower-1);self.not(cut)
    }
    fn zero(&mut self,id:ValueIdentity,scalar:Scalar)->usize {
        let zero=scalar.order_bias();let upper=self.cut(id,scalar,zero);
        let lower=self.minimum(id,scalar,zero);self.operation(lower,upper,false)
    }
    fn less(&mut self,a:ValueIdentity,b:ValueIdentity,scalar:Scalar)->usize {
        if a==b {return 0;}
        let relation=self.atom(Fact::Below(a,b,true,scalar));
        let before_max=self.cut(a,scalar,scalar.bit_mask()-1);
        let after_min=self.minimum(b,scalar,1);
        let relation=self.operation(relation,before_max,false);self.operation(relation,after_min,false)
    }
    fn compare(&mut self,op:Comparison,scalar:Scalar,a:Operand,b:Operand)->Option<usize> {
        if !scalar.is_integer(){return None;}
        let order=|bits:Bits|scalar.truncate(bits)^scalar.order_bias();
        let less=|this:&mut Self,a:Operand,b:Operand|match(a,b) {
            (Operand::Value(a),Operand::Value(b))=>Some(this.less(a,b,scalar)),
            (Operand::Value(a),Operand::Constant(s,b))if s==scalar=>Some(if order(b)==0 {0}else{this.cut(a,scalar,order(b)-1)}),
            (Operand::Constant(s,a),Operand::Value(b))if s==scalar=>Some(if order(a)==scalar.bit_mask(){0}else{this.minimum(b,scalar,order(a)+1)}),
            (Operand::Constant(sa,a),Operand::Constant(sb,b))if sa==scalar&&sb==scalar=>Some(usize::from(order(a)<order(b))),
            _=>None,
        };
        Some(match op {
            Comparison::Less=>less(self,a,b)?,Comparison::Greater=>less(self,b,a)?,
            Comparison::LessEqual=>{let value=less(self,b,a)?;self.not(value)},
            Comparison::GreaterEqual=>{let value=less(self,a,b)?;self.not(value)},
            Comparison::Equal|Comparison::NotEqual=>{
                let left=less(self,a,b)?;let right=less(self,b,a)?;let unequal=self.operation(left,right,true);
                if op==Comparison::Equal {self.not(unequal)}else{unequal}
            },
        })
    }
    fn fact(&mut self,fact:Fact)->usize {
        let fact=fact.numeric_alias();
        match fact {
            Fact::Minimum(id,scalar,lower)if scalar.is_integer()=>return self.minimum(id,scalar,lower),
            Fact::Maximum(id,scalar,upper)if scalar.is_integer()=>return self.cut(id,scalar,upper),
            Fact::Zero(id)|Fact::NonZero(id)=>{
                if let Some(scalar)=self.integer_type(id) {
                    let zero=self.zero(id,scalar);return if matches!(fact,Fact::Zero(_)){zero}else{self.not(zero)};
                }
                if self.types.get(&id)==Some(&Some(Scalar::Bool)) {
                    let truth=self.atom(Fact::Boolean(id,true));return if matches!(fact,Fact::NonZero(_)){truth}else{self.not(truth)};
                }
            },
            Fact::Equal(a,b)|Fact::NotEqual(a,b)=>{
                if a==b{return usize::from(matches!(fact,Fact::Equal(..)));}
                if let Some(scalar)=self.integer_type(a).filter(|scalar|self.integer_type(b)==Some(*scalar)) {
                    return self.compare(if matches!(fact,Fact::Equal(..)){Comparison::Equal}else{Comparison::NotEqual},scalar,Operand::Value(a),Operand::Value(b)).unwrap();
                }
            },
            Fact::Below(a,b,strict,scalar)if scalar.is_integer()=>return self.compare(if strict{Comparison::Less}else{Comparison::LessEqual},scalar,Operand::Value(a),Operand::Value(b)).unwrap(),
            Fact::Predicate(predicate,truth)=>if let Some(value)=self.compare(predicate.op,predicate.scalar,predicate.a,predicate.b){return if truth{value}else{self.not(value)};},
            Fact::Index(id,domain)|Fact::Outside(id,domain)=>{
                let relation=match domain {
                    IndexDomain::Fixed(count)=>Some(if count==0{0}else{self.cut(id,Scalar::U32,Bits::from(count)-1)}),
                    IndexDomain::Upper(limit,distance)if distance<=1=>self.integer_type(id).filter(|scalar|!scalar.is_signed()&&self.integer_type(limit)==Some(*scalar))
                        .map(|scalar|self.compare(if distance==0{Comparison::LessEqual}else{Comparison::Less},scalar,Operand::Value(id),Operand::Value(limit)).unwrap()),
                    IndexDomain::ApplicationData(root)|IndexDomain::ApplicationLength(root)=>self.lengths.get(&root).copied().and_then(|length|
                        self.integer_type(id).filter(|scalar|!scalar.is_signed()&&self.integer_type(length)==Some(*scalar)).map(|scalar|
                            self.compare(if matches!(domain,IndexDomain::ApplicationData(_)){Comparison::Less}else{Comparison::LessEqual},scalar,Operand::Value(id),Operand::Value(length)).unwrap())),
                    _=>None,
                };
                if let Some(value)=relation{return if matches!(fact,Fact::Index(..)){value}else{self.not(value)};}
            },
            _=>{},
        }
        let(positive,truth)=match fact {
            Fact::Boolean(id,truth)=>(Fact::Boolean(id,true),truth),Fact::NonZero(id)=>(Fact::Zero(id),false),
            Fact::NotEqual(a,b)=>(Fact::Equal(a.min(b),a.max(b)),false),Fact::Equal(a,b)=>(Fact::Equal(a.min(b),a.max(b)),true),
            Fact::Outside(id,domain)=>(Fact::Index(id,domain),false),Fact::Predicate(predicate,truth)=>(Fact::Predicate(predicate,true),truth),
            other=>(other,true),
        };
        let value=self.atom(positive);if truth{value}else{self.not(value)}
    }
    fn unary(fact:&Fact)->bool {
        matches!(fact,Fact::Minimum(..)|Fact::Maximum(..)|Fact::Zero(..)|Fact::NonZero(..)|Fact::Boolean(..)
            |Fact::Index(_,IndexDomain::Fixed(_))|Fact::Outside(_,IndexDomain::Fixed(_)))
            || matches!(fact,Fact::Predicate(predicate,_)if matches!((predicate.a,predicate.b),(Operand::Constant(..),_)|(_,Operand::Constant(..))))
    }
    // Fold the source join as one operation; its binary prefixes have no separate consumer.
    fn operands<'a>(&self,root:&'a Clause,or:bool)->Vec<&'a Clause> {
        let mut pending=vec![root];let mut seen=HashSet::new();let mut operands=Vec::new();
        while let Some(clause)=pending.pop() {
            if !seen.insert(clause.key()){continue;}
            if self.clauses.contains_key(&clause.key()){operands.push(clause);continue;}
            match &**clause.0 {
                ClauseNode::Both(a,b)if !or=>pending.extend([b,a]),
                ClauseNode::Either(a,b)if or=>pending.extend([b,a]),
                ClauseNode::Branch(parent,condition,truth)if !or=>pending.extend([&condition.cases[usize::from(*truth)],parent]),
                _=>operands.push(clause),
            }
        }
        operands.sort_by_key(|clause|!matches!(&**clause.0,ClauseNode::Impossible)
            && !matches!(&**clause.0,ClauseNode::Facts(facts)if facts.iter().all(Self::unary)));
        operands
    }
    fn import(&mut self,root:&Clause)->usize {
        enum Work<'a> {
            Visit(&'a Clause),
            Fold { clause:&'a Clause, or:bool, operands:Vec<&'a Clause>, next:usize, result:usize },
        }
        let mut pending=vec![Work::Visit(root)];
        while let Some(work)=pending.pop() {
            match work {
                Work::Visit(clause)=>{
                    if self.clauses.contains_key(&clause.key()){continue;}
                    let result=match &**clause.0 {
                        ClauseNode::Impossible=>0,
                        ClauseNode::Facts(facts)=>{
                            let mut result=1;
                            for unary in [true,false] {for fact in facts {
                                if Self::unary(fact)==unary {
                                    let fact=self.fact(*fact);result=self.operation(result,fact,false);
                                }
                            }}
                            result
                        },
                        _=>{
                            let or=matches!(&**clause.0,ClauseNode::Either(..));
                            pending.push(Work::Fold{clause,or,operands:self.operands(clause,or),next:0,result:usize::from(!or)});
                            continue;
                        },
                    };
                    self.clauses.insert(clause.key(),(clause.clone(),result));
                },
                Work::Fold{clause,or,operands,mut next,mut result}=>{
                    while next<operands.len() && result!=usize::from(or) {
                        let operand=operands[next];
                        let Some(&(_,value))=self.clauses.get(&operand.key())else{break;};
                        result=self.operation(result,value,or);next+=1;
                    }
                    if next<operands.len() && result!=usize::from(or) {
                        let operand=operands[next];
                        pending.push(Work::Fold{clause,or,operands,next,result});pending.push(Work::Visit(operand));
                    }else{self.clauses.insert(clause.key(),(clause.clone(),result));}
                },
            }
        }
        self.clauses[&root.key()].1
    }
    fn emit(&mut self,root:usize)->Clause {
        let mut pending=vec![(root,false)];
        while let Some((index,ready))=pending.pop() {
            if self.emitted.contains_key(&index){continue;}
            let decision=self.nodes[index].as_ref().unwrap().clone();
            if !ready {
                pending.push((index,true));pending.extend(decision.cells.iter().rev().map(|&(_,child)|(child,false)));continue;
            }
            let mut result:Option<Clause>=None;let mut begin=0;
            for(end,child)in decision.cells {
                if child!=0 {
                    let guard=match decision.variable {
                        Variable::Integer(id,scalar)=>{
                            let mut facts=Facts::new();
                            if begin!=0{facts.insert(Fact::Minimum(id,scalar,begin as Bits));}
                            if end!=decision.variable.end(){facts.insert(Fact::Maximum(id,scalar,(end-1)as Bits));}
                            Clause::facts(facts)
                        },
                        Variable::Atom(fact)=>{
                            let positive=Clause::facts([fact].into());if begin==0{positive.negate()}else{positive}
                        },
                    };
                    let term=if child==1{guard}else if matches!(&**guard.0,ClauseNode::Facts(facts)if facts.is_empty()){self.emitted[&child].clone()}
                        else{Clause::node(ClauseNode::Both(guard,self.emitted[&child].clone()))};
                    result=Some(match result {None=>term,Some(previous)=>Clause::node(ClauseNode::Either(previous,term))});
                }
                begin=end;
            }
            self.emitted.insert(index,result.unwrap_or_else(Clause::impossible));
        }
        self.emitted[&root].clone()
    }
    pub(in crate::check) fn clause(&mut self,value:&Clause)->Clause { let root=self.import(value);self.emit(root) }
    pub(in crate::check) fn condition(&mut self,value:&Condition)->Condition { Condition {cases:value.cases.each_ref().map(|case|self.clause(case))} }
    pub(in crate::check) fn union(&mut self,a:&Condition,b:&Condition)->Condition {
        Condition {cases:std::array::from_fn(|truth| {let a=self.import(&a.cases[truth]);let b=self.import(&b.cases[truth]);let root=self.operation(a,b,truth!=0);self.emit(root)})}
    }
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
