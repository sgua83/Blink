//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
use super::*;
use std::sync::Arc;

pub(super) fn seeks(body: &[Stmt]) -> bool {
    body.iter().any(|statement| match &statement.kind {
        StmtKind::Seek(..) => true,
        StmtKind::If(_, yes, no) => seeks(yes) || seeks(no),
        StmtKind::Match(_, arms) => arms.iter().any(|arm|seeks(&arm.body)),
        // Nested loops own their reentry; their seek cannot replace this cursor.
        _ => false,
    })
}
pub(super) fn for_cursor(begin: &Value, limit: &Value, body: &[Stmt]) -> Proof {
    let minimum = if seeks(body) { 0 } else { begin.minimum() };
    Proof::unsigned(minimum, limit.maximum().saturating_sub(1))
        .with_domains(indices_below(&limit.proof.domains(0), true))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fact {
    Equal(ValueIdentity, ValueIdentity),
    Index(ValueIdentity, IndexDomain),
    Minimum(ValueIdentity, u32),
    Maximum(ValueIdentity, u32),
    NonZero(ValueIdentity),
    Boolean(ValueIdentity, bool),
    Below(ValueIdentity, ValueIdentity, bool, bool),
    Outside(ValueIdentity,IndexDomain),
    Zero(ValueIdentity),
}
type Facts = BTreeSet<Fact>;
type OutputBindings = BTreeMap<ValueIdentity, ValueIdentity>;
fn output_identity(identity: ValueIdentity, bindings: &OutputBindings) -> ValueIdentity {
    bindings.get(&identity).copied().unwrap_or(identity)
}
fn output_domain(domain: IndexDomain, bindings: &OutputBindings) -> IndexDomain {
    match domain {
        IndexDomain::View(ids) => IndexDomain::View(ids.map(|id| output_identity(id, bindings))),
        IndexDomain::ViewLength(ids) => {
            IndexDomain::ViewLength(ids.map(|id| output_identity(id, bindings)))
        }
        IndexDomain::ApplicationData(id) => {
            IndexDomain::ApplicationData(output_identity(id, bindings))
        }
        IndexDomain::ApplicationLength(id) => {
            IndexDomain::ApplicationLength(output_identity(id, bindings))
        }
        other => other,
    }
}
impl Place {
    fn refine_position(
        &mut self,
        identity: ValueIdentity,
        fact: Fact,
        roots: &BTreeMap<ValueIdentity, ValueIdentity>,
    ) {
        match self {
            Self::ApplicationElement {
                index, index_range, ..
            } => {
                let position = ValueIdentity::Register(*index);
                if roots.get(&position).unwrap_or(&position)
                    == roots.get(&identity).unwrap_or(&identity)
                {
                    match fact {
                        Fact::Minimum(_, lower) => index_range.0 = index_range.0.max(lower),
                        Fact::Maximum(_, upper) => index_range.1 = index_range.1.min(upper),
                        Fact::NonZero(_) => index_range.0 = index_range.0.max(1),
                        _ => {}
                    }
                }
            }
            Self::Indexed { base, .. } => base.refine_position(identity, fact, roots),
            _ => {}
        }
    }
}
#[derive(Clone)]
struct IntegerRanges(Vec<(u32, u32)>);
impl Default for IntegerRanges {
    fn default() -> Self {
        Self(vec![(0, u32::MAX)])
    }
}
impl IntegerRanges {
    fn from_intervals(intervals: impl IntoIterator<Item = (u32, u32)>) -> Self {
        let mut intervals: Vec<_> = intervals.into_iter().filter(|(a, b)| a <= b).collect();
        intervals.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (lower, upper) in intervals {
            if let Some(last) = merged.last_mut()
                && u64::from(lower) <= u64::from(last.1) + 1
            {
                last.1 = last.1.max(upper);
            } else {
                merged.push((lower, upper));
            }
        }
        Self(merged)
    }
    fn interval(lower: u32, upper: u32) -> Self {
        Self::from_intervals([(lower, upper)])
    }
    fn hull(&self) -> (u32, u32) {
        match (self.0.first(), self.0.last()) {
            (Some(first), Some(last)) => (first.0, last.1),
            _ => (1, 0),
        }
    }
    fn contains(&self, value: u32) -> bool {
        self.0
            .iter()
            .any(|&(lower, upper)| lower <= value && value <= upper)
    }
    fn union(&self, other: &Self) -> Self {
        Self::from_intervals(self.0.iter().chain(&other.0).copied())
    }
    fn restrict(&self, lower: u32, upper: u32) -> Self {
        Self::from_intervals(self.0.iter().map(|&(a, b)| (a.max(lower), b.min(upper))))
    }
    fn without(&self, value: u32) -> Self {
        let mut intervals = Vec::new();
        for &(lower, upper) in &self.0 {
            if value < lower || value > upper {
                intervals.push((lower, upper));
            } else {
                if lower < value {
                    intervals.push((lower, value - 1));
                }
                if value < upper {
                    intervals.push((value + 1, upper));
                }
            }
        }
        Self(intervals)
    }
    fn map(&self, mut operation: impl FnMut((u32, u32)) -> Self) -> Self {
        Self::from_intervals(self.0.iter().flat_map(|&interval| operation(interval).0))
    }
    fn flip_sign_order(&self) -> Self {
        self.map(|(lower, upper)| {
            integer_result_range(
                (
                    i128::from(lower) + (1i128 << 31),
                    i128::from(upper) + (1i128 << 31),
                ),
                0,
            )
        })
    }
    fn shift_counts(&self, bias: u32) -> Self {
        self.map(|(lower, upper)| {
            let mask = u32::BITS - 1;
            if upper - lower >= mask {
                return Self::interval(bias, bias | mask);
            }
            // 两种有序编码只差符号位，低五位与原移位量相同。
            let (a, b) = (lower & mask, upper & mask);
            if a <= b {
                Self::interval(bias | a, bias | b)
            } else {
                Self::from_intervals([(bias, bias | b), (bias | a, bias | mask)])
            }
        })
    }
    fn combine(
        &self,
        other: &Self,
        mut operation: impl FnMut((u32, u32), (u32, u32)) -> Self,
    ) -> Self {
        let mut intervals = Vec::new();
        for &left in &self.0 {
            for &right in &other.0 {
                intervals.extend(operation(left, right).0);
            }
        }
        Self::from_intervals(intervals)
    }
}
#[derive(Clone, Default)]
pub(super) struct Proof {
    // 首项是实际来源，其余是当前路径已经证明相等的快照。
    origins: Vec<Vec<ValueIdentity>>,
    conditions: Vec<Option<Box<Condition>>>,
    // 整数按数值顺序编码；i32 翻转符号位，复用相同的区间与路径合流。
    ranges: Vec<IntegerRanges>,
    nonzero: Vec<bool>,
    domains: Vec<Domains>,
    // view分量的实际借用来源，与数值事实分别合流；只存在于编译期。
    views: Vec<view::Borrow>,
}
impl Proof {
    pub(super) fn borrow_state(&self)->Self {Self{views:self.views.clone(),..Self::default()}}
    pub(super) fn typed_view_borrow(&self,ty:&Ty)->view::Borrow {view_contracts::components(ty).into_iter().fold(view::Borrow::default(),|mut all,component|{all.union(&self.view_borrow(component));all})}
    pub(super) fn select_view_borrows(&mut self,values:&[(Condition,Self)]){
        let width=values.iter().map(|(_,proof)|proof.views.len()).max().unwrap_or(0);
        self.views=(0..width).map(|component|{let mut value=view::Borrow::default();for(when,proof)in values{value.union(&proof.view_borrow(component).restricted(&when.assume(&Clause::default(),true)));}value}).collect();
    }
    pub(super) fn join_view_borrows(&mut self,a:&Self,b:&Self,choice:Option<&Condition>){
        let a_when=choice.cloned().unwrap_or_else(||Condition::constant(true));let b_when=choice.cloned().map(Condition::negate).unwrap_or_else(||Condition::constant(true));
        self.select_view_borrows(&[(a_when,a.borrow_state()),(b_when,b.borrow_state())]);
    }
    pub(super) fn retain_view_borrows(&mut self,ty:&Ty) {let selected=view_contracts::components(ty);for(component,borrow)in self.views.iter_mut().enumerate(){if !selected.contains(&component){*borrow=view::Borrow::default();}}}
    pub(super) fn view_borrow(&self,component:usize)->view::Borrow {self.views.get(component).cloned().unwrap_or_default()}
    pub(super) fn borrowed_view(&self)->view::Borrow {self.views.iter().fold(view::Borrow::default(),|mut all,value|{all.union(value);all})}
    pub(super) fn borrowed_views(&self)->BTreeSet<view::Source> {self.borrowed_view().sources.entries().filter_map(|(source,when)|when.possible().then_some(*source)).collect()}
    pub(super) fn with_view_sources(self,component:usize,sources:BTreeSet<view::Source>)->Self {self.with_view_borrow(component,view::Borrow::from_sources(sources))}
    pub(super) fn with_view_borrow(mut self,component:usize,borrow:view::Borrow)->Self {
        self.views.resize_with(self.views.len().max(component+1),view::Borrow::default);self.views[component]=borrow;self
    }
    pub(super) fn view_parameters(mut self, ty: &Ty, identity: impl Fn(usize) -> ValueIdentity) -> Self {
        for at in view_contracts::components(ty) {
            self = self.with_view_sources(at, BTreeSet::from([view::Source::Parameter(identity(at))]));
        }
        self
    }
    pub(super) fn width(&self) -> usize {
        self.origins.len()
    }
    pub(super) fn output_bindings(
        &self,
        registers: &[Reg],
        entry: usize,
        bindings: &mut OutputBindings,
    ) {
        for (origins, reg) in self.origins.iter().zip(registers) {
            for identity in origins {
                if let ValueIdentity::Register(source) = identity
                    && *source >= entry
                    && source != reg
                {
                    bindings
                        .entry(*identity)
                        .or_insert(ValueIdentity::Register(*reg));
                }
            }
        }
    }
    pub(super) fn output_aliases(
        &self,
        registers: &[Reg],
        types: &[Scalar],
        bindings: &OutputBindings,
    ) -> Clause {
        let mut outputs=self.output_bound(bindings);
        outputs.origins.resize_with(registers.len(),Vec::new);
        for(origins,reg)in outputs.origins.iter_mut().zip(registers){let id=ValueIdentity::Register(*reg);if !origins.contains(&id){origins.push(id);}}
        let values=registers.iter().enumerate().filter(|(_,reg)|types[**reg]==Scalar::Bool).flat_map(|(component,_)|outputs.project(&[component],registers.len()).facts(&Ty::scalar(Scalar::Bool))).collect();
        let mut aliases=Clause::both(&Clause::facts(values),&Clause::facts(
            self.origins
                .iter()
                .zip(registers)
                .filter(|(_, reg)| matches!(types[**reg], Scalar::Bool | Scalar::U32 | Scalar::I32))
                .flat_map(|(origins, reg)| {
                    let target = ValueIdentity::Register(*reg);
                    origins
                        .iter()
                        .map(|id| output_identity(*id, bindings))
                        .filter(move |id| *id != target)
                        .map(move |id| Fact::equal(id, target))
                })
                .collect(),
        ));
        for(component,reg)in registers.iter().enumerate(){if types[*reg]==Scalar::Bool && let Some(condition)=outputs.component_condition(component){
            let cases=std::array::from_fn::<_,2,_>(|truth|Clause::both(&condition.cases[truth],&Clause::facts([Fact::Boolean(ValueIdentity::Register(*reg),truth!=0)].into())));
            aliases=Clause::both(&aliases,&Clause::either(&cases[0],&cases[1]));
        }}aliases
    }
    pub(super) fn output_bound(&self,bindings:&OutputBindings)->Self{
        let mut proof=self.clone();
        for sources in &mut proof.views {
            *sources=sources.output_bound(bindings);
        }
        for origins in &mut proof.origins {
            for identity in origins {
                *identity = output_identity(*identity, bindings);
            }
        }
        for domains in &mut proof.domains {
            *domains = domains
                .iter()
                .map(|d| output_domain(*d, bindings))
                .collect();
        }
        for condition in proof.conditions.iter_mut().flatten() {
            for case in &mut condition.cases {
                *case = case.output_bound(bindings);
            }
        }
        proof
    }
    pub(super) fn bind_outputs(
        &mut self,
        ty: &Ty,
        registers: Option<&[Reg]>,
        bindings: &OutputBindings,
        aliases: &Clause,
    ) {
        *self=self.output_bound(bindings);
        if let Some(registers) = registers {
            self.origins.resize_with(registers.len(), Vec::new);
            for (origins, reg) in self.origins.iter_mut().zip(registers) {
                let identity = ValueIdentity::Register(*reg);
                if !origins.contains(&identity) {
                    origins.push(identity);
                }
            }
        }
        self.under_path(ty, aliases);
    }
    // 函数体的寄存器身份只在该编译单元内有效；调用边界只翻译实际参数关系。
    pub(super) fn at_call(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        fn identity(
            id: ValueIdentity,
            bindings: &BTreeMap<ValueIdentity, Proof>,
        ) -> Option<ValueIdentity> {
            bindings.get(&id)?.known_identity(0)
        }
        fn domain(
            value: IndexDomain,
            bindings: &BTreeMap<ValueIdentity, Proof>,
        ) -> Option<IndexDomain> {
            Some(match value {
                IndexDomain::Fixed(n) => IndexDomain::Fixed(n),
                IndexDomain::ApplicationData(id) => {
                    IndexDomain::ApplicationData(identity(id, bindings)?)
                }
                IndexDomain::ApplicationLength(id) => {
                    IndexDomain::ApplicationLength(identity(id, bindings)?)
                }
                IndexDomain::View(ids) => IndexDomain::View([
                    identity(ids[0], bindings)?,
                    identity(ids[1], bindings)?,
                    identity(ids[2], bindings)?,
                ]),
                IndexDomain::ViewLength(ids) => IndexDomain::ViewLength([
                    identity(ids[0], bindings)?,
                    identity(ids[1], bindings)?,
                    identity(ids[2], bindings)?,
                ]),
                _ => return None,
            })
        }
        fn fact(value: Fact, bindings: &BTreeMap<ValueIdentity, Proof>) -> Clause {
            let translated = match value {
                Fact::Boolean(id, truth) => {
                    let Some(value) = bindings.get(&id) else {
                        return Clause::facts(Facts::new());
                    };
                    let condition = value.component_condition(0).cloned().unwrap_or_default();
                    let mut clause = condition.cases[usize::from(truth)].clone();
                    if let Some(id) = value.known_identity(0) {
                        clause = Clause::both(
                            &clause,
                            &Clause::facts([Fact::Boolean(id, truth)].into()),
                        );
                    }
                    return clause;
                }
                Fact::Below(lower, upper, strict, signed) => {
                    let Some((lower_id, upper_id)) =
                        identity(lower, bindings).zip(identity(upper, bindings))
                    else {
                        return Clause::facts(Facts::new());
                    };
                    let (lower, upper) = (&bindings[&lower], &bindings[&upper]);
                    let same = lower.origins[0]
                        .iter()
                        .any(|id| upper.origins[0].contains(id));
                    let possible = lower.range_set(0).0.iter().any(|&(a, b)| {
                        upper.range_set(0).0.iter().any(|&(c, d)| {
                            if same {
                                !strict && a.max(c) <= b.min(d)
                            } else if strict {
                                a < d
                            } else {
                                a <= d
                            }
                        })
                    });
                    if !possible {
                        return Clause::impossible();
                    }
                    let mut facts = Facts::new();
                    constrain_proofs(
                        &mut facts,
                        (lower_id, lower),
                        (upper_id, upper),
                        strict,
                        signed,
                    );
                    return Clause::facts(facts);
                }
                Fact::Equal(a, b) => identity(a, bindings)
                    .zip(identity(b, bindings))
                    .map(|(a, b)| Fact::equal(a, b)),
                Fact::Index(id, d) => identity(id, bindings)
                    .zip(domain(d, bindings))
                    .map(|(id, d)| Fact::Index(id, d)),
                Fact::Minimum(id,n)=>{
                    if let Some(value)=bindings.get(&id){let range=value.range_set(0).hull();if range.0>=n{return Clause::default();}if range.1<n{return Clause::impossible();}}
                    identity(id,bindings).map(|id|Fact::Minimum(id,n))
                },
                Fact::Maximum(id,n)=>{
                    if let Some(value)=bindings.get(&id){let range=value.range_set(0).hull();if range.1<=n{return Clause::default();}if range.0>n{return Clause::impossible();}}
                    identity(id,bindings).map(|id|Fact::Maximum(id,n))
                },
                Fact::NonZero(id)=>{
                    if bindings.get(&id).is_some_and(|value|value.nonzero.first()==Some(&true)){return Clause::default();}
                    identity(id,bindings).map(Fact::NonZero)
                },
                Fact::Zero(id)=>{
                    if bindings.get(&id).is_some_and(|value|value.nonzero.first()==Some(&true)){return Clause::impossible();}
                    identity(id,bindings).map(Fact::Zero)
                },
                Fact::Outside(id,d)=>identity(id,bindings).zip(domain(d,bindings)).map(|(id,d)|Fact::Outside(id,d)),
            };
            Clause::facts(translated.into_iter().collect())
        }
        let mut translated = self.clone();
        translated.views = self.views.iter().map(|borrow|borrow.at_call(bindings)).collect();
        for (i, origins) in self.origins.iter().enumerate() {
            let values = origins
                .iter()
                .filter_map(|id| bindings.get(id))
                .collect::<Vec<_>>();
            translated.origins[i] = values
                .iter()
                .flat_map(|proof| proof.origins.first().into_iter().flatten().copied())
                .collect();
            translated
                .ranges
                .resize_with(translated.ranges.len().max(i + 1), IntegerRanges::default);
            translated
                .nonzero
                .resize(translated.nonzero.len().max(i + 1), false);
            translated
                .domains
                .resize_with(translated.domains.len().max(i + 1), Domains::new);
            translated.domains[i] = self
                .domains(i)
                .into_iter()
                .filter_map(|d| domain(d, bindings))
                .collect();
            for value in values {
                let incoming = value.range_set(0);
                translated.ranges[i] = IntegerRanges::from_intervals(
                    translated.ranges[i].0.iter().flat_map(|(a, b)| {
                        incoming
                            .0
                            .iter()
                            .map(move |(c, d)| ((*a).max(*c), (*b).min(*d)))
                    }),
                );
                translated.nonzero[i] |= value.nonzero.first() == Some(&true);
                translated.domains[i].extend(value.domains(0));
            }
        }
        // Domains may exist without a captured origin (for example a proven index).
        for i in self.origins.len()..translated.domains.len() {
            translated.domains[i] = self
                .domains(i)
                .into_iter()
                .filter_map(|d| domain(d, bindings))
                .collect();
        }
        let roots: Vec<_> = self
            .conditions
            .iter()
            .flatten()
            .flat_map(|condition| condition.cases.iter().cloned())
            .collect();
        if !roots.is_empty() {
            let mut clauses = BTreeMap::new();
            for clause in Clause::ordered(&roots) {
                let mapped = match &*clause.0 {
                    ClauseNode::Impossible => Clause::impossible(),
                    ClauseNode::Facts(facts) => facts
                        .iter()
                        .fold(Clause::facts(Facts::new()), |current, value| {
                            Clause::both(&current, &fact(*value, bindings))
                        }),
                    ClauseNode::Both(a, b) => Clause::both(&clauses[&a.key()], &clauses[&b.key()]),
                    ClauseNode::Branch(parent, condition, truth) => Clause::both(
                        &clauses[&parent.key()],
                        &clauses[&condition.cases[usize::from(*truth)].key()],
                    ),
                    ClauseNode::Either(a, b) => {
                        Clause::either(&clauses[&a.key()], &clauses[&b.key()])
                    }
                };
                clauses.insert(clause.key(), mapped);
            }
            for condition in translated.conditions.iter_mut().flatten() {
                condition.cases =
                    std::array::from_fn(|i| clauses[&condition.cases[i].key()].clone());
            }
        }
        // 输出仍是入口快照时，继续交接调用者已经持有的分量条件。
        // 实参图属于调用者命名空间，不能再次按被调函数的身份替换。
        for (i, origins) in self.origins.iter().enumerate() {
            for incoming in origins.iter().filter_map(|id| bindings.get(id)) {
                if let Some(condition) = incoming.component_condition(0) {
                    translated
                        .conditions
                        .resize_with(translated.conditions.len().max(i + 1), || None);
                    let current = translated.conditions[i]
                        .get_or_insert_with(|| Box::new(Condition::default()));
                    for truth in 0..2 {
                        current.cases[truth] =
                            Clause::both(&current.cases[truth], &condition.cases[truth]);
                    }
                }
            }
        }
        translated
    }
    pub(super) fn known_value(&self, ty: &Ty) -> Option<Vec<u32>> {
        ty.components
            .iter()
            .enumerate()
            .map(|(component, (scalar, _))| {
                if *scalar == Scalar::Bool {
                    return match self
                        .component_condition(component)?
                        .cases
                        .each_ref()
                        .map(Clause::possible)
                    {
                        [false, true] => Some(1),
                        [true, false] => Some(0),
                        _ => None,
                    };
                }
                let bias = match scalar {
                    Scalar::U32 => 0,
                    Scalar::I32 => 1 << 31,
                    _ => return None,
                };
                match self.range_set(component).0.as_slice() {
                    [(lower, upper)] if lower == upper => Some(lower ^ bias),
                    _ => None,
                }
            })
            .collect()
    }
    pub(super) fn known_identity(&self, component: usize) -> Option<ValueIdentity> {
        self.origins
            .get(component)
            .and_then(|origins| origins.first())
            .copied()
    }
    // 数值事实失效不改变仍存在的view所借用的实际存储。
    pub(super) fn forget_value(&mut self, components: &[usize], width: usize) {
        let fresh=Self { views:components.iter().map(|i|self.view_borrow(*i)).collect(),..Self::default() };
        self.assign(components,width,fresh);
    }
    pub(super) fn assign_view_borrows(&mut self,positions:&[Position],owner:&[Position],fresh:&Self){
        self.views.resize_with(owner.len(),view::Borrow::default);
        for(component,position)in positions.iter().enumerate(){
            let sources=fresh.view_borrow(component);
            for(index,leaf)in owner.iter().enumerate(){
                let selected=owned_presence::Presence::selected(position,leaf);
                if selected!=owned_presence::Presence::None{self.views[index]=view::Borrow::selected(&selected.condition(),&sources,&self.views[index]);}
            }
        }
    }
    pub(super) fn assign(&mut self, components: &[usize], width: usize, fresh: Self) {
        if components.iter().copied().eq(0..width) {
            *self = fresh;
        } else {
            self.views.resize_with(width, view::Borrow::default);
            self.origins.resize_with(width, Vec::new);
            self.conditions.resize_with(width, || None);
            self.nonzero.resize(width, false);
            self.ranges.resize_with(width, IntegerRanges::default);
            self.domains.resize(width, Domains::new());
            for (j, &i) in components.iter().enumerate() {
                self.views[i] = fresh.view_borrow(j);
                self.origins[i] = fresh.origins.get(j).cloned().unwrap_or_default();
                self.conditions[i] = fresh.conditions.get(j).cloned().flatten();
                self.nonzero[i] = fresh.nonzero.get(j) == Some(&true);
                self.ranges[i] = fresh.range_set(j);
                self.domains[i] = fresh.domains(j);
            }
        }
    }
    pub(super) fn join(&mut self,other:&Self,width:usize,branch:Option<&Value>){let choice=branch.map(Condition::of);self.join_condition(other,width,choice.as_ref());}
    pub(super) fn join_condition(&mut self,other:&Self,width:usize,branch:Option<&Condition>){
        let conditions = (0..width)
            .map(|i| {
                if self.component_condition(i).is_none() && other.component_condition(i).is_none() {
                    return None;
                }
                let a = self.component_condition(i).cloned().unwrap_or_default();
                let b = other.component_condition(i).cloned().unwrap_or_default();
                Some(Box::new(if let Some(c) = branch {
                    Condition::branch(c.clone(), a, b)
                } else {
                    Condition::common(a, b)
                }))
            })
            .collect();
        self.views=(0..width).map(|i|if let Some(choice)=branch{view::Borrow::selected(choice,&self.view_borrow(i),&other.view_borrow(i))}else{self.view_borrow(i).joined(other.view_borrow(i))}).collect();
        self.origins = (0..width)
            .map(|i| {
                let a = self.origins.get(i).cloned().unwrap_or_default();
                let b = other.origins.get(i).cloned().unwrap_or_default();
                a.into_iter()
                    .filter(|identity| b.contains(identity))
                    .collect()
            })
            .collect();
        self.nonzero = (0..width)
            .map(|i| self.nonzero.get(i) == Some(&true) && other.nonzero.get(i) == Some(&true))
            .collect();
        self.ranges = (0..width)
            .map(|i| self.range_set(i).union(&other.range_set(i)))
            .collect();
        self.domains = (0..width)
            .map(|i| {
                self.domains(i)
                    .intersection(&other.domains(i))
                    .copied()
                    .collect()
            })
            .collect();
        self.conditions = conditions;
    }
    pub(super) fn project(&self, components: &[usize], width: usize) -> Self {
        if components.iter().copied().eq(0..width) {
            return self.clone();
        }
        Self {
            views: components.iter().map(|&i| self.view_borrow(i)).collect(),
            origins: components
                .iter()
                .map(|&i| self.origins.get(i).cloned().unwrap_or_default())
                .collect(),
            conditions: components
                .iter()
                .map(|&i| self.conditions.get(i).cloned().flatten())
                .collect(),
            nonzero: components
                .iter()
                .map(|&i| self.nonzero.get(i) == Some(&true))
                .collect(),
            ranges: components.iter().map(|&i| self.range_set(i)).collect(),
            domains: components.iter().map(|&i| self.domains(i)).collect(),
        }
    }
    pub fn condition(condition: Condition) -> Self {
        Self {
            conditions: vec![Some(Box::new(condition))],
            ..Self::default()
        }
    }
    pub fn unsigned(minimum: u32, maximum: u32) -> Self {
        Self {
            ranges: vec![IntegerRanges::interval(minimum, maximum)],
            ..Self::default()
        }
    }
    pub fn with_domains(mut self, domains: Domains) -> Self {
        self.domains = vec![domains];
        self
    }
    pub fn with_identity(mut self, identity: ValueIdentity) -> Self {
        let mut origins = self.origins.first().cloned().unwrap_or_default();
        origins.retain(|origin| *origin != identity);
        origins.insert(0, identity);
        self.origins = vec![origins];
        self
    }
    pub(super) fn parameter_content(parameter: usize, width: usize) -> Self {
        Self {
            origins: (0..width)
                .map(|component| vec![ValueIdentity::ParameterContent(parameter, component)])
                .collect(),
            ..Self::default()
        }
    }
    pub(super) fn domains(&self, component: usize) -> Domains {
        self.domains.get(component).cloned().unwrap_or_default()
    }
    fn component_condition(&self, component: usize) -> Option<&Condition> {
        self.conditions.get(component)?.as_deref()
    }
    fn value_condition(value: &Value, component: usize) -> Option<Box<Condition>> {
        if value.ty.components[component].0 == Scalar::Bool {
            Some(Box::new(Condition::component(value, component)))
        } else {
            value.proof.conditions.get(component).cloned().flatten()
        }
    }
    fn range(&self, component: usize) -> (u32, u32) {
        self.range_set(component).hull()
    }
    fn range_set(&self, component: usize) -> IntegerRanges {
        self.ranges.get(component).cloned().unwrap_or_default()
    }
    pub fn integer_bits(value: &Value) -> Self {
        Self {
            ranges: (0..value.regs.len())
                .map(|i| value.component_ranges(i).flip_sign_order())
                .collect(),
            nonzero: (0..value.regs.len())
                .map(|i| value.component_nonzero(i))
                .collect(),
            // 索引域约束原 u32 位模式；中间 i32 仍不能直接作为索引。
            domains: (0..value.regs.len())
                .map(|i| value.proof.domains(i))
                .collect(),
            ..Self::default()
        }
    }
    pub fn capture(value: &Value) -> Self {
        let mut proof = value.proof.clone();
        proof.ranges = (0..value.regs.len())
            .map(|i| value.component_ranges(i))
            .collect();
        proof.nonzero = (0..value.regs.len())
            .map(|i| value.component_nonzero(i))
            .collect();
        proof.origins = (0..value.regs.len())
            .map(|i| value.component_origins(i))
            .collect();
        proof.conditions = (0..value.regs.len())
            .map(|i| Self::value_condition(value, i))
            .collect();
        proof
    }
    pub fn common(a: &Value, b: &Value) -> Self {
        Self {
            views: (0..a.regs.len()).map(|i| a.proof.view_borrow(i).joined(b.proof.view_borrow(i))).collect(),
            origins: (0..a.regs.len())
                .map(|i| {
                    let other = b.component_origins(i);
                    a.component_origins(i)
                        .into_iter()
                        .filter(|id| other.contains(id))
                        .collect()
                })
                .collect(),
            conditions: (0..a.regs.len())
                .map(|i| {
                    let (a, b) = (Self::value_condition(a, i), Self::value_condition(b, i));
                    if a.is_none() && b.is_none() {
                        None
                    } else {
                        Some(Box::new(Condition::common(
                            a.as_deref().cloned().unwrap_or_default(),
                            b.as_deref().cloned().unwrap_or_default(),
                        )))
                    }
                })
                .collect(),
            ranges: (0..a.regs.len())
                .map(|i| a.component_ranges(i).union(&b.component_ranges(i)))
                .collect(),
            nonzero: (0..a.regs.len())
                .map(|i| a.component_nonzero(i) && b.component_nonzero(i))
                .collect(),
            domains: (0..a.regs.len())
                .map(|i| {
                    a.proof
                        .domains(i)
                        .intersection(&b.proof.domains(i))
                        .copied()
                        .collect()
                })
                .collect(),
            ..Self::default()
        }
    }
    pub fn selection(c: &Value, a: &Value, b: &Value) -> Self {
        let mut proof = Self::common(a, b);
        for (i, condition) in proof.conditions.iter_mut().enumerate() {
            if condition.is_some() {
                *condition = Some(Box::new(Condition::branch(
                    Condition::of(c),
                    Condition::component(a, i),
                    Condition::component(b, i),
                )));
            }
        }
        proof
    }
    pub fn comparison(op: Comparison, a: &Value, b: &Value) -> Self {
        let width = a.regs.len().max(b.regs.len());
        if width == 1 {
            return Self::condition(Condition::comparison(op, a, b));
        }
        fn component(value: &Value, i: usize) -> Value {
            let i = i % value.regs.len();
            Value {
                ty: Ty::scalar(value.ty.components[i].0),
                regs: vec![value.regs[i]],
                proof: value.projected_proof(&[i]),
                constant: value.constant.as_ref().map(|bits| vec![bits[i]]),
                uniform: value.uniform,
                literal: value.literal,
            }
        }
        Self {
            conditions: (0..width)
                .map(|i| {
                    Some(Box::new(Condition::comparison(
                        op,
                        &component(a, i),
                        &component(b, i),
                    )))
                })
                .collect(),
            ..Self::default()
        }
    }
    fn facts(&self, ty: &Ty) -> Facts {
        let mut facts = Facts::new();
        for (component, (scalar, _)) in
            ty.components.iter().enumerate().filter(|(_, (scalar, _))| {
                matches!(scalar, Scalar::Bool | Scalar::U32 | Scalar::I32)
            })
        {
            let Some(origins) = self.origins.get(component) else {
                continue;
            };
            for pair in origins.windows(2) {
                facts.insert(Fact::equal(pair[0], pair[1]));
            }
            for &identity in origins {
                if *scalar == Scalar::Bool
                    && let Some(condition) = self.component_condition(component)
                {
                    for truth in [false, true] {
                        if !condition.cases[usize::from(!truth)].possible() {
                            facts.insert(Fact::Boolean(identity, truth));
                        }
                    }
                }
                if matches!(scalar, Scalar::U32 | Scalar::I32)
                    && self.nonzero.get(component) == Some(&true)
                {
                    facts.insert(Fact::NonZero(identity));
                }
                if matches!(scalar, Scalar::U32 | Scalar::I32) {
                    let (minimum, maximum) = self.range(component);
                    let zero=if *scalar==Scalar::I32{0x8000_0000}else{0};
                    if minimum==zero && maximum==zero{facts.insert(Fact::Zero(identity));}
                    if minimum != 0 {
                        facts.insert(Fact::Minimum(identity, minimum));
                    }
                    if maximum != u32::MAX {
                        facts.insert(Fact::Maximum(identity, maximum));
                    }
                    if *scalar == Scalar::U32 {
                        facts.extend(
                            self.domains(component)
                                .into_iter()
                                .map(|d| Fact::Index(identity, d)),
                        );
                    }
                }
            }
        }
        facts
    }
    pub(super) fn returning(
        value: &Value,
        environment: &BTreeMap<String, Binding>,
        path: &Clause,
    ) -> Self {
        let mut proof = value.proof.clone();
        proof.conditions = (0..value.regs.len())
            .map(|i| Self::value_condition(value, i))
            .collect();
        if proof.conditions.iter().any(Option::is_some) {
            let facts = Clause::facts(environment_facts(environment));
            for condition in proof.conditions.iter_mut().flatten() {
                for case in &mut condition.cases {
                    *case = Clause::both(case, &facts);
                }
            }
        }
        proof.under_path(&value.ty, path);
        proof
    }
    pub(super) fn under_path(&mut self, ty: &Ty, path: &Clause) {
        self.conditions
            .resize_with(self.conditions.len().max(ty.components.len()), || None);
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            if *scalar == Scalar::Bool && self.conditions[component].is_none() {
                let mut condition = Condition::default();
                if let Some(identity) = self.known_identity(component) {
                    for (truth, case) in condition.cases.iter_mut().enumerate() {
                        *case = Clause::facts([Fact::Boolean(identity, truth != 0)].into());
                    }
                }
                self.conditions[component] = Some(Box::new(condition));
            }
            if let Some(condition) = &mut self.conditions[component] {
                for case in &mut condition.cases {
                    *case = Clause::both(case, path);
                }
            }
        }
    }
    pub fn append(&mut self, value: &Value) {
        self.views.extend((0..value.regs.len()).map(|i| value.proof.view_borrow(i)));
        self.conditions
            .extend((0..value.regs.len()).map(|i| Self::value_condition(value, i)));
        self.origins
            .extend((0..value.regs.len()).map(|i| value.component_origins(i)));
        self.nonzero
            .extend((0..value.regs.len()).map(|i| value.component_nonzero(i)));
        self.ranges
            .extend((0..value.regs.len()).map(|i| value.component_ranges(i)));
        self.domains
            .extend((0..value.regs.len()).map(|i| value.proof.domains(i)));
    }
    pub fn splat(&mut self, width: usize) {
        self.views = vec![self.view_borrow(0); width];
        self.conditions = vec![self.conditions.first().cloned().flatten(); width];
        self.origins = vec![self.origins[0].clone(); width];
        self.nonzero = vec![self.nonzero[0]; width];
        self.ranges = vec![self.ranges[0].clone(); width];
        self.domains = vec![self.domains[0].clone(); width];
    }
    pub(super) fn mark_nonzero(&mut self, component: usize) {
        self.nonzero
            .resize(self.nonzero.len().max(component + 1), false);
        self.nonzero[component] = true;
    }
    pub(super) fn mark_domain(&mut self, component: usize, domain: IndexDomain) {
        if let IndexDomain::Fixed(count) = domain {
            // 固定形状的索引事实同时给出数值上界；保留原有区间与空路径。
            self.ranges
                .resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
            self.ranges[component] = self.ranges[component].restrict(0, count).without(count);
        }
        self.domains
            .resize(self.domains.len().max(component + 1), Domains::new());
        self.domains[component].insert(domain);
    }
    pub(super) fn unsigned_minimum(&self, component: usize) -> u32 {
        self.range(component)
            .0
            .max(u32::from(self.nonzero.get(component) == Some(&true)))
    }
    fn refine(&mut self, ty: &Ty, component: usize, fact: Fact) {
        match fact {
            Fact::Boolean(_, truth) if ty.components[component].0 == Scalar::Bool => {
                self.conditions
                    .resize_with(self.conditions.len().max(component + 1), || None);
                let condition = self.conditions[component]
                    .get_or_insert_with(|| Box::new(Condition::default()));
                condition.cases[usize::from(!truth)] = Clause::impossible();
            }
            Fact::Index(_, domain) if ty.components[component].0 == Scalar::U32 => {
                self.mark_domain(component, domain);
            }
            Fact::Minimum(_, lower)
                if matches!(ty.components[component].0, Scalar::U32 | Scalar::I32) =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
                self.ranges[component] = self.ranges[component].restrict(lower, u32::MAX);
                if ty.components[component].0 == Scalar::U32 && lower != 0 {
                    self.mark_nonzero(component);
                }
            }
            Fact::Maximum(_, upper)
                if matches!(ty.components[component].0, Scalar::U32 | Scalar::I32) =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
                self.ranges[component] = self.ranges[component].restrict(0, upper);
            }
            Fact::Zero(_) if matches!(ty.components[component].0,Scalar::U32|Scalar::I32)=>{
                let zero=if ty.components[component].0==Scalar::I32{0x8000_0000}else{0};
                self.ranges.resize_with(self.ranges.len().max(component+1),IntegerRanges::default);
                self.ranges[component]=self.ranges[component].restrict(zero,zero);
            },
            Fact::NonZero(_) if matches!(ty.components[component].0, Scalar::U32 | Scalar::I32) => {
                self.mark_nonzero(component);
            }
            _ => {}
        }
    }
}
fn environment_facts(environment: &BTreeMap<String, Binding>) -> Facts {
    environment
        .values()
        .flat_map(|binding| match binding {
            Binding::ApplicationData(data) => data.length.proof.facts(&data.length.ty),
            Binding::Value(value) => Proof::capture(value).facts(&value.ty),
            Binding::Owned(owner) => owner.proof.facts(owner.place.ty()),
            Binding::OwnedValue(owner) => owner
                .value
                .proof
                .facts(&owner.value.ty)
                .into_iter()
                .chain(
                    owner
                        .resources()
                        .into_iter()
                        .flat_map(|d| d.length.proof.facts(&d.length.ty)),
                )
                .collect(),
            Binding::Data(view) => view
                .length
                .clone()
                .with_identity(ValueIdentity::DataLength(view.binding))
                .facts(&Ty::scalar(Scalar::U32)),
            Binding::Tile(view) => view
                .length
                .clone()
                .with_identity(ValueIdentity::TileLength)
                .facts(&Ty::scalar(Scalar::U32)),
            _ => Facts::new(),
        })
        .collect()
}
impl Fact {
    fn equal(a: ValueIdentity, b: ValueIdentity) -> Self {
        Self::Equal(a.min(b), a.max(b))
    }
    fn identity(self) -> Option<ValueIdentity> {
        match self {
            Self::Index(id, _)
            | Self::Outside(id,_)
            | Self::Zero(id)
            | Self::Minimum(id, _)
            | Self::Maximum(id, _)
            | Self::NonZero(id)
            | Self::Boolean(id, _) => Some(id),
            Self::Equal(_, _) | Self::Below(..) => None,
        }
    }
    fn at(self, id: ValueIdentity) -> Self {
        match self {
            Self::Index(_, domain) => Self::Index(id, domain),
            Self::Outside(_,domain)=>Self::Outside(id,domain),
            Self::Zero(_)=>Self::Zero(id),
            Self::Minimum(_, bound) => Self::Minimum(id, bound),
            Self::Maximum(_, bound) => Self::Maximum(id, bound),
            Self::NonZero(_) => Self::NonZero(id),
            Self::Boolean(_, truth) => Self::Boolean(id, truth),
            equal @ Self::Equal(_, _) => equal,
            below @ Self::Below(..) => below,
        }
    }
}
fn equality_roots(facts: &Facts) -> BTreeMap<ValueIdentity, ValueIdentity> {
    let mut roots = BTreeMap::new();
    for fact in facts {
        if let Fact::Equal(a, b) = *fact {
            let a_root = roots.get(&a).copied().unwrap_or(a);
            let b_root = roots.get(&b).copied().unwrap_or(b);
            let root = a_root.min(b_root);
            for current in roots.values_mut() {
                if *current == a_root || *current == b_root {
                    *current = root;
                }
            }
            roots.insert(a, root);
            roots.insert(b, root);
        }
    }
    roots
}
fn equivalent_facts(mut facts: Facts, roots: &BTreeMap<ValueIdentity, ValueIdentity>) -> Facts {
    let mut inferred = Facts::new();
    for fact in &facts {
        if let Some(identity) = fact.identity()
            && let Some(root) = roots.get(&identity)
        {
            inferred.extend(
                roots
                    .iter()
                    .filter(|(_, group)| *group == root)
                    .map(|(id, _)| fact.at(*id)),
            );
        }
    }
    facts.extend(inferred);
    facts
}
impl Proof {
    fn extend_equalities(&mut self, roots: &BTreeMap<ValueIdentity, ValueIdentity>) {
        for origins in &mut self.origins {
            let groups: BTreeSet<_> = origins
                .iter()
                .filter_map(|id| roots.get(id))
                .copied()
                .collect();
            for (id, root) in roots {
                if groups.contains(root) && !origins.contains(id) {
                    origins.push(*id);
                }
            }
        }
    }
}
impl Value {
    pub(super) fn is_nonzero(&self) -> bool {
        self.ty.is_numeric()
            && matches!(
                self.ty.components.first(),
                Some((Scalar::U32 | Scalar::I32, _))
            )
            && (0..self.regs.len()).all(|i| self.component_nonzero(i))
    }
    fn component_nonzero(&self, component: usize) -> bool {
        matches!(self.ty.components[component].0, Scalar::U32 | Scalar::I32)
            && (self.proof.nonzero.get(component) == Some(&true)
                || !self.component_ranges(component).contains(
                    if self.ty.components[component].0 == Scalar::I32 {
                        0x8000_0000
                    } else {
                        0
                    },
                )
                || self.constant.as_ref().is_some_and(|v| v[component] != 0))
    }
    pub(super) fn minimum(&self) -> u32 {
        if self.ty != Ty::scalar(Scalar::U32) {
            return 0;
        }
        self.component_range(0).0
    }
    pub(super) fn maximum(&self) -> u32 {
        if self.ty != Ty::scalar(Scalar::U32) {
            return u32::MAX;
        }
        self.component_range(0).1
    }
    fn component_range(&self, component: usize) -> (u32, u32) {
        self.component_ranges(component).hull()
    }
    fn component_ranges(&self, component: usize) -> IntegerRanges {
        let scalar = self.ty.components[component].0;
        if !matches!(scalar, Scalar::U32 | Scalar::I32) {
            return IntegerRanges::default();
        }
        let bias = if scalar == Scalar::I32 {
            0x8000_0000
        } else {
            0
        };
        let mut ranges = self.proof.range_set(component);
        if self.proof.nonzero.get(component) == Some(&true) {
            ranges = ranges.without(bias);
        }
        if let Some(bits) = &self.constant {
            let value = bits[component] ^ bias;
            ranges = ranges.restrict(value, value);
        }
        ranges
    }
    pub(super) fn identity(&self) -> ValueIdentity {
        self.component_identity(0)
    }
    fn component_identity(&self, index: usize) -> ValueIdentity {
        self.proof
            .origins
            .get(index)
            .and_then(|origins| origins.first())
            .copied()
            .unwrap_or(ValueIdentity::Register(self.regs[index]))
    }
    fn component_origins(&self, index: usize) -> Vec<ValueIdentity> {
        self.proof
            .origins
            .get(index)
            .filter(|origins| !origins.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![ValueIdentity::Register(self.regs[index])])
    }
    fn same_component(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let other = other.component_origins(other_index);
        self.component_origins(index)
            .iter()
            .any(|origin| other.contains(origin))
    }
    pub(super) fn view_identity(&self) -> [ValueIdentity; 3] {
        std::array::from_fn(|i| self.component_identity(i))
    }
    pub(super) fn projected_proof(&self, indices: &[usize]) -> Proof {
        Proof {
            views: indices.iter().map(|i| self.proof.view_borrow(*i)).collect(),
            origins: indices.iter().map(|i| self.component_origins(*i)).collect(),
            conditions: indices
                .iter()
                .map(|i| self.proof.conditions.get(*i).cloned().flatten())
                .collect(),
            ranges: indices.iter().map(|i| self.component_ranges(*i)).collect(),
            nonzero: indices.iter().map(|i| self.component_nonzero(*i)).collect(),
            domains: indices.iter().map(|i| self.proof.domains(*i)).collect(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Condition {
    // false／true 的条件图保留析取，实参代入后才提取各路径共有的事实。
    cases: [Clause; 2],
}
impl Default for Condition {
    fn default() -> Self {
        Self {
            cases: std::array::from_fn(|_| Clause::facts(Facts::new())),
        }
    }
}
#[derive(Clone)]
pub(super) struct Clause(Arc<ClauseNode>);
impl Default for Clause {
    fn default() -> Self {
        Self::facts(Facts::new())
    }
}
enum ClauseNode {
    Impossible,
    Facts(Facts),
    Both(Clause, Clause),
    Either(Clause, Clause),
    Branch(Clause, Arc<Condition>, bool),
}
impl Clause {
    fn negate(&self)->Self{
        fn fact(value:Fact)->Clause{match value{
            Fact::Minimum(id,0)|Fact::Maximum(id,u32::MAX)=>{let _=id;Clause::impossible()},
            Fact::Minimum(id,n)=>Clause::facts([Fact::Maximum(id,n-1)].into()),
            Fact::Maximum(id,n)=>Clause::facts([Fact::Minimum(id,n+1)].into()),
            Fact::NonZero(id)=>Clause::facts([Fact::Zero(id)].into()),
            Fact::Zero(id)=>Clause::facts([Fact::NonZero(id)].into()),
            Fact::Boolean(id,truth)=>Clause::facts([Fact::Boolean(id,!truth)].into()),
            Fact::Equal(a,b)=>Clause::either(&Clause::facts([Fact::Below(a,b,true,false)].into()),&Clause::facts([Fact::Below(b,a,true,false)].into())),
            Fact::Below(a,b,strict,signed)=>Clause::facts([Fact::Below(b,a,!strict,signed)].into()),
            Fact::Index(id,IndexDomain::Fixed(n))=>Clause::facts([Fact::Minimum(id,n)].into()),
            Fact::Index(id,d)=>Clause::facts([Fact::Outside(id,d)].into()),
            Fact::Outside(id,d)=>Clause::facts([Fact::Index(id,d)].into()),
        }}
        let mut values=BTreeMap::new();
        for clause in Self::ordered(std::slice::from_ref(self)){
            let next=match &*clause.0{
                ClauseNode::Impossible=>Self::default(),
                ClauseNode::Facts(facts)=>facts.iter().fold(Self::impossible(),|all,value|Self::either(&all,&fact(*value))),
                ClauseNode::Both(a,b)=>Self::either(&values[&a.key()],&values[&b.key()]),
                ClauseNode::Either(a,b)=>Self::both(&values[&a.key()],&values[&b.key()]),
                ClauseNode::Branch(parent,condition,truth)=>Self::either(&values[&parent.key()],&values[&condition.cases[usize::from(*truth)].key()]),
            };values.insert(clause.key(),next);
        }
        values.remove(&self.key()).unwrap()
    }
    pub(super) fn output_bound(&self, bindings: &OutputBindings) -> Self {
        if bindings.is_empty() {
            return self.clone();
        }
        let identity = |id| output_identity(id, bindings);
        let mut clauses: BTreeMap<usize, Clause> = BTreeMap::new();
        for clause in Self::ordered(std::slice::from_ref(self)) {
            let next = match &*clause.0 {
                ClauseNode::Impossible => clause.clone(),
                ClauseNode::Facts(facts) => {
                    let renamed: Facts = facts
                        .iter()
                        .map(|fact| match *fact {
                            Fact::Equal(a, b) => Fact::equal(identity(a), identity(b)),
                            Fact::Index(id, domain) => {
                                Fact::Index(identity(id), output_domain(domain, bindings))
                            }
                            Fact::Minimum(id, n) => Fact::Minimum(identity(id), n),
                            Fact::Maximum(id, n) => Fact::Maximum(identity(id), n),
                            Fact::NonZero(id) => Fact::NonZero(identity(id)),
                            Fact::Zero(id)=>Fact::Zero(identity(id)),
                            Fact::Outside(id,d)=>Fact::Outside(identity(id),output_domain(d,bindings)),
                            Fact::Boolean(id, truth) => Fact::Boolean(identity(id), truth),
                            Fact::Below(a, b, strict, signed) => {
                                Fact::Below(identity(a), identity(b), strict, signed)
                            }
                        })
                        .collect();
                    if renamed == *facts {
                        clause.clone()
                    } else {
                        Self::facts(renamed)
                    }
                }
                ClauseNode::Both(a, b) | ClauseNode::Either(a, b)
                    if Arc::ptr_eq(&clauses[&a.key()].0, &a.0)
                        && Arc::ptr_eq(&clauses[&b.key()].0, &b.0) =>
                {
                    clause.clone()
                }
                ClauseNode::Both(a, b) => Self::both(&clauses[&a.key()], &clauses[&b.key()]),
                ClauseNode::Either(a, b) => Self::either(&clauses[&a.key()], &clauses[&b.key()]),
                ClauseNode::Branch(parent, condition, truth) => {
                    let case = &condition.cases[usize::from(*truth)];
                    let next_parent = &clauses[&parent.key()];
                    let next_case = &clauses[&case.key()];
                    if Arc::ptr_eq(&next_parent.0, &parent.0) && Arc::ptr_eq(&next_case.0, &case.0)
                    {
                        clause.clone()
                    } else {
                        Self::both(next_parent, next_case)
                    }
                }
            };
            clauses.insert(clause.key(), next);
        }
        clauses.remove(&self.key()).unwrap()
    }
    fn selection(index: ownership::ApplicationIndex, element: usize) -> Self {
        match index {
            ownership::ApplicationIndex::Constant(index) => {
                if index as usize == element {
                    Self::default()
                } else {
                    Self::impossible()
                }
            }
            ownership::ApplicationIndex::Value(identity,_) => Self::facts(
                [
                    Fact::Minimum(identity, element as u32),
                    Fact::Maximum(identity, element as u32),
                ]
                .into(),
            ),
        }
    }
    pub(super) fn impossible() -> Self {
        Self(Arc::new(ClauseNode::Impossible))
    }
    fn facts(facts: Facts) -> Self {
        if contradictory(&facts) {
            Self::impossible()
        } else {
            Self(Arc::new(ClauseNode::Facts(facts)))
        }
    }
    pub(super) fn both(a: &Self, b: &Self) -> Self {
        match (&*a.0, &*b.0) {
            (ClauseNode::Impossible, _) | (_, ClauseNode::Impossible) => Self::impossible(),
            (ClauseNode::Facts(facts), _) if facts.is_empty() => b.clone(),
            (_, ClauseNode::Facts(facts)) if facts.is_empty() => a.clone(),
            (ClauseNode::Facts(a), ClauseNode::Facts(b)) => {
                Self::facts(a.union(b).copied().collect())
            }
            _ if Arc::ptr_eq(&a.0, &b.0) => a.clone(),
            _ => Self(Arc::new(ClauseNode::Both(a.clone(), b.clone()))),
        }
    }
    fn branch(parent: &Self, condition: &Arc<Condition>, truth: bool) -> Self {
        if matches!(&*parent.0, ClauseNode::Impossible)
            || matches!(
                &*condition.cases[usize::from(truth)].0,
                ClauseNode::Impossible
            )
        {
            return Self::impossible();
        }
        Self(Arc::new(ClauseNode::Branch(
            parent.clone(),
            condition.clone(),
            truth,
        )))
    }
    fn joined_branches(a: &Self, b: &Self) -> Option<Self> {
        match (&*a.0, &*b.0) {
            (ClauseNode::Branch(ap, ac, at), ClauseNode::Branch(bp, bc, bt))
                if Arc::ptr_eq(&ap.0, &bp.0) && Arc::ptr_eq(ac, bc) && at != bt =>
            {
                Some(ap.clone())
            }
            _ => None,
        }
    }
    pub(super) fn either(a: &Self, b: &Self) -> Self {
        if let Some(parent) = Self::joined_branches(a, b) {
            return parent;
        }
        if let ClauseNode::Either(x, y) = &*a.0 {
            if let Some(parent) = Self::joined_branches(y, b) {
                return Self::either(x, &parent);
            }
            if let Some(parent) = Self::joined_branches(x, b) {
                return Self::either(&parent, y);
            }
        }
        if let ClauseNode::Either(x, y) = &*b.0 {
            if let Some(parent) = Self::joined_branches(a, x) {
                return Self::either(&parent, y);
            }
            if let Some(parent) = Self::joined_branches(a, y) {
                return Self::either(x, &parent);
            }
        }
        match (&*a.0, &*b.0) {
            (ClauseNode::Impossible, _) => b.clone(),
            (_, ClauseNode::Impossible) => a.clone(),
            (ClauseNode::Facts(facts), _) if facts.is_empty() => a.clone(),
            (_, ClauseNode::Facts(facts)) if facts.is_empty() => b.clone(),
            (ClauseNode::Facts(left), ClauseNode::Facts(right)) if left.is_subset(right) => {
                a.clone()
            }
            (ClauseNode::Facts(left), ClauseNode::Facts(right)) if right.is_subset(left) => {
                b.clone()
            }
            _ if Arc::ptr_eq(&a.0, &b.0) => a.clone(),
            _ => Self(Arc::new(ClauseNode::Either(a.clone(), b.clone()))),
        }
    }
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    // 按子节点在先的顺序只访问每个共享节点一次，不展开路径的笛卡尔积。
    fn ordered(roots: &[Self]) -> Vec<&Self> {
        let mut pending: Vec<_> = roots.iter().rev().map(|root| (root, false)).collect();
        let mut visited = BTreeSet::new();
        let mut ordered = Vec::new();
        while let Some((clause, ready)) = pending.pop() {
            if visited.contains(&clause.key()) {
                continue;
            }
            if !ready {
                match &*clause.0 {
                    ClauseNode::Both(a, b) | ClauseNode::Either(a, b) => {
                        pending.extend([(clause, true), (b, false), (a, false)]);
                        continue;
                    }
                    ClauseNode::Branch(parent, condition, truth) => {
                        pending.extend([
                            (clause, true),
                            (&condition.cases[usize::from(*truth)], false),
                            (parent, false),
                        ]);
                        continue;
                    }
                    _ => {}
                }
            }
            visited.insert(clause.key());
            ordered.push(clause);
        }
        ordered
    }
    // 同一资源来源的路径按事实合取去重，保留每个来源实际成立的完整条件。
    pub(super) fn canonical(&self)->Self{
        fn insert(terms:&mut BTreeSet<Facts>,facts:Facts){
            if terms.iter().any(|old|old.is_subset(&facts)){return;}
            terms.retain(|old|!facts.is_subset(old));terms.insert(facts);
        }
        fn facts(value:&Facts)->Option<Facts>{
            if contradictory(value){return None;}
            let mut next=value.iter().filter(|fact|!matches!(fact,Fact::Minimum(..)|Fact::Maximum(..))).copied().collect::<Facts>();
            next.extend(minimums(value).into_iter().filter(|(_,n)|*n!=0).map(|(id,n)|Fact::Minimum(id,n)));
            next.extend(maximums(value).into_iter().filter(|(_,n)|*n!=u32::MAX).map(|(id,n)|Fact::Maximum(id,n)));
            Some(next)
        }
        let mut values:BTreeMap<usize,BTreeSet<Facts>>=BTreeMap::new();
        for clause in Self::ordered(std::slice::from_ref(self)){
            let next=match &*clause.0{
                ClauseNode::Impossible=>BTreeSet::new(),
                ClauseNode::Facts(value)=>facts(value).into_iter().collect(),
                ClauseNode::Either(a,b)=>{
                    let mut terms=values[&a.key()].clone();for value in &values[&b.key()]{insert(&mut terms,value.clone());}terms
                },
                node=>{
                    let (a,b)=match node{ClauseNode::Both(a,b)=>(a,b),ClauseNode::Branch(parent,condition,truth)=>(parent,&condition.cases[usize::from(*truth)]),_=>unreachable!()};
                    let mut terms=BTreeSet::new();
                    for left in &values[&a.key()]{for right in &values[&b.key()]{
                        if let Some(value)=facts(&left.union(right).copied().collect()){insert(&mut terms,value);}
                    }}terms
                },
            };values.insert(clause.key(),next);
        }
        values.remove(&self.key()).unwrap().into_iter().fold(Self::impossible(),|all,facts|Self::either(&all,&Self::facts(facts)))
    }
    fn required(&self) -> Option<Facts> {
        self.required_in(&Facts::new())
    }
    fn required_in(&self, environment: &Facts) -> Option<Facts> {
        let mut environment = environment.clone();
        loop {
            let mut facts: BTreeMap<usize, Option<Facts>> = BTreeMap::new();
            let mut pending = vec![self];
            while let Some(clause) = pending.pop() {
                if facts.contains_key(&clause.key()) {
                    continue;
                }
                let (left, right, alternative) = match &*clause.0 {
                    ClauseNode::Impossible => {
                        facts.insert(clause.key(), None);
                        continue;
                    }
                    ClauseNode::Facts(required) => {
                        facts.insert(clause.key(), combine_facts(required, &environment));
                        continue;
                    }
                    ClauseNode::Both(a, b) => (a, b, false),
                    ClauseNode::Branch(parent, condition, truth) => {
                        (parent, &condition.cases[usize::from(*truth)], false)
                    }
                    ClauseNode::Either(a, b) => (a, b, true),
                };
                let Some(a) = facts.get(&left.key()) else {
                    pending.extend([clause, left]);
                    continue;
                };
                // 合取的前提已不可能，另一侧没有本次查询的消费者。
                if !alternative && a.is_none() {
                    facts.insert(clause.key(), None);
                    continue;
                }
                let Some(b) = facts.get(&right.key()) else {
                    pending.extend([clause, right]);
                    continue;
                };
                let required = if alternative {
                    either_facts(a.clone(), b.clone())
                } else {
                    both_facts(a, b)
                };
                facts.insert(clause.key(), required);
            }
            let required = facts.remove(&self.key()).unwrap()?;
            if required == environment {
                return Some(required);
            }
            environment = required;
        }
    }
    pub(super) fn possible(&self) -> bool {
        self.required().is_some()
    }
}
fn contradictory(facts: &Facts) -> bool {
    if facts.iter().any(|fact| {
        matches!(fact, Fact::Boolean(id, truth) if facts.contains(&Fact::Boolean(*id, !truth)))
    }) {
        return true;
    }
    if facts.iter().any(|fact|match *fact{
        Fact::NonZero(id)=>facts.contains(&Fact::Zero(id)),
        Fact::Index(id,d)=>facts.contains(&Fact::Outside(id,d)),
        Fact::Below(a,b,strict,signed)=>{(a==b&&strict)||facts.contains(&Fact::Below(b,a,!strict,signed))||(strict&&facts.contains(&Fact::equal(a,b)))},
        _=>false,
    }){return true;}
    let lower = minimums(facts);
    maximums(facts)
        .iter()
        .any(|(id, upper)| lower.get(id).is_some_and(|lower| lower > upper))
}
fn both_facts(a: &Option<Facts>, b: &Option<Facts>) -> Option<Facts> {
    combine_facts(a.as_ref()?, b.as_ref()?)
}
fn combine_facts(a: &Facts, b: &Facts) -> Option<Facts> {
    let facts: Facts = a.union(b).copied().collect();
    if contradictory(&facts) {
        None
    } else {
        Some(facts)
    }
}
fn either_facts(a: Option<Facts>, b: Option<Facts>) -> Option<Facts> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let left = equality_roots(&a);
            let right = equality_roots(&b);
            let a = equivalent_facts(a, &left);
            let b = equivalent_facts(b, &right);
            let mut common: Facts = a.intersection(&b).copied().collect();
            let identities: BTreeSet<_> = left.keys().chain(right.keys()).copied().collect();
            let mut groups: BTreeMap<_, Vec<ValueIdentity>> = BTreeMap::new();
            for identity in identities {
                groups
                    .entry((
                        left.get(&identity).copied().unwrap_or(identity),
                        right.get(&identity).copied().unwrap_or(identity),
                    ))
                    .or_default()
                    .push(identity);
            }
            for group in groups.values() {
                for other in group.iter().skip(1) {
                    common.insert(Fact::equal(group[0], *other));
                }
            }
            let upper_a = maximums(&a);
            let upper_b = maximums(&b);
            for (identity, upper) in upper_a {
                if let Some(other) = upper_b.get(&identity) {
                    common.insert(Fact::Maximum(identity, upper.max(*other)));
                }
            }
            let a = minimums(&a);
            let b = minimums(&b);
            for (identity, lower) in a {
                if let Some(other) = b.get(&identity) {
                    common.insert(Fact::Minimum(identity, lower.min(*other)));
                }
            }
            Some(common)
        }
        (a, None) | (None, a) => a,
    }
}
fn minimums(facts: &Facts) -> BTreeMap<ValueIdentity, u32> {
    let mut result: BTreeMap<ValueIdentity, u32> = BTreeMap::new();
    for fact in facts {
        if let Fact::Minimum(identity, lower) = fact {
            result
                .entry(*identity)
                .and_modify(|v| *v = (*v).max(*lower))
                .or_insert(*lower);
        }
    }
    result
}
fn maximums(facts: &Facts) -> BTreeMap<ValueIdentity, u32> {
    let mut result: BTreeMap<ValueIdentity, u32> = BTreeMap::new();
    for fact in facts {
        if let Fact::Maximum(identity, upper) = fact {
            result
                .entry(*identity)
                .and_modify(|v| *v = (*v).min(*upper))
                .or_insert(*upper);
        }
    }
    result
}
impl PartialEq for Condition {
    fn eq(&self,other:&Self)->bool { self.cmp(other)==std::cmp::Ordering::Equal }
}
impl Eq for Condition {}
impl PartialOrd for Condition {
    fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}
}
impl Ord for Condition {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        use crate::module::codec::Encode;
        Proof::condition(self.clone()).bytes().cmp(&Proof::condition(other.clone()).bytes())
    }
}
impl crate::module::codec::Encode for Condition {
    fn encode(&self,w:&mut crate::module::codec::Writer){crate::module::codec::Encode::encode(&Proof::condition(self.clone()),w);}
}
impl crate::module::codec::Decode for Condition {
    fn decode(r:&mut crate::module::codec::Reader<'_>)->crate::module::codec::Result<Self>{
        let proof=<Proof as crate::module::codec::Decode>::decode(r)?;
        proof.conditions.into_iter().next().flatten().map(|value|*value).ok_or_else(||"模块资源位置缺少条件合同".into())
    }
}
impl Condition {
    pub(super) fn guard(path:Clause)->Self{Self{cases:[path.negate(),path]}}
    pub(super) fn guard_at_call(&self,bindings:&BTreeMap<ValueIdentity,Proof>)->Self{
        // 先投影真实输入关系，再求补集；不可用的私有布尔不能成为接口条件。
        let value=self.at_call(bindings);Self::guard(value.cases[1].clone())
    }
    pub(super) fn covers(&self,other:&Self)->bool{!Clause::both(&self.cases[0],&other.cases[1]).possible()}
    pub(super) fn overlaps(&self,other:&Self)->bool{Clause::both(&self.cases[1],&other.cases[1]).possible()}
    pub(super) fn active(&self)->bool{self.cases[1].possible()}
    pub(super) fn with_aliases(&self,aliases:&Clause)->Self{
        let common=aliases.required();
        Self{cases:self.cases.each_ref().map(|case|match Clause::both(case,aliases).required(){
            Some(facts)=>Clause::both(case,&Clause::facts(facts.difference(common.as_ref().unwrap()).copied().collect())),
            None=>Clause::impossible(),
        })}
    }
    pub(super) fn impossible(&self)->bool{self.cases.iter().all(|case|!case.possible())}
    pub(super) fn paths(yes:Clause,no:Clause)->Self{Self{cases:[no,yes]}}
    pub(super) fn unconstrained(&self)->bool{self.cases.iter().all(|case|matches!(&*case.0,ClauseNode::Facts(facts)if facts.is_empty()))}
    pub(super) fn exclusive(&self)->bool{!Clause::both(&self.cases[0],&self.cases[1]).possible()}
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,Proof>)->Self{
        Proof::condition(self.clone()).at_call(bindings).component_condition(0).unwrap().clone()
    }
    pub(super) fn output_bound(&self,bindings:&OutputBindings)->Self{Self{cases:self.cases.each_ref().map(|case|case.output_bound(bindings))}}
    pub(super) fn assume(&self,path:&Clause,truth:bool)->Clause{Clause::both(path,&self.cases[usize::from(truth)])}
    pub(super) fn truth_in(&self,path:&Clause)->Option<bool>{
        if !self.assume(path,false).possible(){Some(true)}else if !self.assume(path,true).possible(){Some(false)}else{None}
    }
    pub(super) fn mentions(&self,identity:ValueIdentity)->bool{Clause::ordered(&self.cases).iter().any(|clause|matches!(&*clause.0,ClauseNode::Facts(facts)if facts.iter().any(|fact|match fact{Fact::Equal(a,b)|Fact::Below(a,b,..)=>*a==identity||*b==identity,Fact::Index(a,_)|Fact::Outside(a,_)|Fact::Zero(a)|Fact::Minimum(a,_)|Fact::Maximum(a,_)|Fact::NonZero(a)|Fact::Boolean(a,_)=>*a==identity})))}
    pub(super) fn constant(truth:bool)->Self{let mut value=Self::default();value.cases[usize::from(!truth)]=Clause::impossible();value}
    pub(super) fn selected(index:ownership::ApplicationIndex,element:usize)->Self{
        let ownership::ApplicationIndex::Value(id,_)=index else{return Self::constant(matches!(index,ownership::ApplicationIndex::Constant(i)if i as usize==element))};
        let n=element as u32;let mut no=Clause::impossible();
        if n!=0{no=Clause::either(&no,&Clause::facts([Fact::Maximum(id,n-1)].into()));}
        if n!=u32::MAX{no=Clause::either(&no,&Clause::facts([Fact::Minimum(id,n+1)].into()));}
        Self{cases:[no,Clause::selection(index,element)]}
    }
    pub(super) fn logical(self,other:Self,any:bool)->Self{if any{Self{cases:[Clause::both(&self.cases[0],&other.cases[0]),Clause::either(&self.cases[1],&other.cases[1])]}}else{Self{cases:[Clause::either(&self.cases[0],&other.cases[0]),Clause::both(&self.cases[1],&other.cases[1])]}}}
    pub(super) fn boolean_constant(&self) -> Option<bool> {
        match self
            .cases
            .each_ref()
            .map(|case| matches!(&*case.0, ClauseNode::Impossible))
        {
            [true, false] => Some(true),
            [false, true] => Some(false),
            _ => None,
        }
    }
    fn common(a: Self, b: Self) -> Self {
        let [af, at] = a.cases;
        let [bf, bt] = b.cases;
        Self {
            cases: [Clause::either(&af, &bf), Clause::either(&at, &bt)],
        }
    }
    pub fn of(value: &Value) -> Self {
        Self::component(value, 0)
    }
    fn component(value: &Value, component: usize) -> Self {
        let mut condition = value
            .proof
            .component_condition(component)
            .cloned()
            .unwrap_or_default();
        for (truth, case) in condition.cases.iter_mut().enumerate() {
            *case = Clause::both(
                case,
                &Clause::facts(
                    [Fact::Boolean(
                        value.component_identity(component),
                        truth != 0,
                    )]
                    .into(),
                ),
            );
        }
        // 常量排除相反结果，但不抹掉成立结果已经持有的范围与长度事实。
        if let Some(bits) = &value.constant {
            condition.cases[usize::from(bits[component] == 0)] = Clause::impossible();
        }
        condition
    }
    pub fn negate(mut self) -> Self {
        self.cases.swap(0, 1);
        self
    }
    fn branch(c: Self, a: Self, b: Self) -> Self {
        Self {
            cases: std::array::from_fn(|truth| {
                Clause::either(
                    &Clause::both(&c.cases[1], &a.cases[truth]),
                    &Clause::both(&c.cases[0], &b.cases[truth]),
                )
            }),
        }
    }
    pub fn comparison(op: Comparison, a: &Value, b: &Value) -> Self {
        if a.ty == Ty::scalar(Scalar::Bool) && b.ty == a.ty {
            let a = Self::of(a);
            let b = Self::of(b);
            let equal = Self {
                cases: [
                    Clause::either(
                        &Clause::both(&a.cases[0], &b.cases[1]),
                        &Clause::both(&a.cases[1], &b.cases[0]),
                    ),
                    Clause::either(
                        &Clause::both(&a.cases[0], &b.cases[0]),
                        &Clause::both(&a.cases[1], &b.cases[1]),
                    ),
                ],
            };
            return if op == Comparison::NotEqual {
                equal.negate()
            } else {
                equal
            };
        }
        let signed = a.ty == Ty::scalar(Scalar::I32);
        if (!signed && a.ty != Ty::scalar(Scalar::U32)) || b.ty != a.ty {
            return Self::default();
        }
        if op==Comparison::NotEqual{return Self::comparison(Comparison::Equal,a,b).negate();}
        let left = a.component_ranges(0);
        let right = b.component_ranges(0);
        let same = a.same_component(0, b, 0);
        let mut facts = [Facts::new(), Facts::new()];
        let mut possible = [false; 2];
        for (truth, facts) in facts.iter_mut().enumerate() {
            let op = if truth != 0 {
                op
            } else {
                match op {
                    Comparison::Less => Comparison::GreaterEqual,
                    Comparison::Greater => Comparison::LessEqual,
                    Comparison::LessEqual => Comparison::Greater,
                    Comparison::GreaterEqual => Comparison::Less,
                    Comparison::Equal => Comparison::NotEqual,
                    Comparison::NotEqual => Comparison::Equal,
                }
            };
            possible[truth] = left.0.iter().any(|&(al, ah)| {
                right.0.iter().any(|&(bl, bh)| {
                    if same {
                        return al.max(bl) <= ah.min(bh)
                            && matches!(
                                op,
                                Comparison::Equal
                                    | Comparison::LessEqual
                                    | Comparison::GreaterEqual
                            );
                    }
                    match op {
                        Comparison::Less => al < bh,
                        Comparison::Greater => ah > bl,
                        Comparison::LessEqual => al <= bh,
                        Comparison::GreaterEqual => ah >= bl,
                        Comparison::Equal => al.max(bl) <= ah.min(bh),
                        Comparison::NotEqual => al != bh || ah != bl,
                    }
                })
            });
            if !possible[truth] {
                continue;
            }
            match op {
                Comparison::Less => constrain(facts, a, b, true),
                Comparison::Greater => constrain(facts, b, a, true),
                Comparison::LessEqual => constrain(facts, a, b, false),
                Comparison::GreaterEqual => constrain(facts, b, a, false),
                Comparison::Equal => {
                    facts.insert(Fact::equal(a.identity(), b.identity()));
                    facts.extend(
                        b.proof
                            .domains(0)
                            .iter()
                            .map(|domain| Fact::Index(a.identity(), *domain)),
                    );
                    facts.extend(
                        a.proof
                            .domains(0)
                            .iter()
                            .map(|domain| Fact::Index(b.identity(), *domain)),
                    );
                    for (value, lower) in [(a, b.component_range(0).0), (b, a.component_range(0).0)]
                    {
                        if lower != 0 {
                            facts.insert(Fact::Minimum(value.identity(), lower));
                        }
                    }
                    for (value, upper) in [(a, b.component_range(0).1), (b, a.component_range(0).1)]
                    {
                        if upper != u32::MAX {
                            facts.insert(Fact::Maximum(value.identity(), upper));
                        }
                    }
                    for (value, other) in [(a, b), (b, a)] {
                        if other.is_nonzero() {
                            facts.insert(Fact::NonZero(value.identity()));
                        }
                    }
                }
                Comparison::NotEqual => {
                    for (value, other) in [(a, b), (b, a)] {
                        if other.constant.as_deref() == Some(&[0]) {
                            facts.insert(if signed {
                                Fact::NonZero(value.identity())
                            } else {
                                Fact::Minimum(value.identity(), 1)
                            });
                        }
                    }
                }
            }
        }
        let mut cases=std::array::from_fn(|i|if possible[i]{Clause::facts(std::mem::take(&mut facts[i]))}else{Clause::impossible()});
        if op==Comparison::Equal{cases[0]=Clause::both(&cases[0],&Clause::either(&Self::comparison(Comparison::Less,a,b).cases[1],&Self::comparison(Comparison::Greater,a,b).cases[1]));}
        Self{cases}
    }
}
fn constrain(facts: &mut Facts, index: &Value, limit: &Value, strict: bool) {
    constrain_proofs(
        facts,
        (index.identity(), &Proof::capture(index)),
        (limit.identity(), &Proof::capture(limit)),
        strict,
        index.ty == Ty::scalar(Scalar::I32),
    );
}
fn constrain_proofs(
    facts: &mut Facts,
    (index_id, index): (ValueIdentity, &Proof),
    (limit_id, limit): (ValueIdentity, &Proof),
    strict: bool,
    signed: bool,
) {
    facts.insert(Fact::Below(index_id, limit_id, strict, signed));
    if let Some(upper) = limit.range(0).1.checked_sub(u32::from(strict)) {
        if upper != u32::MAX {
            facts.insert(Fact::Maximum(index_id, upper));
        }
    }
    if !signed {
        facts.extend(
            indices_below(&limit.domains(0), strict)
                .into_iter()
                .map(|domain| Fact::Index(index_id, domain)),
        );
    }
    if let Some(lower) = index
        .range(0)
        .0
        .checked_add(u32::from(strict))
        .filter(|&lower| lower != 0)
    {
        facts.insert(Fact::Minimum(limit_id, lower));
    }
}
/// 数值区间逐分量计算；资源索引域只沿已证明的无符号关系传递。
pub(super) fn arithmetic(op: &str, a: &Value, b: &Value) -> Proof {
    let count = a.regs.len().max(b.regs.len());
    let mut proof = Proof {
        ranges: if matches!(a.ty.components[0].0, Scalar::U32 | Scalar::I32) {
            (0..count)
                .map(|i| {
                    let ai = i % a.regs.len();
                    let bi = i % b.regs.len();
                    let left = a.component_ranges(ai);
                    let mut right = b.component_ranges(bi);
                    let signed = a.ty.components[0].0 == Scalar::I32;
                    let bias = if signed { 0x8000_0000 } else { 0 };
                    if matches!(op, "-" | "^" | "&" | "|" | "/" | "%" | "min" | "max")
                        && a.same_component(ai, b, bi)
                    {
                        // 同值快照只取共同可达片段；空路径仍为空。
                        let shared = left.combine(&right, |(al, ah), (bl, bh)| {
                            IntegerRanges::interval(al.max(bl), ah.min(bh))
                        });
                        return match op {
                            "&" | "|" | "min" | "max" => shared,
                            _ => {
                                // 整数除余在进入范围计算前已由 binary 证明除数非零。
                                let value = if op == "/" { bias ^ 1 } else { bias };
                                shared.map(|_| IntegerRanges::interval(value, value))
                            }
                        };
                    }
                    if matches!(op, "<<" | ">>") {
                        right = right.shift_counts(bias);
                    }
                    left.combine(&right, |left, right| {
                        let bitwise = |operation: fn(u32, u32) -> u32| {
                            let (lower, upper) = bitwise_range(left, right, bias, operation);
                            IntegerRanges::interval(lower, upper)
                        };
                        match op {
                            "&" => bitwise(|a, b| a & b),
                            "|" => bitwise(|a, b| a | b),
                            "^" => bitwise(|a, b| a ^ b),
                            _ if signed => signed_range(op, left, right),
                            _ => unsigned_range(op, left, right),
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        },
        nonzero: if matches!(a.ty.components[0].0, Scalar::U32 | Scalar::I32) && op == "|" {
            (0..count)
                .map(|i| {
                    a.component_nonzero(i % a.regs.len()) || b.component_nonzero(i % b.regs.len())
                })
                .collect()
        } else {
            Vec::new()
        },
        domains: if a.ty.components[0].0 == Scalar::U32 {
            (0..count)
                .map(|i| arithmetic_domains(op, a, b, i))
                .collect()
        } else {
            Vec::new()
        },
        ..Proof::default()
    };
    if matches!(a.ty.components[0].0, Scalar::U32 | Scalar::I32) {
        for component in 0..count {
            if let Some((value, source)) = arithmetic_source(op, a, b, component) {
                let mut exact = value.projected_proof(&[source]);
                // 继承操作数身份，但仍与运算的可达片段相交，保留空路径。
                exact.ranges[0] = exact
                    .range_set(0)
                    .combine(&proof.range_set(component), |(al, ah), (bl, bh)| {
                        IntegerRanges::interval(al.max(bl), ah.min(bh))
                    });
                exact.domains[0].extend(proof.domains(component));
                proof.assign(&[component], count, exact);
            }
        }
    }
    proof
}
fn arithmetic_source<'a>(
    op: &str,
    a: &'a Value,
    b: &'a Value,
    component: usize,
) -> Option<(&'a Value, usize)> {
    let ai = component % a.regs.len();
    let bi = component % b.regs.len();
    if matches!(op, "&" | "|" | "min" | "max") && a.same_component(ai, b, bi) {
        return Some((a, ai));
    }
    if matches!(op, "min" | "max") {
        let (al, ah) = a.component_range(ai);
        let (bl, bh) = b.component_range(bi);
        return if ah <= bl {
            Some(if op == "min" { (a, ai) } else { (b, bi) })
        } else if bh <= al {
            Some(if op == "min" { (b, bi) } else { (a, ai) })
        } else {
            None
        };
    }
    if matches!(op, "<<" | ">>") {
        return (b.component_ranges(bi).shift_counts(0).hull() == (0, 0)).then_some((a, ai));
    }
    let (right, left) = match op {
        "+" | "|" | "^" => (0, Some(0)),
        "*" => (1, Some(1)),
        "-" => (0, None),
        "/" => (1, None),
        "&" => (u32::MAX, Some(u32::MAX)),
        _ => return None,
    };
    let bits = |value: &Value, index| {
        let (lower, upper) = value.component_range(index);
        let bias = if value.ty.components[index].0 == Scalar::I32 {
            0x8000_0000
        } else {
            0
        };
        (lower == upper).then_some(lower ^ bias)
    };
    if bits(b, bi) == Some(right) {
        Some((a, ai))
    } else if left.is_some_and(|neutral| bits(a, ai) == Some(neutral)) {
        Some((b, bi))
    } else {
        None
    }
}
fn arithmetic_domains(op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let ai = component % a.regs.len();
    let bi = component % b.regs.len();
    let left = a.proof.domains(ai);
    let right = b.proof.domains(bi);
    if matches!(op, "min" | "max") {
        let left = indices_below(&left, false);
        let right = indices_below(&right, false);
        return if op == "min" {
            left.union(&right).copied().collect()
        } else {
            left.intersection(&right).copied().collect()
        };
    }
    if matches!(op, "&" | "|") && a.same_component(ai, b, bi) {
        return left.union(&right).copied().collect();
    }
    if op == "&" {
        let mut domains = indices_below(&left, false);
        domains.extend(indices_below(&right, false));
        return domains;
    }
    if op == "%" && b.component_range(bi).0 != 0 {
        let mut domains = indices_below(&left, false);
        domains.extend(indices_below(&right, true));
        return domains;
    }
    if op == "/" && b.component_range(bi).0 != 0 {
        let strict = a.component_nonzero(ai) && b.component_range(bi).0 > 1;
        return indices_below(&left, strict);
    }
    if op == ">>" {
        let strict = a.component_nonzero(ai) && !b.component_ranges(bi).shift_counts(0).contains(0);
        return indices_below(&left, strict);
    }
    let Some(constant) = b.constant.as_ref().map(|values| values[bi]) else {
        return Domains::new();
    };
    let strict = match op {
        "-" if a.component_range(ai).0 >= constant => constant != 0,
        _ => return Domains::new(),
    };
    indices_below(&left, strict)
}
pub(super) fn unary(op: UnaryOp, value: &Value) -> Proof {
    if op == UnaryOp::LogicalNot {
        return Proof::condition(Condition::of(value).negate());
    }
    let mut proof = Proof::default();
    if value.ty.components[0].0 == Scalar::U32 {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => IntegerRanges::interval(!upper, !lower),
                    UnaryOp::UnsignedNegate => {
                        integer_result_range((-i128::from(upper), -i128::from(lower)), 0)
                    }
                    _ => IntegerRanges::default(),
                })
            })
            .collect();
    } else if value.ty.components[0].0 == Scalar::I32 {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => IntegerRanges::interval(!upper, !lower),
                    UnaryOp::SignedNegate => integer_result_range(
                        (
                            -i128::from((upper ^ 0x8000_0000) as i32),
                            -i128::from((lower ^ 0x8000_0000) as i32),
                        ),
                        0x8000_0000,
                    ),
                    _ => IntegerRanges::default(),
                })
            })
            .collect();
    }
    proof
}
fn bitwise_range(
    (al, ah): (u32, u32),
    (bl, bh): (u32, u32),
    bias: u32,
    operation: fn(u32, u32) -> u32,
) -> (u32, u32) {
    // 从高位向低位交付可达的输出前缀。四个松弛位分别表示两个输入已经
    // 严格大于下界／小于上界；相同状态的后缀选择相同，只需保留前缀极值。
    let mut prefixes = [None; 16];
    prefixes[0] = Some((0u32, 0u32));
    for bit in (0..u32::BITS).rev() {
        let (al, ah, bl, bh) = (
            (al >> bit) & 1,
            (ah >> bit) & 1,
            (bl >> bit) & 1,
            (bh >> bit) & 1,
        );
        let sign = (bias >> bit) & 1;
        let mut next: [Option<(u32, u32)>; 16] = [None; 16];
        for (loose, prefix) in prefixes.into_iter().enumerate() {
            let Some((lower, upper)) = prefix else {
                continue;
            };
            for a in 0..=1 {
                for b in 0..=1 {
                    if (loose & 1 == 0 && a < al)
                        || (loose & 2 == 0 && a > ah)
                        || (loose & 4 == 0 && b < bl)
                        || (loose & 8 == 0 && b > bh)
                    {
                        continue;
                    }
                    let state = loose
                        | usize::from(a > al)
                        | (usize::from(a < ah) << 1)
                        | (usize::from(b > bl) << 2)
                        | (usize::from(b < bh) << 3);
                    // i32 的端点按翻转符号位排序；运算消费真实位，再回到相同顺序。
                    let value = operation(a ^ sign, b ^ sign) ^ sign;
                    let (lower, upper) = ((lower << 1) | value, (upper << 1) | value);
                    next[state] = Some(match next[state] {
                        Some((old_lower, old_upper)) => {
                            (old_lower.min(lower), old_upper.max(upper))
                        }
                        None => (lower, upper),
                    });
                }
            }
        }
        prefixes = next;
    }
    prefixes
        .into_iter()
        .flatten()
        .fold((u32::MAX, 0), |(lower, upper), (a, b)| {
            (lower.min(a), upper.max(b))
        })
}
fn signed_range(op: &str, (al, ah): (u32, u32), (bl, bh): (u32, u32)) -> IntegerRanges {
    let decode = |value| i64::from((value ^ 0x8000_0000u32) as i32);
    let (al, ah, bl, bh) = (decode(al), decode(ah), decode(bl), decode(bh));
    let corners = |f: fn(i64, i64) -> i64| {
        let values = [f(al, bl), f(al, bh), f(ah, bl), f(ah, bh)];
        (*values.iter().min().unwrap(), *values.iter().max().unwrap())
    };
    let range = match op {
        "min" => (al.min(bl), ah.min(bh)),
        "max" => (al.max(bl), ah.max(bh)),
        "+" => (al + bl, ah + bh),
        "-" => (al - bh, ah - bl),
        "*" => corners(|a, b| a * b),
        "/" if bl > 0 || bh < 0 => corners(|a, b| a / b),
        "%" if bl > 0 || bh < 0 => {
            let maximum = bl.abs().max(bh.abs()) - 1;
            (al.min(0).max(-maximum), ah.max(0).min(maximum))
        }
        ">>" => corners(|a, b| a >> b),
        "<<" => corners(|a, b| a * (1i64 << b)),
        _ => return IntegerRanges::default(),
    };
    integer_result_range((i128::from(range.0), i128::from(range.1)), 0x8000_0000)
}
fn integer_result_range((lower, upper): (i128, i128), bias: u32) -> IntegerRanges {
    // 按有序周期投影；不足一个完整周期的跨断点结果保留两侧区间。
    let period = 1i128 << 32;
    let offset = i128::from(bias);
    if upper - lower >= period - 1 {
        return IntegerRanges::default();
    }
    let (a, b) = (lower as u32 ^ bias, upper as u32 ^ bias);
    if (lower + offset).div_euclid(period) == (upper + offset).div_euclid(period) {
        IntegerRanges::interval(a, b)
    } else {
        IntegerRanges::from_intervals([(0, b), (a, u32::MAX)])
    }
}
fn unsigned_range(op: &str, (al, ah): (u32, u32), (bl, bh): (u32, u32)) -> IntegerRanges {
    let (al, ah, bl, bh) = (
        i128::from(al),
        i128::from(ah),
        i128::from(bl),
        i128::from(bh),
    );
    let range = match op {
        "min" => (al.min(bl), ah.min(bh)),
        "max" => (al.max(bl), ah.max(bh)),
        "+" => (al + bl, ah + bh),
        "-" => (al - bh, ah - bl),
        "*" => (al * bl, ah * bh),
        "/" if bl > 0 => (al / bh, ah / bl),
        "%" if bl > 0 => (0, ah.min(bh - 1)),
        ">>" => (al >> bh, ah >> bl),
        "<<" => (al << bl, ah << bh),
        _ => return IntegerRanges::default(),
    };
    integer_result_range(range, 0)
}
pub(super) fn indices_below(domains: &Domains, strict: bool) -> Domains {
    domains
        .iter()
        .filter_map(|domain| match *domain {
            IndexDomain::DataLength(binding) if strict => Some(IndexDomain::Data(binding)),
            IndexDomain::TileLength if strict => Some(IndexDomain::Tile),
            IndexDomain::ViewLength(origin) if strict => Some(IndexDomain::View(origin)),
            IndexDomain::ApplicationLength(handle) if strict => {
                Some(IndexDomain::ApplicationData(handle))
            }
            domain @ (IndexDomain::Data(_)
            | IndexDomain::Tile
            | IndexDomain::View(_)
            | IndexDomain::ApplicationData(_)) => Some(domain),
            _ => None,
        })
        .collect()
}
impl Builder<'_> {
    pub(super) fn observe_bounds(&mut self, value: &Value) {
        self.path = Clause::both(
            &self.path,
            &Clause::facts(Proof::capture(value).facts(&value.ty)),
        );
    }
    fn selected_proof(
        &self,
        proof: &Proof,
        source: &Ty,
        ty: &Ty,
        choices: &[Vec<(usize, Clause)>],
        registers: Option<&[Reg]>,
    ) -> Proof {
        let environment = environment_facts(&self.env);
        let mut result = Proof::default();
        for (component, ((scalar, _), candidates)) in ty.components.iter().zip(choices).enumerate()
        {
            let mut selected: Option<Proof> = None;
            for (source_component, condition) in candidates {
                let mut path = Clause::both(&self.path, condition);
                let Some(context) = path.required_in(&environment) else {
                    continue;
                };
                // 同次聚合读取的字段来自同一选择，不能在合流前拆散其身份关系。
                if let Some(registers) = registers {
                    for (register, siblings) in registers.iter().zip(choices) {
                        let mut related = Clause::impossible();
                        for (sibling, selection) in siblings {
                            if selection.required_in(&context).is_none() {
                                continue;
                            }
                            let facts = proof
                                .project(&[*sibling], source.components.len())
                                .with_identity(ValueIdentity::Register(*register))
                                .facts(&Ty::scalar(source.components[*sibling].0));
                            related = Clause::either(
                                &related,
                                &Clause::both(selection, &Clause::facts(facts)),
                            );
                        }
                        path = Clause::both(&path, &related);
                    }
                }
                let mut next = proof.project(&[*source_component], source.components.len());
                next.under_path(&Ty::scalar(*scalar), &path);
                match &mut selected {
                    Some(selected) => selected.join(&next, 1, None),
                    None => selected = Some(next),
                }
            }
            result.assign(
                &[component],
                ty.components.len(),
                selected.unwrap_or_default(),
            );
        }
        result
    }
    pub(super) fn array_value_proof(
        &self,
        proof: &Proof,
        array: &Ty,
        index: ownership::ApplicationIndex,
        registers: &[Reg],
    ) -> Proof {
        let element = array.array.as_ref().unwrap();
        let width = element.components.len();
        let choices: Vec<Vec<_>> = (0..width)
            .map(|component| {
                (0..array.values[0] as usize)
                    .map(|i| (i * width + component, Clause::selection(index, i)))
                    .collect()
            })
            .collect();
        self.selected_proof(proof, array, element, &choices, Some(registers))
    }
    pub(super) fn projected_owned_proof(
        &self,
        place: &Place,
        registers: Option<&[Reg]>,
    ) -> Option<Proof> {
        fn collect(
            position: &Position,
            owner: &[Position],
            path: Clause,
            choices: &mut Vec<(usize, Clause)>,
        ) -> Option<()> {
            if let Some(component) = owner.iter().position(|candidate| candidate == position) {
                choices.push((component, path));
                return Some(());
            }
            let Position::Indexed(elements, index) = position else {
                return None;
            };
            for (i, element) in elements.iter().enumerate() {
                collect(
                    element,
                    owner,
                    Clause::both(&path, &Clause::selection(*index, i)),
                    choices,
                )?;
            }
            Some(())
        }
        let positions = place.owned_positions()?;
        if positions.is_empty() {
            return None;
        }
        for binding in self.env.values() {
            let (proof, owner) = match binding {
                Binding::Owned(owner) => (&owner.proof, owner.place.clone()),
                Binding::OwnedValue(owner) => {
                    let Some(place) = owner.place() else {
                        continue;
                    };
                    (&owner.value.proof, place)
                }
                _ => continue,
            };
            let owner_positions = self.positions(&owner);
            let choices: Option<Vec<_>> = positions
                .iter()
                .map(|position| {
                    let mut choices = Vec::new();
                    collect(position, &owner_positions, Clause::default(), &mut choices)?;
                    Some(choices)
                })
                .collect();
            let Some(choices) = choices else {
                continue;
            };
            return Some(self.selected_proof(proof, owner.ty(), place.ty(), &choices, registers));
        }
        None
    }
    pub(super) fn refine_parallel_domain(&mut self) {
        for binding in self.env.values_mut() {
            if let Binding::Data(view) = binding
                && view.binding == self.domain
            {
                view.length.refine(
                    &Ty::scalar(Scalar::U32),
                    0,
                    Fact::Minimum(ValueIdentity::DataLength(self.domain), 1),
                );
            }
        }
    }
    pub(super) fn dataset_view(&self, binding: usize) -> DatasetView {
        DatasetView {
            binding,
            length: self.program.buffers[binding]
                .count
                .map_or_else(Proof::default, |n| Proof::unsigned(n, n))
                .with_domains(Domains::from([IndexDomain::DataLength(binding)]))
                .with_identity(ValueIdentity::DataLength(binding)),
        }
    }
    pub(super) fn data_length(&mut self, view: &DatasetView) -> Value {
        let r = self.reg(Scalar::U32);
        self.code
            .push(Inst::Uniform(r, 4 * (2 + view.binding as u32)));
        Value {
            proof: view.length.clone(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![r],
            uniform: true,
            constant: None,
        }
    }
    pub(super) fn data_index_proof(&self, binding: usize) -> Proof {
        Proof::unsigned(
            0,
            self.program.buffers[binding]
                .count
                .unwrap_or(u32::MAX)
                .saturating_sub(1),
        )
    }
    pub(super) fn index_in(&self, value: &Value, domain: IndexDomain, minimum: u32) -> bool {
        if value.proof.domains(0).contains(&domain) {
            return true;
        }
        let limit = self.resource_minimum(domain, minimum);
        value
            .component_ranges(0)
            .0
            .iter()
            .all(|&(_, upper)| upper < limit)
    }
    pub(super) fn resource_minimum(&self, domain: IndexDomain, minimum: u32) -> u32 {
        let length = match domain {
            IndexDomain::Fixed(count) => return count,
            IndexDomain::Data(b) | IndexDomain::DataLength(b) => IndexDomain::DataLength(b),
            IndexDomain::Tile | IndexDomain::TileLength => IndexDomain::TileLength,
            IndexDomain::View(id) | IndexDomain::ViewLength(id) => IndexDomain::ViewLength(id),
            IndexDomain::ApplicationData(handle) | IndexDomain::ApplicationLength(handle) => {
                IndexDomain::ApplicationLength(handle)
            }
        };
        self.env.values().fold(minimum, |minimum, binding| {
            let lower = match binding {
                Binding::Value(v) => (0..v.regs.len())
                    .filter(|&i| {
                        v.ty.components[i].0 == Scalar::U32 && v.proof.domains(i).contains(&length)
                    })
                    .map(|i| v.component_range(i).0)
                    .max()
                    .unwrap_or(0),
                Binding::Owned(owner) => (0..owner.place.ty().components.len())
                    .filter(|&i| {
                        owner.place.ty().components[i].0 == Scalar::U32
                            && owner.proof.domains(i).contains(&length)
                    })
                    .map(|i| owner.proof.unsigned_minimum(i))
                    .max()
                    .unwrap_or(0),
                Binding::OwnedValue(owner) => (0..owner.value.ty.components.len())
                    .filter(|i| {
                        owner.value.ty.components[*i].0 == Scalar::U32
                            && owner.value.proof.domains(*i).contains(&length)
                    })
                    .map(|i| owner.value.proof.unsigned_minimum(i))
                    .chain(
                        owner
                            .resources()
                            .iter()
                            .filter(|d| d.length.proof.domains(0).contains(&length))
                            .map(|d| d.length.minimum()),
                    )
                    .max()
                    .unwrap_or(0),
                Binding::Data(DatasetView { length: proof, .. })
                | Binding::Tile(TileView { length: proof, .. })
                    if proof.domains(0).contains(&length) =>
                {
                    proof
                        .range(0)
                        .0
                        .max(u32::from(proof.nonzero.get(0) == Some(&true)))
                }
                Binding::ApplicationData(data)
                    if data.length.proof.domains(0).contains(&length) =>
                {
                    data.length
                        .proof
                        .range(0)
                        .0
                        .max(u32::from(data.length.proof.nonzero.get(0) == Some(&true)))
                }
                _ => 0,
            };
            minimum.max(lower)
        })
    }
    pub(super) fn restore_expression_environment(
        &mut self,
        mut environment: BTreeMap<String, Binding>,
        modified: &BTreeSet<String>,
    ) {
        // 撤回分支内精化；只有未被调用改写的位置能保留首次读取取得的身份。
        let written = effects::modified_owners(&environment, modified);
        for (name, binding) in &mut environment {
            if written.contains(name) {
                continue;
            }
            if let Binding::OwnedValue(owner) = binding
                && let Some(Binding::OwnedValue(current)) = self.env.get(name)
            {
                owner
                    .value
                    .proof
                    .origins
                    .resize_with(owner.value.ty.components.len(), Vec::new);
                for (origin, now) in owner
                    .value
                    .proof
                    .origins
                    .iter_mut()
                    .zip(&current.value.proof.origins)
                {
                    if origin.is_empty() {
                        origin.extend(now.iter().take(1).copied());
                    }
                }
            }
            if let Binding::Owned(owner) = binding {
                if let Some(Binding::Owned(current)) = self.env.get(name) {
                    owner
                        .proof
                        .origins
                        .resize_with(owner.place.ty().components.len(), Vec::new);
                    for (origin, current) in
                        owner.proof.origins.iter_mut().zip(&current.proof.origins)
                    {
                        if origin.is_empty() {
                            origin.extend(current.iter().take(1).copied());
                        }
                    }
                }
            }
        }
        self.env = environment;
    }
    pub(super) fn residency_path(&self)->Clause{Clause::both(&self.path,&Clause::facts(environment_facts(&self.env)))}
    pub(super) fn condition_truth(&self,condition:&Condition,assuming:Option<&Condition>)->Option<bool>{
        let environment=environment_facts(&self.env);let mut path=self.path.clone();if let Some(assuming)=assuming{path=Clause::both(&path,&assuming.cases[1]);}
        if Clause::both(&path,&condition.cases[0]).required_in(&environment).is_none(){Some(true)}else if Clause::both(&path,&condition.cases[1]).required_in(&environment).is_none(){Some(false)}else{None}
    }
    pub(super) fn refine_bounds(&mut self, condition: &Arc<Condition>, truth: bool) {
        self.path = Clause::branch(&self.path, condition, truth);
        let Some(facts) = self.path.required_in(&environment_facts(&self.env)) else {
            return;
        };
        let roots = equality_roots(&facts);
        for binding in self.env.values_mut() {
            match binding {
                Binding::Value(value) => {
                    value.proof.origins = (0..value.regs.len())
                        .map(|i| value.component_origins(i))
                        .collect();
                    value.proof.extend_equalities(&roots);
                }
                Binding::Owned(owner) => owner.proof.extend_equalities(&roots),
                Binding::OwnedValue(owner) => {
                    owner.value.proof.extend_equalities(&roots);
                    owner.resources_mut(&mut |data| data.length.proof.extend_equalities(&roots));
                }
                Binding::Data(view) => view.length.extend_equalities(&roots),
                Binding::ApplicationData(data) => data.length.proof.extend_equalities(&roots),
                Binding::Tile(view) => view.length.extend_equalities(&roots),
                _ => {}
            }
        }
        for fact in &facts {
            let Some(identity) = fact.identity() else {
                continue;
            };
            for binding in self.env.values_mut() {
                match binding {
                    Binding::Value(value) => {
                        for component in 0..value.regs.len() {
                            if value.proof.origins[component].contains(&identity) {
                                value.proof.refine(&value.ty, component, *fact);
                            }
                        }
                    }
                    Binding::Owned(owner) => {
                        owner.place.refine_position(identity, *fact, &roots);
                        for component in 0..owner.proof.origins.len() {
                            if owner.proof.origins[component].contains(&identity) {
                                owner.proof.refine(owner.place.ty(), component, *fact);
                            }
                        }
                    }
                    Binding::OwnedValue(owner) => {
                        for component in 0..owner.value.proof.origins.len() {
                            if owner.value.proof.origins[component].contains(&identity) {
                                owner.value.proof.refine(&owner.value.ty, component, *fact);
                            }
                        }
                        owner.resources_mut(&mut |data| {
                            if data
                                .length
                                .proof
                                .origins
                                .iter()
                                .flatten()
                                .any(|id| *id == identity)
                            {
                                data.length.proof.refine(&data.length.ty, 0, *fact);
                            }
                        });
                    }
                    Binding::ApplicationData(data)
                        if data
                            .length
                            .proof
                            .origins
                            .iter()
                            .flatten()
                            .any(|id| *id == identity) =>
                    {
                        data.length.proof.refine(&data.length.ty, 0, *fact);
                    }
                    Binding::Data(view)
                        if view
                            .length
                            .origins
                            .iter()
                            .flatten()
                            .any(|id| *id == identity) =>
                    {
                        view.length.refine(&Ty::scalar(Scalar::U32), 0, *fact);
                    }
                    Binding::Tile(view)
                        if view
                            .length
                            .origins
                            .iter()
                            .flatten()
                            .any(|id| *id == identity) =>
                    {
                        view.length.refine(&Ty::scalar(Scalar::U32), 0, *fact);
                    }
                    _ => {}
                }
            }
        }
    }
    pub(super) fn read_owned_proof(&mut self, place: &Place, regs: &[Reg]) -> (Proof, bool) {
        for binding in self.env.values_mut() {
            let Some((proof, uniform, owner)) = binding.proof_storage() else {
                continue;
            };
            let Some(components) = place.components_in(&owner) else {
                continue;
            };
            let width = owner.ty().components.len();
            proof.origins.resize_with(width, Vec::new);
            for (component, reg) in components.iter().zip(regs) {
                if proof.origins[*component].is_empty() {
                    proof.origins[*component].push(ValueIdentity::Register(*reg));
                }
            }
            return (proof.project(&components, width), *uniform);
        }
        (
            self.projected_owned_proof(place, Some(regs))
                .unwrap_or_default(),
            false,
        )
    }
    pub(super) fn invalidate_application_contents(&mut self, identity: ValueIdentity) {
        for binding in self.env.values_mut() {
            let Some((proof, uniform, owner)) = binding.proof_storage() else {
                continue;
            };
            if matches!(owner.storage(), Place::ApplicationElement { identity: source, .. } if *source == identity)
            {
                *proof = Proof::default();
                *uniform = false;
            }
        }
    }
    pub(super) fn assign_owned_proof(&mut self, place: &Place, fresh: Proof, uniform: bool) {
        let positions=if place.ty().contains_views(){self.positions(place)}else{Vec::new()};
        let owners:BTreeMap<_,_>=self.env.iter().filter_map(|(name,binding)| {
            let owner=binding.stored_place()?;
            owner.ty().contains_views().then(||(name.clone(),self.positions(&owner)))
        }).collect();
        for (name,binding) in &mut self.env {
            let modified = {
                let Some((proof, state, owner)) = binding.proof_storage() else {
                    continue;
                };
                if let Some(components) = place.components_in(&owner) {
                    proof.assign(&components, owner.ty().components.len(), fresh.clone());
                    *state = uniform;
                    true
                } else if let Place::Indexed { base, .. } = place
                    && let Some(components) = base.components_in(&owner)
                {
                    proof.forget_value(&components, owner.ty().components.len());
                    if let Some(owner)=owners.get(name) {proof.assign_view_borrows(&positions,owner,&fresh);}
                    *state = false;
                    true
                } else if place.overlaps(&owner) {
                    let components: Vec<_> = match (place.storage(), &owner) {
                        (
                            Place::ApplicationElement { words, .. },
                            Place::ApplicationElement { words: root, .. },
                        ) => root
                            .iter()
                            .enumerate()
                            .filter_map(|(i, word)| words.contains(word).then_some(i))
                            .collect(),
                        _ => (0..owner.ty().components.len()).collect(),
                    };
                    proof.forget_value(&components, owner.ty().components.len());
                    if let Some(owner)=owners.get(name) {proof.assign_view_borrows(&positions,owner,&fresh);}
                    *state = false;
                    true
                } else {
                    false
                }
            };
            if modified && let Binding::OwnedValue(owner) = binding {
                owner.value.constant = None;
            }
        }
    }
}

impl crate::module::codec::Encode for IntegerRanges {
    fn encode(&self, w: &mut crate::module::codec::Writer) {
        crate::module::codec::Encode::encode(&self.0, w);
    }
}
impl crate::module::codec::Decode for IntegerRanges {
    fn decode(r: &mut crate::module::codec::Reader<'_>) -> crate::module::codec::Result<Self> {
        Ok(Self(crate::module::codec::Decode::decode(r)?))
    }
}
// 所有逻辑分量及其两个结果共享节点表；编码不受 Arc 的物理共享方式影响。
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum EncodedClause {
    Impossible,
    Facts(Facts),
    Both(usize, usize),
    Either(usize, usize),
}
crate::module::codec::variants!(EncodedClause{0=>Impossible,1=>Facts(facts),2=>Both(a,b),3=>Either(a,b)});
impl crate::module::codec::Encode for Proof {
    fn encode(&self, w: &mut crate::module::codec::Writer) {
        self.origins.encode(w);
        let width = self
            .conditions
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        (width != 0).encode(w);
        if width != 0 {
            // 沿用原条件存在位；旧标量格式为 2，分量载体为 3，双方明确拒绝混用。
            3u8.encode(w);
            let roots: Vec<_> = self.conditions[..width]
                .iter()
                .flatten()
                .flat_map(|condition| condition.cases.iter().cloned())
                .collect();
            let mut nodes = Vec::new();
            let mut unique = BTreeMap::new();
            let mut indices = BTreeMap::new();
            for clause in Clause::ordered(&roots) {
                let node = match &*clause.0 {
                    ClauseNode::Impossible => EncodedClause::Impossible,
                    ClauseNode::Facts(facts) => EncodedClause::Facts(facts.clone()),
                    ClauseNode::Both(a, b) => {
                        EncodedClause::Both(indices[&a.key()], indices[&b.key()])
                    }
                    ClauseNode::Branch(parent, condition, truth) => EncodedClause::Both(
                        indices[&parent.key()],
                        indices[&condition.cases[usize::from(*truth)].key()],
                    ),
                    ClauseNode::Either(a, b) => {
                        EncodedClause::Either(indices[&a.key()], indices[&b.key()])
                    }
                };
                let index = *unique.entry(node.clone()).or_insert_with(|| {
                    let index = nodes.len();
                    nodes.push(node);
                    index
                });
                indices.insert(clause.key(), index);
            }
            nodes.encode(w);
            self.conditions[..width]
                .iter()
                .map(|condition| {
                    condition.as_deref().map(|condition| {
                        condition.cases.each_ref().map(|case| indices[&case.key()])
                    })
                })
                .collect::<Vec<_>>()
                .encode(w);
        }
        self.ranges.encode(w);
        self.nonzero.encode(w);
        self.domains.encode(w);
        self.views.encode(w);
    }
}
impl crate::module::codec::Decode for Proof {
    fn decode(r: &mut crate::module::codec::Reader<'_>) -> crate::module::codec::Result<Self> {
        let origins = crate::module::codec::Decode::decode(r)?;
        let conditions = if bool::decode(r)? {
            if u8::decode(r)? != 3 {
                return Err("模块分量条件合同格式不兼容，需要用同一编译器重新编译".into());
            }
            fn node(nodes: &[Clause], index: usize) -> crate::module::codec::Result<Clause> {
                nodes
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "模块条件节点引用无效".into())
            }
            let mut nodes = Vec::new();
            for encoded in Vec::<EncodedClause>::decode(r)? {
                nodes.push(match encoded {
                    EncodedClause::Impossible => Clause::impossible(),
                    EncodedClause::Facts(facts) => Clause::facts(facts),
                    EncodedClause::Both(a, b) => Clause::both(&node(&nodes, a)?, &node(&nodes, b)?),
                    EncodedClause::Either(a, b) => {
                        Clause::either(&node(&nodes, a)?, &node(&nodes, b)?)
                    }
                });
            }
            Vec::<Option<[usize; 2]>>::decode(r)?
                .into_iter()
                .map(|roots| {
                    roots
                        .map(|[a, b]| {
                            Ok(Box::new(Condition {
                                cases: [node(&nodes, a)?, node(&nodes, b)?],
                            }))
                        })
                        .transpose()
                })
                .collect::<crate::module::codec::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            origins,
            conditions,
            ranges: crate::module::codec::Decode::decode(r)?,
            nonzero: crate::module::codec::Decode::decode(r)?,
            domains: crate::module::codec::Decode::decode(r)?,
            views: crate::module::codec::Decode::decode(r)?,
        })
    }
}
crate::module::codec::variants!(Fact{0=>Equal(a,b),1=>Index(value,domain),2=>Minimum(value,lower),3=>Maximum(value,upper),4=>NonZero(value),5=>Boolean(value,truth),6=>Below(lower,upper,strict,signed),7=>Outside(value,domain),8=>Zero(value)});
