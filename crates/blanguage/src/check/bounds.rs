//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
use super::*;
use std::{borrow::Cow, collections::{HashMap, HashSet}, sync::Arc};

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
    Minimum(ValueIdentity, Scalar, Bits),
    Maximum(ValueIdentity, Scalar, Bits),
    NonZero(ValueIdentity),
    Boolean(ValueIdentity, bool),
    Below(ValueIdentity, ValueIdentity, bool, Scalar),
    Outside(ValueIdentity,IndexDomain),
    Zero(ValueIdentity),
    NotEqual(ValueIdentity, ValueIdentity),
}
type Facts = BTreeSet<Fact>;
// 固定点环境只由本轮持有；节点仅交接环境之外的事实，保持到最后消费。
type QueryFacts<'a> = Arc<Cow<'a, Facts>>;
struct QueryResult<'a> {
    clause: &'a Clause,
    inputs: Option<(usize, usize, bool)>,
    users: usize,
    remaining: usize,
    value: Option<Option<QueryFacts<'a>>>,
}
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
        IndexDomain::Upper(limit, distance) => IndexDomain::Upper(output_identity(limit, bindings), distance),
        IndexDomain::Room(base) => IndexDomain::Room(output_identity(base, bindings)),
        IndexDomain::Sum(left, right) => IndexDomain::Sum(output_identity(left, bindings), output_identity(right, bindings)),
        IndexDomain::Offset(base, amount) => IndexDomain::Offset(output_identity(base, bindings), amount),
        IndexDomain::Shifted(base, limit, distance) => IndexDomain::Shifted(output_identity(base, bindings), output_identity(limit, bindings), distance),
        other => other,
    }
}
fn numeric_domain(domain: &IndexDomain) -> bool {
    matches!(domain, IndexDomain::Upper(..) | IndexDomain::Room(..) | IndexDomain::Sum(..) | IndexDomain::Shifted(..) | IndexDomain::Offset(..))
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
                        Fact::Minimum(_, Scalar::U32, lower) => index_range.0 = index_range.0.max(lower as u32),
                        Fact::Maximum(_, Scalar::U32, upper) => index_range.1 = index_range.1.min(upper as u32),
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
struct IntegerRanges(Vec<(Bits, Bits)>);
impl Default for IntegerRanges {
    fn default() -> Self {
        Self(vec![(0, Bits::MAX)])
    }
}
impl IntegerRanges {
    fn from_intervals(intervals: impl IntoIterator<Item = (Bits, Bits)>) -> Self {
        let mut intervals: Vec<_> = intervals.into_iter().filter(|(a, b)| a <= b).collect();
        intervals.sort_unstable();
        let mut merged: Vec<(Bits, Bits)> = Vec::new();
        for (lower, upper) in intervals {
            if let Some(last) = merged.last_mut()
                && u128::from(lower) <= u128::from(last.1) + 1
            {
                last.1 = last.1.max(upper);
            } else {
                merged.push((lower, upper));
            }
        }
        Self(merged)
    }
    fn interval(lower: Bits, upper: Bits) -> Self {
        Self::from_intervals([(lower, upper)])
    }
    fn hull(&self) -> (Bits, Bits) {
        match (self.0.first(), self.0.last()) {
            (Some(first), Some(last)) => (first.0, last.1),
            _ => (1, 0),
        }
    }
    fn contains(&self, value: Bits) -> bool {
        self.0
            .iter()
            .any(|&(lower, upper)| lower <= value && value <= upper)
    }
    fn union(&self, other: &Self) -> Self {
        Self::from_intervals(self.0.iter().chain(&other.0).copied())
    }
    fn restrict(&self, lower: Bits, upper: Bits) -> Self {
        Self::from_intervals(self.0.iter().map(|&(a, b)| (a.max(lower), b.min(upper))))
    }
    fn without(&self, value: Bits) -> Self {
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
    fn map(&self, mut operation: impl FnMut((Bits, Bits)) -> Self) -> Self {
        Self::from_intervals(self.0.iter().flat_map(|&interval| operation(interval).0))
    }
    fn shift_counts(&self, scalar: Scalar) -> Self {
        let bias = scalar.order_bias();
        self.map(|(lower, upper)| {
            let mask = Bits::from(scalar.bit_width() - 1);
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
        mut operation: impl FnMut((Bits, Bits), (Bits, Bits)) -> Self,
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
        let values=registers.iter().enumerate().flat_map(|(component,reg)|outputs.project(&[component],registers.len()).facts(&Ty::scalar(types[*reg]))).collect();
        let mut aliases=Clause::both(&Clause::facts(values),&Clause::facts(
            self.origins
                .iter()
                .zip(registers)
                .filter(|(_, reg)| types[**reg] == Scalar::Bool || types[**reg].is_integer())
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
        for(component,reg)in registers.iter().enumerate(){if let Some(condition)=outputs.component_condition(component){
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
        let cases = proof.conditions.iter_mut().flatten()
            .flat_map(|condition| condition.cases.iter_mut()).collect::<Vec<_>>();
        let roots = cases.iter().map(|case| (**case).clone()).collect::<Vec<_>>();
        for (case, bound) in cases.into_iter().zip(Clause::output_bounds(&roots, bindings)) {
            *case = bound;
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
                IndexDomain::Upper(limit, distance) => IndexDomain::Upper(identity(limit, bindings)?, distance),
                IndexDomain::Room(base) => IndexDomain::Room(identity(base, bindings)?),
                IndexDomain::Sum(left, right) => IndexDomain::Sum(identity(left, bindings)?, identity(right, bindings)?),
                IndexDomain::Offset(base, amount) => IndexDomain::Offset(identity(base, bindings)?, amount),
                IndexDomain::Shifted(base, limit, distance) => IndexDomain::Shifted(identity(base, bindings)?, identity(limit, bindings)?, distance),
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
                Fact::Below(lower, upper, strict, scalar) => {
                    let Some((lower_id, upper_id)) =
                        identity(lower, bindings).zip(identity(upper, bindings))
                    else {
                        return Clause::facts(Facts::new());
                    };
                    let (lower, upper) = (&bindings[&lower], &bindings[&upper]);
                    let same = lower.origins[0]
                        .iter()
                        .any(|id| upper.origins[0].contains(id));
                    let possible = lower.range_set(0).restrict(0, scalar.bit_mask()).0.iter().any(|&(a, b)| {
                        upper.range_set(0).restrict(0, scalar.bit_mask()).0.iter().any(|&(c, d)| {
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
                        scalar,
                    );
                    return Clause::facts(facts);
                }
                Fact::Equal(a, b) => identity(a, bindings)
                    .zip(identity(b, bindings))
                    .map(|(a, b)| Fact::equal(a, b)),
                Fact::NotEqual(a, b) => {
                    if let Some((left, right)) = bindings.get(&a).zip(bindings.get(&b)) {
                        let same = left.known_identity(0).zip(right.known_identity(0))
                            .is_some_and(|(a, b)| a == b);
                        let booleans = left.component_condition(0).and_then(Condition::boolean_constant)
                            .zip(right.component_condition(0).and_then(Condition::boolean_constant));
                        let (al, ah) = left.range(0);
                        let (bl, bh) = right.range(0);
                        if same || booleans.is_some_and(|(a,b)| a == b)
                            || (al == ah && bl == bh && al == bl) {
                            return Clause::impossible();
                        }
                        if booleans.is_some_and(|(a,b)| a != b) || ah < bl || bh < al {
                            return Clause::default();
                        }
                    }
                    identity(a, bindings).zip(identity(b, bindings))
                        .map(|(a,b)| Fact::NotEqual(a.min(b), a.max(b)))
                },
                Fact::Index(id, d) => identity(id, bindings)
                    .zip(domain(d, bindings))
                    .map(|(id, d)| Fact::Index(id, d)),
                Fact::Minimum(id, scalar, n)=>{
                    if let Some(value)=bindings.get(&id){let range=value.range_set(0).restrict(0,scalar.bit_mask()).hull();if range.0>=n{return Clause::default();}if range.1<n{return Clause::impossible();}}
                    identity(id,bindings).map(|id|Fact::Minimum(id, scalar, n))
                },
                Fact::Maximum(id, scalar, n)=>{
                    if let Some(value)=bindings.get(&id){let range=value.range_set(0).restrict(0,scalar.bit_mask()).hull();if range.1<=n{return Clause::default();}if range.0>n{return Clause::impossible();}}
                    identity(id,bindings).map(|id|Fact::Maximum(id, scalar, n))
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
    pub(super) fn known_value(&self, ty: &Ty) -> Option<Vec<Bits>> {
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
                if !scalar.is_integer() { return None; }
                let bias = scalar.order_bias();
                match self.range_set(component).restrict(0, scalar.bit_mask()).0.as_slice() {
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
    // 当前值只继承回边保持的界，不继承入口常量、相等身份或变体选择。
    pub(super) fn retain_loop_bounds(&mut self, initial: &Self, component: usize, lower: bool, upper: bool) {
        self.ranges.resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
        if lower {
            self.ranges[component] = self.ranges[component].restrict(Bits::from(initial.unsigned_minimum(component)), Bits::from(u32::MAX));
            if initial.unsigned_minimum(component) != 0 { self.mark_nonzero(component); }
        }
        if upper {
            self.ranges[component] = self.ranges[component].restrict(0, Bits::from(initial.unsigned_maximum(component)));
            for domain in indices_below(&initial.domains(component), false) { self.mark_domain(component, domain); }
            if let Some(id) = initial.known_identity(component) { self.mark_domain(component, IndexDomain::Upper(id, 0)); }
        }
    }
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
            ranges: vec![IntegerRanges::interval(Bits::from(minimum), Bits::from(maximum))],
            ..Self::default()
        }
    }
    pub fn with_domains(mut self, mut domains: Domains) -> Self {
        // 存储版本更换资源域，数值仍是实际同一份长度快照。
        domains.extend(self.domains(0).into_iter().filter(numeric_domain));
        self.domains = vec![domains];
        self
    }
    fn below_length(&self, identities: &[ValueIdentity], length: &Self, component: usize) -> bool {
        let bounds = self.domains(0);
        let origins = length.origins.get(component).map(Vec::as_slice).unwrap_or_default();
        if length.domains(component).iter().any(|domain| {
            let IndexDomain::Offset(base, amount) = *domain else { return false; };
            amount != 0 && identities.contains(&base) || bounds.iter().any(|bound| match *bound {
                IndexDomain::Offset(source, offset) => source == base && offset < amount,
                IndexDomain::Upper(limit, distance) => limit == base && (amount != 0 || distance != 0),
                _ => false,
            })
        }) { return true; }
        bounds.iter().any(|bound| match *bound {
            IndexDomain::Upper(limit, distance) => distance != 0 && origins.contains(&limit),
            IndexDomain::Shifted(base, limit, distance) if distance != 0 => length.domains(component).iter().any(|sum| {
                matches!(*sum, IndexDomain::Sum(left, right) if (left == base && right == limit) || (left == limit && right == base))
            }),
            _ => false,
        })
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
    fn range(&self, component: usize) -> (Bits, Bits) {
        self.range_set(component).hull()
    }
    fn range_set(&self, component: usize) -> IntegerRanges {
        self.ranges.get(component).cloned().unwrap_or_default()
    }
    pub fn integer_cast(value: &Value, target: Scalar) -> Self {
        Self {
            ranges: (0..value.regs.len())
                .map(|i| {
                    let source = value.ty.components[i].0;
                    value.component_ranges(i).map(|(lower,upper)| {
                        let decode = |bits| if source.is_signed() {
                            i128::from(source.signed(bits ^ source.order_bias()))
                        } else { i128::from(bits) };
                        integer_result_range((decode(lower),decode(upper)),target.order_bias(),target.bit_width())
                    })
                })
                .collect(),
            nonzero: (0..value.regs.len())
                .map(|i| target.size() >= value.ty.components[i].0.size() && value.component_nonzero(i))
                .collect(),
            // 索引域约束原 u32 位模式；中间 i32 仍不能直接作为索引。
            domains: (0..value.regs.len())
                .map(|i| if target.size() == value.ty.components[i].0.size() { value.proof.domains(i) } else { Domains::new() })
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
    pub(super) fn fact_components(ty: &Ty) -> impl Iterator<Item = usize> + '_ {
        ty.components.iter().enumerate().filter_map(|(component, (scalar, _))| {
            (*scalar == Scalar::Bool || scalar.is_integer()).then_some(component)
        })
    }
    fn facts(&self, ty: &Ty) -> Facts {
        let mut facts = Facts::new();
        for component in Self::fact_components(ty) {
            let scalar = &ty.components[component].0;
            let Some(origins) = self.origins.get(component) else {
                continue;
            };
            for pair in origins.windows(2) {
                facts.insert(Fact::equal(pair[0], pair[1]));
            }
            for &identity in origins {
                // 环境快照只交付已有事实；完整条件在实际分支消费处求解。
                // 不为每个布尔值及其别名重新求解已经持有的整张条件图。
                if *scalar == Scalar::Bool
                    && let Some(condition) = self.component_condition(component)
                {
                    for truth in [false, true] {
                        if matches!(&*condition.cases[usize::from(!truth)].0, ClauseNode::Impossible) {
                            facts.insert(Fact::Boolean(identity, truth));
                        }
                    }
                }
                if scalar.is_integer()
                    && self.nonzero.get(component) == Some(&true)
                {
                    facts.insert(Fact::NonZero(identity));
                }
                if scalar.is_integer() {
                    let (minimum, maximum) = self.range_set(component).restrict(0, scalar.bit_mask()).hull();
                    let zero=scalar.order_bias();
                    if minimum==zero && maximum==zero{facts.insert(Fact::Zero(identity));}
                    if minimum != 0 {
                        facts.insert(Fact::Minimum(identity, *scalar, minimum));
                    }
                    if maximum != scalar.bit_mask() {
                        facts.insert(Fact::Maximum(identity, *scalar, maximum));
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
            self.ranges[component] = self.ranges[component].restrict(0, Bits::from(count)).without(Bits::from(count));
        }
        self.domains
            .resize(self.domains.len().max(component + 1), Domains::new());
        self.domains[component].insert(domain);
    }
    pub(super) fn unsigned_maximum(&self, component: usize) -> u32 { self.range_set(component).restrict(0, Bits::from(u32::MAX)).hull().1 as u32 }
    pub(super) fn unsigned_minimum(&self, component: usize) -> u32 {
        self.range_set(component).restrict(0, Bits::from(u32::MAX)).hull().0
            .max(Bits::from(self.nonzero.get(component) == Some(&true))) as u32
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
            Fact::Minimum(_, scalar, lower)
                if ty.components[component].0 == scalar =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
                self.ranges[component] = self.ranges[component].restrict(lower, scalar.bit_mask());
                if !scalar.is_signed() && lower != 0 {
                    self.mark_nonzero(component);
                }
            }
            Fact::Maximum(_, scalar, upper)
                if ty.components[component].0 == scalar =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), IntegerRanges::default);
                self.ranges[component] = self.ranges[component].restrict(0, upper);
            }
            Fact::Zero(_) if ty.components[component].0.is_integer()=>{
                let zero=ty.components[component].0.order_bias();
                self.ranges.resize_with(self.ranges.len().max(component+1),IntegerRanges::default);
                self.ranges[component]=self.ranges[component].restrict(zero,zero);
            },
            Fact::NonZero(_) if ty.components[component].0.is_integer() => {
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
            Binding::Value(value) if Proof::fact_components(&value.ty).next().is_some() => Proof::capture(value).facts(&value.ty),
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
            | Self::Minimum(id, ..)
            | Self::Maximum(id, ..)
            | Self::NonZero(id)
            | Self::Boolean(id, _) => Some(id),
            Self::Equal(_, _) | Self::NotEqual(_, _) | Self::Below(..) => None,
        }
    }
    fn at(self, id: ValueIdentity) -> Self {
        match self {
            Self::Index(_, domain) => Self::Index(id, domain),
            Self::Outside(_,domain)=>Self::Outside(id,domain),
            Self::Zero(_)=>Self::Zero(id),
            Self::Minimum(_, scalar, bound) => Self::Minimum(id, scalar, bound),
            Self::Maximum(_, scalar, bound) => Self::Maximum(id, scalar, bound),
            Self::NonZero(_) => Self::NonZero(id),
            Self::Boolean(_, truth) => Self::Boolean(id, truth),
            equal @ Self::Equal(_, _) => equal,
            unequal @ Self::NotEqual(_, _) => unequal,
            below @ Self::Below(..) => below,
        }
    }
}
fn equality_roots<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> BTreeMap<ValueIdentity, ValueIdentity> {
    let mut roots = BTreeMap::new();
    extend_equality_roots(&mut roots, facts);
    roots
}
fn extend_equality_roots<'a>(
    roots: &mut BTreeMap<ValueIdentity, ValueIdentity>,
    facts: impl IntoIterator<Item = &'a Fact>,
) {
    // Facts 及其 union 按 Fact 排序；Equal 是首个事实类别。
    for fact in facts.into_iter().take_while(|fact| matches!(fact, Fact::Equal(..))) {
        if let Fact::Equal(a, b) = *fact {
            let a_root = roots.get(&a).copied().unwrap_or(a);
            let b_root = roots.get(&b).copied().unwrap_or(b);
            let root = a_root.min(b_root);
            if a_root != b_root {
                let merged = a_root.max(b_root);
                // 只改被合并组；其成员身份不可能小于该组的最小代表值。
                for (_, current) in roots.range_mut(merged..) {
                    if *current == merged {
                        *current = root;
                    }
                }
            }
            roots.insert(a, root);
            roots.insert(b, root);
        }
    }
}
// environment 在本轮固定点内不变；节点只合并自己实际新增的 Equal。
fn additional_equality_roots<'a>(
    environment: &'a BTreeMap<ValueIdentity, ValueIdentity>,
    additions: &Facts,
) -> Cow<'a, BTreeMap<ValueIdentity, ValueIdentity>> {
    let mut equalities = additions.iter().take_while(|fact| matches!(fact, Fact::Equal(..))).peekable();
    if equalities.peek().is_none() {
        return Cow::Borrowed(environment);
    }
    let mut roots = environment.clone();
    extend_equality_roots(&mut roots, equalities);
    Cow::Owned(roots)
}
fn equivalent_facts<'a>(environment: &Facts, mut facts: QueryFacts<'a>, roots: &Cow<'_, BTreeMap<ValueIdentity, ValueIdentity>>) -> QueryFacts<'a> {
    // 等价组为空时没有别名事实需要派生，不读取无关事实集。
    if roots.is_empty() {
        return facts;
    }
    let mut inferred = Facts::new();
    // 仅将原关系表的借用引用排成同组连续的顺序。
    let mut members: Vec<_> = roots.iter().map(|(id, group)| (group, id)).collect();
    members.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
    // 原根的环境由本轮所有者交接；分支新根继续投影环境。
    let empty = Facts::new();
    let source_environment = match roots {
        Cow::Borrowed(_) => &empty,
        Cow::Owned(_) => environment,
    };
    for fact in source_environment.union(&facts) {
        if let Some(identity) = fact.identity()
            && let Some(root) = roots.get(&identity)
        {
            let begin = members.partition_point(|(group, _)| *group < root);
            let end = members.partition_point(|(group, _)| *group <= root);
            inferred.extend(members[begin..end].iter()
                .filter(|(_, id)| **id != identity)
                .map(|(_, id)| fact.at(**id))
                .filter(|fact| !environment.contains(fact) && !facts.contains(fact)));
        }
    }
    if !inferred.is_empty() {
        Arc::make_mut(&mut facts).to_mut().extend(inferred);
    }
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
            && self.ty.components.first().is_some_and(|(scalar, _)| scalar.is_integer())
            && (0..self.regs.len()).all(|i| self.component_nonzero(i))
    }
    fn component_nonzero(&self, component: usize) -> bool {
        self.ty.components[component].0.is_integer()
            && (self.proof.nonzero.get(component) == Some(&true)
                || !self.component_ranges(component).contains(
                    self.ty.components[component].0.order_bias(),
                )
                || self.constant.as_ref().is_some_and(|v| v[component] != 0))
    }
    pub(super) fn minimum(&self) -> u32 {
        if self.ty != Ty::scalar(Scalar::U32) {
            return 0;
        }
        self.component_range(0).0 as u32
    }
    pub(super) fn maximum(&self) -> u32 {
        if self.ty != Ty::scalar(Scalar::U32) {
            return u32::MAX;
        }
        self.component_range(0).1 as u32
    }
    fn component_range(&self, component: usize) -> (Bits, Bits) {
        self.component_ranges(component).hull()
    }
    fn component_ranges(&self, component: usize) -> IntegerRanges {
        let scalar = self.ty.components[component].0;
        if !scalar.is_integer() {
            return IntegerRanges::default();
        }
        let bias = scalar.order_bias();
        let mut ranges = self.proof.range_set(component).restrict(0, scalar.bit_mask());
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
            Fact::Minimum(_, _, 0)=>Clause::impossible(),
            Fact::Maximum(_, scalar, n) if n==scalar.bit_mask()=>Clause::impossible(),
            Fact::Minimum(id, scalar, n)=>Clause::facts([Fact::Maximum(id, scalar, n-1)].into()),
            Fact::Maximum(id, scalar, n)=>Clause::facts([Fact::Minimum(id, scalar, n+1)].into()),
            Fact::NonZero(id)=>Clause::facts([Fact::Zero(id)].into()),
            Fact::Zero(id)=>Clause::facts([Fact::NonZero(id)].into()),
            Fact::Boolean(id,truth)=>Clause::facts([Fact::Boolean(id,!truth)].into()),
            Fact::Equal(a,b)=>Clause::facts([Fact::NotEqual(a,b)].into()),
            Fact::NotEqual(a,b)=>Clause::facts([Fact::equal(a,b)].into()),
            Fact::Below(a,b,strict,signed)=>Clause::facts([Fact::Below(b,a,!strict,signed)].into()),
            Fact::Index(id,IndexDomain::Fixed(n))=>Clause::facts([Fact::Minimum(id, Scalar::U32, Bits::from(n))].into()),
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
        Self::output_bounds(std::slice::from_ref(self), bindings).pop().unwrap()
    }
    // One renaming owns all source roots and preserves shared condition identity.
    fn output_bounds(roots: &[Self], bindings: &OutputBindings) -> Vec<Self> {
        if bindings.is_empty() {
            return roots.to_vec();
        }
        let identity = |id| output_identity(id, bindings);
        let mut clauses: BTreeMap<usize, Clause> = BTreeMap::new();
        let mut conditions: BTreeMap<usize, Arc<Condition>> = BTreeMap::new();
        let mut pending=roots.iter().rev().map(|root|(root,false)).collect::<Vec<_>>();
        while let Some((clause,ready))=pending.pop(){
            if clauses.contains_key(&clause.key()){continue;}
            if !ready {
                match &*clause.0 {
                    ClauseNode::Both(a,b)|ClauseNode::Either(a,b)=>{
                        pending.extend([(clause,true),(b,false),(a,false)]);
                        continue;
                    }
                    ClauseNode::Branch(parent,condition,_)=>{
                        pending.extend([(clause,true),(&condition.cases[1],false),(&condition.cases[0],false),(parent,false)]);
                        continue;
                    }
                    _=>{}
                }
            }
            let next = match &*clause.0 {
                ClauseNode::Impossible => clause.clone(),
                ClauseNode::Facts(facts) => {
                    let renamed: Facts = facts
                        .iter()
                        .map(|fact| match *fact {
                            Fact::Equal(a, b) => Fact::equal(identity(a), identity(b)),
                            Fact::NotEqual(a, b) => {let (a,b)=(identity(a),identity(b));Fact::NotEqual(a.min(b),a.max(b))},
                            Fact::Index(id, domain) => {
                                Fact::Index(identity(id), output_domain(domain, bindings))
                            }
                            Fact::Minimum(id, scalar, n) => Fact::Minimum(identity(id), scalar, n),
                            Fact::Maximum(id, scalar, n) => Fact::Maximum(identity(id), scalar, n),
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
                    let next_parent=&clauses[&parent.key()];
                    let next_condition=conditions.entry(Arc::as_ptr(condition) as usize).or_insert_with(||{
                        let cases=std::array::from_fn(|i|clauses[&condition.cases[i].key()].clone());
                        if (0..2).all(|i|Arc::ptr_eq(&cases[i].0,&condition.cases[i].0)){
                            condition.clone()
                        }else{Arc::new(Condition{cases})}
                    });
                    if Arc::ptr_eq(&next_parent.0,&parent.0)&&Arc::ptr_eq(next_condition,condition){
                        clause.clone()
                    }else{Self::branch(next_parent,next_condition,*truth)}
                }
            };
            clauses.insert(clause.key(), next);
        }
        roots.iter().map(|root| clauses[&root.key()].clone()).collect()
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
                    Fact::Minimum(identity, Scalar::U32, element as Bits),
                    Fact::Maximum(identity, Scalar::U32, element as Bits),
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
    fn query_inputs(&self) -> Option<(&Self, &Self, bool)> {
        match &*self.0 {
            ClauseNode::Both(a, b) => Some((a, b, false)),
            ClauseNode::Branch(parent, condition, truth) =>
                Some((parent, &condition.cases[usize::from(*truth)], false)),
            ClauseNode::Either(a, b) => Some((a, b, true)),
            _ => None,
        }
    }
    fn take_query<'a>(results: &mut [QueryResult<'a>], index: usize) -> Option<QueryFacts<'a>> {
        let result = &mut results[index];
        result.remaining -= 1;
        if result.remaining == 0 {
            result.value.take().unwrap()
        } else {
            result.value.as_ref().unwrap().clone()
        }
    }
    // 未求值的右侧也归还自己的输入使用；已经求值的节点已经消费过输入。
    fn abandon_query<'a>(results: &mut [QueryResult<'a>], index: usize) {
        let mut pending = vec![index];
        while let Some(index) = pending.pop() {
            let result = &mut results[index];
            result.remaining -= 1;
            if result.remaining == 0 {
                if result.value.take().is_none()
                    && let Some((left, right, _)) = result.inputs
                {
                    pending.extend([left, right]);
                }
            }
        }
    }
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    // 按子节点在先的顺序只访问每个共享节点一次，不展开路径的笛卡尔积。
    fn ordered(roots: &[Self]) -> Vec<&Self> {
        let mut pending: Vec<_> = roots.iter().rev().map(|root| (root, false)).collect();
        let mut visited = HashSet::new();
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
            next.extend(minimums(value).into_iter().filter(|(_,n)|*n!=0).map(|((id,scalar),n)|Fact::Minimum(id, scalar, n)));
            next.extend(maximums(value).into_iter().filter(|((_,scalar),n)|*n!=scalar.bit_mask()).map(|((id,scalar),n)|Fact::Maximum(id, scalar, n)));
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
    fn required_in<'a>(&'a self, environment: &'a Facts) -> Option<Facts> {
        if contradictory(environment) {
            return None;
        }
        // Resolve graph identities once; rounds consume the same query-owned slots.
        let mut indices = HashMap::from([(self.key(), 0)]);
        let mut facts = vec![QueryResult {
            clause: self, inputs: None, users: 1, remaining: 0, value: None,
        }];
        let mut index = 0;
        while index < facts.len() {
            if let Some((left, right, alternative)) = facts[index].clause.query_inputs() {
                let mut inputs = [0; 2];
                for (slot, input) in inputs.iter_mut().zip([left, right]) {
                    match indices.entry(input.key()) {
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            *slot = facts.len();
                            entry.insert(*slot);
                            facts.push(QueryResult {
                                clause: input, inputs: None, users: 1, remaining: 0, value: None,
                            });
                        }
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            *slot = *entry.get();
                            facts[*slot].users += 1;
                        }
                    }
                }
                facts[index].inputs = Some((inputs[0], inputs[1], alternative));
            }
            index += 1;
        }
        drop(indices);
        let mut pending = Vec::new();
        let mut environment: Cow<'a, Facts> = Cow::Borrowed(environment);
        loop {
            // 共同环境的别名事实只在原环境所有者处生产一次。
            let environment_roots = Cow::Owned(equality_roots(environment.iter()));
            {
                let inherited = equivalent_facts(
                    &environment,
                    Arc::new(Cow::Owned(Facts::new())),
                    &environment_roots,
                );
                if !inherited.is_empty() {
                    environment.to_mut().extend(inherited.iter().copied());
                }
            }
            for result in &mut facts {
                result.remaining = result.users;
                result.value = None;
            }
            pending.push(0);
            while let Some(index) = pending.pop() {
                if facts[index].value.is_some() {
                    continue;
                }
                match &*facts[index].clause.0 {
                    ClauseNode::Impossible => {
                        facts[index].value = Some(None);
                        continue;
                    }
                    ClauseNode::Facts(required) => {
                        let required = if required.is_disjoint(&environment) {
                            Cow::Borrowed(required)
                        } else {
                            Cow::Owned(required.difference(&environment).copied().collect())
                        };
                        let required: QueryFacts<'a> = Arc::new(required);
                        let value = if cross_conflict(&environment, &required) { None } else { Some(required) };
                        facts[index].value = Some(value);
                        continue;
                    }
                    _ => {}
                }
                let (left, right, alternative) = facts[index].inputs.unwrap();
                if facts[left].value.is_none() {
                    pending.extend([index, left]);
                    continue;
                }
                // 合取的前提已不可能，另一侧没有本次查询的消费者。
                if !alternative && facts[left].value.as_ref().unwrap().is_none() {
                    Self::take_query(&mut facts, left);
                    Self::abandon_query(&mut facts, right);
                    facts[index].value = Some(None);
                    continue;
                }
                if facts[right].value.is_none() {
                    pending.extend([index, right]);
                    continue;
                }
                let a = Self::take_query(&mut facts, left);
                let b = Self::take_query(&mut facts, right);
                let required = if alternative {
                    either_facts(&environment, &environment_roots, a, b)
                } else {
                    match (a, b) {
                        (Some(a), Some(b)) => {
                            // 叶子与合取已经检查；析取派生事实在本次合取消费处检查。
                            if (matches!(&*facts[left].clause.0, ClauseNode::Either(..)) && additions_conflict(&environment, &a))
                                || (matches!(&*facts[right].clause.0, ClauseNode::Either(..)) && additions_conflict(&environment, &b))
                            {
                                None
                            } else {
                                combine_facts(a, b)
                            }
                        }
                        _ => None,
                    }
                };
                facts[index].value = Some(required);
            }
            let required = Self::take_query(&mut facts, 0)?;
            if required.is_empty() {
                return Some(environment.into_owned());
            }
            environment.to_mut().extend(required.iter().copied());
            if contradictory(&environment) {
                return None;
            }
        }
    }
    pub(super) fn possible(&self) -> bool {
        match &*self.0 {
            ClauseNode::Impossible => false,
            ClauseNode::Facts(_) => true,
            ClauseNode::Both(..) | ClauseNode::Either(..) | ClauseNode::Branch(..) => {
                self.required().is_some()
            }
        }
    }
    pub(super) fn overlaps(&self,other:&Self) -> bool {
        // 来源能否同时成立不能由各分支的共有事实代替。
        // 只沿本次路径消费共享图；备选保留续点，不展开路径项组合。
        fn push<'a>(goals:&mut Vec<(&'a Clause,Option<usize>)>,next:&mut Option<usize>,clause:&'a Clause){
            goals.push((clause,*next));*next=Some(goals.len()-1);
        }
        let mut goals=vec![(self,None),(other,Some(0))];
        let mut next=Some(1);
        let mut alternatives:Vec<(&Self,Option<usize>,usize,usize,usize)>=Vec::new();
        let mut facts=Facts::new();
        let mut additions=Vec::new();
        let mut visited=HashSet::new();
        let mut visits=Vec::new();
        loop {
            let mut impossible=false;
            while let Some(index)=next {
                let (clause,remaining)=goals[index];next=remaining;
                if !visited.insert(clause.key()){continue;}
                visits.push(clause.key());
                match &*clause.0 {
                    ClauseNode::Impossible=>{impossible=true;break;},
                    ClauseNode::Facts(required)=>{
                        let before=additions.len();
                        for fact in required {if facts.insert(*fact){additions.push(*fact);}}
                        if additions.len()!=before && contradictory(&facts){impossible=true;break;}
                    },
                    _=>{
                        let (left,right,alternative)=clause.query_inputs().unwrap();
                        if alternative {
                            alternatives.push((right,next,goals.len(),additions.len(),visits.len()));
                        } else {push(&mut goals,&mut next,right);}
                        push(&mut goals,&mut next,left);
                    },
                }
            }
            if !impossible{return true;}
            let Some((right,remaining,goal_count,fact_count,visit_count))=alternatives.pop()else{return false;};
            for fact in additions.drain(fact_count..){facts.remove(&fact);}
            for key in visits.drain(visit_count..){visited.remove(&key);}
            goals.truncate(goal_count);next=remaining;
            push(&mut goals,&mut next,right);
        }
    }
}
fn contradictory(facts: &Facts) -> bool {
    let roots = equality_roots(facts);
    let root = |id| roots.get(&id).copied().unwrap_or(id);
    let canonical = |fact: Fact| match fact {
        Fact::Equal(a, b) => Fact::equal(root(a), root(b)),
        Fact::NotEqual(a, b) => {
            let (a, b) = (root(a), root(b));
            Fact::NotEqual(a.min(b), a.max(b))
        }
        Fact::Below(a, b, strict, scalar) => Fact::Below(root(a), root(b), strict, scalar),
        value => value.identity().map(|id| value.at(root(id))).unwrap_or(value),
    };
    // 原查询只生产一次别名规范视图；排序、查找及判定不重复投影。
    let mut aliases: Vec<Fact> = Vec::new();
    if !roots.is_empty() {
        aliases.extend(facts.iter().copied().map(canonical));
        aliases.sort_unstable();
        aliases.dedup();
    }
    let select = |query: Fact, preceding: bool| {
        if roots.is_empty() {
            if preceding { facts.range(..=query).next_back().copied() }
            else { facts.range(query..).next().copied() }
        } else {
            let index = aliases.partition_point(|fact| {
                let value = *fact;
                if preceding { value <= query } else { value < query }
            });
            if preceding {
                index.checked_sub(1).map(|index| aliases[index])
            } else {
                aliases.get(index).copied()
            }
        }
    };
    let contains = |fact| select(fact, false) == Some(fact);
    let bounds = |id, scalar: Scalar| {
        let lower = match select(Fact::Minimum(id, scalar, scalar.bit_mask()), true) {
            Some(Fact::Minimum(other, actual, bound)) if (other, actual) == (id, scalar) => bound,
            _ => 0,
        };
        let upper = match select(Fact::Maximum(id, scalar, 0), false) {
            Some(Fact::Maximum(other, actual, bound)) if (other, actual) == (id, scalar) => bound,
            _ => scalar.bit_mask(),
        };
        (lower, upper)
    };
    let conflict = |fact| match fact {
        Fact::Boolean(id, truth) => contains(Fact::Boolean(id, !truth)),
        Fact::Zero(id) => contains(Fact::NonZero(id)),
        Fact::NonZero(id) => contains(Fact::Zero(id)),
        Fact::Index(id, domain) => contains(Fact::Outside(id, domain)),
        Fact::Outside(id, domain) => contains(Fact::Index(id, domain)),
        Fact::Minimum(id, scalar, _) | Fact::Maximum(id, scalar, _) => {
            let (lower, upper) = bounds(id, scalar);
            let zero = scalar.order_bias();
            lower > upper
                || (contains(Fact::Zero(id)) && !(lower <= zero && zero <= upper))
                || (contains(Fact::NonZero(id)) && lower == upper && lower == zero)
        }
        Fact::NotEqual(a, b) => {
            a == b
                || (contains(Fact::Zero(a)) && contains(Fact::Zero(b)))
                || [false, true].into_iter().any(|truth| {
                    contains(Fact::Boolean(a, truth)) && contains(Fact::Boolean(b, truth))
                })
                || [Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64].into_iter().any(|scalar| {
                    let value = |id| {
                        if contains(Fact::Zero(id)) { Some(scalar.order_bias()) }
                        else {
                            let (lower, upper) = bounds(id, scalar);
                            (lower == upper).then_some(lower)
                        }
                    };
                    value(a).zip(value(b)).is_some_and(|(a, b)| a == b)
                })
        }
        Fact::Below(a, b, strict, scalar) => (a == b && strict)
            || contains(Fact::Below(b, a, !strict, scalar)),
        Fact::Equal(..) => false,
    };
    if roots.is_empty() { facts.iter().copied().any(conflict) }
    else { aliases.iter().copied().any(conflict) }
}

// 环境已由本轮固定点检查；这里只处理新增事实内部及其与环境的关系。
fn additions_conflict(environment: &Facts, additions: &Facts) -> bool {
    contradictory(additions) || cross_conflict(environment, additions)
}
// 两侧已各自完成内部检查；共有事实不会与另一侧冲突。
fn cross_conflict(a: &Facts, b: &Facts) -> bool {
    use std::ops::Bound::{Excluded, Included};
    let (other, facts) = if a.len() < b.len() { (b, a) } else { (a, b) };
    facts.iter().any(|fact| match *fact {
        Fact::Boolean(id, truth) => other.contains(&Fact::Boolean(id, !truth)),
        Fact::NonZero(id) => other.contains(&Fact::Zero(id)),
        Fact::Zero(id) => other.contains(&Fact::NonZero(id)),
        Fact::Index(id, domain) => other.contains(&Fact::Outside(id, domain)),
        Fact::Outside(id, domain) => other.contains(&Fact::Index(id, domain)),
        Fact::Below(a, b, strict, scalar) => {
            other.contains(&Fact::Below(b, a, !strict, scalar))
                || (strict && other.contains(&Fact::equal(a, b)))
        }
        Fact::Equal(a, b) => other.contains(&Fact::NotEqual(a, b))
            || (*fact == Fact::equal(a, b) && [Scalar::U32, Scalar::I32, Scalar::U64, Scalar::I64].into_iter().any(|scalar| {
                other.contains(&Fact::Below(a, b, true, scalar))
                    || other.contains(&Fact::Below(b, a, true, scalar))
            })),
        Fact::NotEqual(a, b) => other.contains(&Fact::equal(a, b)),
        Fact::Minimum(id, scalar, lower) => other
            .range(Fact::Maximum(id, scalar, 0)..Fact::Maximum(id, scalar, lower)).next().is_some(),
        Fact::Maximum(id, scalar, upper) => other
            .range((Excluded(Fact::Minimum(id, scalar, upper)), Included(Fact::Minimum(id, scalar, scalar.bit_mask()))))
            .next().is_some(),
    })
}


fn combine_facts<'a>(mut a: QueryFacts<'a>, mut b: QueryFacts<'a>) -> Option<QueryFacts<'a>> {
    if a.is_subset(&b) {
        return Some(b);
    }
    if b.is_subset(&a) {
        return Some(a);
    }
    if cross_conflict(&a, &b) {
        return None;
    }
    if a.len() < b.len() { std::mem::swap(&mut a, &mut b); }
    Arc::make_mut(&mut a).to_mut().extend(b.iter().copied());
    Some(a)
}
fn either_facts<'a>(environment: &Facts, environment_roots: &BTreeMap<ValueIdentity, ValueIdentity>, a: Option<QueryFacts<'a>>, b: Option<QueryFacts<'a>>) -> Option<QueryFacts<'a>> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let left = additional_equality_roots(environment_roots, &a);
            let right = additional_equality_roots(environment_roots, &b);
            let a = equivalent_facts(environment, a, &left);
            let b = equivalent_facts(environment, b, &right);
            let mut common: Facts = a.intersection(&b).copied().collect();
            let mut groups = BTreeMap::new();
            for (identity, left_root) in left.iter() {
                if let Some(right_root) = right.get(identity) {
                    let representative = if left_root == right_root {
                        if identity == left_root { continue; }
                        *left_root
                    } else {
                        match groups.entry((*left_root, *right_root)) {
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                entry.insert(*identity);
                                continue;
                            }
                            std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                        }
                    };
                    let fact = Fact::equal(representative, *identity);
                    if !environment.contains(&fact) { common.insert(fact); }
                }
            }
            // 环境仍归原所有者；这里只生产两支共同新增的范围。
            let mut left = limit_facts(&a).peekable();
            let mut right = limit_facts(&b).peekable();
            while let (Some(&(upper, identity, scalar, left_value)), Some(&(other_upper, other_identity, other_scalar, right_value))) = (left.peek(), right.peek()) {
                match (upper, identity, scalar).cmp(&(other_upper, other_identity, other_scalar)) {
                    std::cmp::Ordering::Less => { left.next(); }
                    std::cmp::Ordering::Greater => { right.next(); }
                    std::cmp::Ordering::Equal => {
                        if upper {
                            let bound = left_value.max(right_value);
                            if environment.range(Fact::Maximum(identity, scalar, 0)..=Fact::Maximum(identity, scalar, bound)).next().is_none() {
                                common.insert(Fact::Maximum(identity, scalar, bound));
                            }
                        } else {
                            let bound = left_value.min(right_value);
                            if environment.range(Fact::Minimum(identity, scalar, bound)..=Fact::Minimum(identity, scalar, scalar.bit_mask())).next().is_none() {
                                common.insert(Fact::Minimum(identity, scalar, bound));
                            }
                        }
                        left.next();
                        right.next();
                    }
                }
            }
            Some(Arc::new(Cow::Owned(common)))
        }
        (a, None) | (None, a) => a,
    }
}
// Fact 按类别、身份、实际标量类型和值排序；这里只借用每一组的最强界。
fn limit_facts(facts: &Facts) -> impl Iterator<Item = (bool, ValueIdentity, Scalar, Bits)> + '_ {
    let mut values = facts.iter().filter_map(|fact| match *fact {
        Fact::Minimum(identity, scalar, value) => Some((false, identity, scalar, value)),
        Fact::Maximum(identity, scalar, value) => Some((true, identity, scalar, value)),
        _ => None,
    }).peekable();
    std::iter::from_fn(move || {
        let (upper, identity, scalar, mut value) = values.next()?;
        while let Some(&(other_upper, other_identity, other_scalar, next)) = values.peek() {
            if (upper, identity, scalar) != (other_upper, other_identity, other_scalar) { break; }
            if !upper { value = next; }
            values.next();
        }
        Some((upper, identity, scalar, value))
    })
}
fn minimums<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> BTreeMap<(ValueIdentity, Scalar), Bits> {
    let mut result: BTreeMap<(ValueIdentity, Scalar), Bits> = BTreeMap::new();
    for fact in facts {
        if let Fact::Minimum(identity, scalar, lower) = fact {
            result
                .entry((*identity, *scalar))
                .and_modify(|v| *v = (*v).max(*lower))
                .or_insert(*lower);
        }
    }
    result
}
fn maximums<'a>(facts: impl IntoIterator<Item = &'a Fact>) -> BTreeMap<(ValueIdentity, Scalar), Bits> {
    let mut result: BTreeMap<(ValueIdentity, Scalar), Bits> = BTreeMap::new();
    for fact in facts {
        if let Fact::Maximum(identity, scalar, upper) = fact {
            result
                .entry((*identity, *scalar))
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
    pub(super) fn output_bound(&self,bindings:&OutputBindings)->Self{
        let cases=Clause::output_bounds(&self.cases,bindings);
        Self{cases:std::array::from_fn(|i|cases[i].clone())}
    }
    pub(super) fn assume(&self,path:&Clause,truth:bool)->Clause{Clause::both(path,&self.cases[usize::from(truth)])}
    pub(super) fn truth_in(&self,path:&Clause)->Option<bool>{
        if !self.assume(path,false).possible(){Some(true)}else if !self.assume(path,true).possible(){Some(false)}else{None}
    }
    pub(super) fn mentions(&self,identity:ValueIdentity)->bool{Clause::ordered(&self.cases).iter().any(|clause|matches!(&*clause.0,ClauseNode::Facts(facts)if facts.iter().any(|fact|match fact{Fact::Equal(a,b)|Fact::NotEqual(a,b)|Fact::Below(a,b,..)=>*a==identity||*b==identity,Fact::Index(a,_)|Fact::Outside(a,_)|Fact::Zero(a)|Fact::Minimum(a,..)|Fact::Maximum(a,..)|Fact::NonZero(a)|Fact::Boolean(a,_)=>*a==identity})))}
    pub(super) fn constant(truth:bool)->Self{let mut value=Self::default();value.cases[usize::from(!truth)]=Clause::impossible();value}
    pub(super) fn selected(index:ownership::ApplicationIndex,element:usize)->Self{
        let ownership::ApplicationIndex::Value(id,_)=index else{return Self::constant(matches!(index,ownership::ApplicationIndex::Constant(i)if i as usize==element))};
        let n=element as u32;let mut no=Clause::impossible();
        if n!=0{no=Clause::either(&no,&Clause::facts([Fact::Maximum(id, Scalar::U32, Bits::from(n-1))].into()));}
        if n!=u32::MAX{no=Clause::either(&no,&Clause::facts([Fact::Minimum(id, Scalar::U32, Bits::from(n+1))].into()));}
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
        let condition=Arc::new(c);
        Self {
            cases: std::array::from_fn(|truth| {
                Clause::either(
                    &Clause::branch(&a.cases[truth], &condition, true),
                    &Clause::branch(&b.cases[truth], &condition, false),
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
        let scalar = a.ty.components[0].0;
        if !scalar.is_integer() || a.ty.components.len() != 1 || b.ty != a.ty {
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
                            facts.insert(Fact::Minimum(value.identity(), scalar, lower));
                        }
                    }
                    for (value, upper) in [(a, b.component_range(0).1), (b, a.component_range(0).1)]
                    {
                        if upper != scalar.bit_mask() {
                            facts.insert(Fact::Maximum(value.identity(), scalar, upper));
                        }
                    }
                    for (value, other) in [(a, b), (b, a)] {
                        if other.is_nonzero() {
                            facts.insert(Fact::NonZero(value.identity()));
                        }
                    }
                }
                Comparison::NotEqual => {
                    let (a,b)=(a.identity(),b.identity());
                    facts.insert(Fact::NotEqual(a.min(b),a.max(b)));
                }
            }
            if op == Comparison::NotEqual {
                    for (value, other) in [(a, b), (b, a)] {
                        if other.constant.as_deref() == Some(&[0]) {
                            facts.insert(if scalar.is_signed() {
                                Fact::NonZero(value.identity())
                            } else {
                                Fact::Minimum(value.identity(), scalar, 1)
                            });
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
        index.ty.components[0].0,
    );
}
fn constrain_proofs(
    facts: &mut Facts,
    (index_id, index): (ValueIdentity, &Proof),
    (limit_id, limit): (ValueIdentity, &Proof),
    strict: bool,
    scalar: Scalar,
) {
    facts.insert(Fact::Below(index_id, limit_id, strict, scalar));
    let index_range = index.range_set(0).restrict(0, scalar.bit_mask()).hull();
    let limit_range = limit.range_set(0).restrict(0, scalar.bit_mask()).hull();
    if let Some(upper) = limit_range.1.checked_sub(Bits::from(strict)) {
        if upper != scalar.bit_mask() {
            facts.insert(Fact::Maximum(index_id, scalar, upper));
        }
    }
    if scalar == Scalar::U32 {
        facts.insert(Fact::Index(index_id, IndexDomain::Upper(limit_id, u32::from(strict))));
        facts.extend(
            indices_below(&limit.domains(0), strict)
                .into_iter()
                .map(|domain| Fact::Index(index_id, domain)),
        );
    }
    if let Some(lower) = index_range.0
        .checked_add(Bits::from(strict))
        .filter(|&lower| lower != 0 && lower <= scalar.bit_mask())
    {
        facts.insert(Fact::Minimum(limit_id, scalar, lower));
    }
}
/// 数值区间逐分量计算；资源索引域只沿已证明的无符号关系传递。
pub(super) fn arithmetic(op: &str, a: &Value, b: &Value) -> Proof {
    let count = a.regs.len().max(b.regs.len());
    let scalar = a.ty.components[0].0;
    let mut proof = Proof {
        ranges: if scalar.is_integer() {
            (0..count)
                .map(|i| {
                    let ai = i % a.regs.len();
                    let bi = i % b.regs.len();
                    let left = a.component_ranges(ai);
                    let mut right = b.component_ranges(bi);
                    let signed = scalar.is_signed();
                    let bias = scalar.order_bias();
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
                        right = right.shift_counts(scalar);
                    }
                    left.combine(&right, |left, right| {
                        let bitwise = |operation: fn(Bits, Bits) -> Bits| {
                            let (lower, upper) = bitwise_range(left, right, scalar, operation);
                            IntegerRanges::interval(lower, upper)
                        };
                        match op {
                            "&" => bitwise(|a, b| a & b),
                            "|" => bitwise(|a, b| a | b),
                            "^" => bitwise(|a, b| a ^ b),
                            _ if signed => signed_range(op, left, right, scalar),
                            _ => unsigned_range(op, left, right, scalar),
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        },
        nonzero: if scalar.is_integer() && op == "|" {
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
    if scalar.is_integer() {
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
        let scalar = b.ty.components[bi].0;
        return (b.component_ranges(bi).shift_counts(scalar).hull()
            == (scalar.order_bias(), scalar.order_bias())).then_some((a, ai));
    }
    let (right, left) = match op {
        "+" | "|" | "^" => (0, Some(0)),
        "*" => (1, Some(1)),
        "-" => (0, None),
        "/" => (1, None),
        "&" => {let mask=a.ty.components[ai].0.bit_mask(); (mask, Some(mask))},
        _ => return None,
    };
    let bits = |value: &Value, index| {
        let (lower, upper) = value.component_range(index);
        let bias = value.ty.components[index].0.order_bias();
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
    let mut domains = inherited_domains(op, a, b, component);
    let left = a.proof.domains(ai);
    let right = b.proof.domains(bi);
    let a_id = a.component_origins(ai)[0];
    let b_id = b.component_origins(bi)[0];
    if op == "+" {
        for (base, part) in [(a_id, &right), (b_id, &left)] {
            for relation in part {
                if let IndexDomain::Upper(limit, distance) = *relation {
                    domains.insert(IndexDomain::Shifted(base, limit, distance));
                }
            }
        }
        let no_wrap = u64::from(a.component_range(ai).1) + u64::from(b.component_range(bi).1) <= u64::from(u32::MAX)
            || left.contains(&IndexDomain::Room(b_id)) || right.contains(&IndexDomain::Room(a_id));
        if no_wrap {
            domains.insert(IndexDomain::Sum(a_id, b_id));
            for (base, source, amount) in [(a_id, &left, b.constant.as_ref().map(|v| v[bi] as u32)), (b_id, &right, a.constant.as_ref().map(|v| v[ai] as u32))] {
                if let Some(amount) = amount.filter(|amount| *amount != 0) {
                    domains.insert(IndexDomain::Offset(base, amount));
                    for relation in source {
                        if let IndexDomain::Offset(origin, previous) = *relation {
                            if let Some(amount) = previous.checked_add(amount) { domains.insert(IndexDomain::Offset(origin, amount)); }
                        }
                    }
                }
            }
        }
        for (source, addend) in [(&left, b.constant.as_ref().map(|v| v[bi] as u32)), (&right, a.constant.as_ref().map(|v| v[ai] as u32))] {
            if let Some(amount) = addend {
                for relation in source {
                    let shifted = match *relation {
                        IndexDomain::Upper(limit, distance) => distance.checked_sub(amount).map(|d| IndexDomain::Upper(limit, d)),
                        IndexDomain::Shifted(base, limit, distance) => distance.checked_sub(amount).map(|d| IndexDomain::Shifted(base, limit, d)),
                        _ => None,
                    };
                    domains.extend(shifted);
                }
            }
        }
    }
    if op == "-" && a.component_range(ai).0 >= b.component_range(bi).1 {
        domains.insert(IndexDomain::Upper(a_id, b.component_range(bi).0 as u32));
        if a.constant.as_ref().is_some_and(|v| v[ai] == Bits::from(u32::MAX)) { domains.insert(IndexDomain::Room(b_id)); }
        if let Some(amount) = b.constant.as_ref().map(|v| v[bi] as u32) {
            for relation in left {
                let shifted = match relation {
                    IndexDomain::Upper(limit, distance) => distance.checked_add(amount).map(|d| IndexDomain::Upper(limit, d)),
                    IndexDomain::Shifted(base, limit, distance) => distance.checked_add(amount).map(|d| IndexDomain::Shifted(base, limit, d)),
                    _ => None,
                };
                domains.extend(shifted);
            }
        }
    }
    domains
}
fn inherited_domains(op: &str, a: &Value, b: &Value, component: usize) -> Domains {
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
        let strict = a.component_nonzero(ai) && !b.component_ranges(bi).shift_counts(Scalar::U32).contains(0);
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
    let scalar = value.ty.components[0].0;
    if scalar.is_integer() && !scalar.is_signed() {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => IntegerRanges::interval(!upper & scalar.bit_mask(), !lower & scalar.bit_mask()),
                    UnaryOp::UnsignedNegate => {
                        integer_result_range((-i128::from(upper), -i128::from(lower)), 0, scalar.bit_width())
                    }
                    _ => IntegerRanges::default(),
                })
            })
            .collect();
    } else if scalar.is_integer() && scalar.is_signed() {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => IntegerRanges::interval(!upper & scalar.bit_mask(), !lower & scalar.bit_mask()),
                    UnaryOp::SignedNegate => integer_result_range(
                        (
                            -i128::from(scalar.signed(upper ^ scalar.order_bias())),
                            -i128::from(scalar.signed(lower ^ scalar.order_bias())),
                        ),
                        scalar.order_bias(), scalar.bit_width(),
                    ),
                    _ => IntegerRanges::default(),
                })
            })
            .collect();
    }
    proof
}
fn bitwise_range(
    (al, ah): (Bits, Bits),
    (bl, bh): (Bits, Bits),
    scalar: Scalar,
    operation: fn(Bits, Bits) -> Bits,
) -> (Bits, Bits) {
    // 从高位向低位交付可达的输出前缀。四个松弛位分别表示两个输入已经
    // 严格大于下界／小于上界；相同状态的后缀选择相同，只需保留前缀极值。
    let mut prefixes = [None; 16];
    prefixes[0] = Some((0, 0));
    for bit in (0..scalar.bit_width()).rev() {
        let (al, ah, bl, bh) = (
            (al >> bit) & 1,
            (ah >> bit) & 1,
            (bl >> bit) & 1,
            (bh >> bit) & 1,
        );
        let sign = (scalar.order_bias() >> bit) & 1;
        let mut next: [Option<(Bits, Bits)>; 16] = [None; 16];
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
        .fold((scalar.bit_mask(), 0), |(lower, upper), (a, b)| {
            (lower.min(a), upper.max(b))
        })
}
fn signed_range(op: &str, (al, ah): (Bits, Bits), (bl, bh): (Bits, Bits), scalar: Scalar) -> IntegerRanges {
    let decode = |value| i128::from(scalar.signed(value ^ scalar.order_bias()));
    let (al, ah, bl, bh) = (decode(al), decode(ah), decode(bl), decode(bh));
    let corners = |f: fn(i128, i128) -> i128| {
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
        "<<" => corners(|a, b| a * (1i128 << b)),
        _ => return IntegerRanges::interval(0, scalar.bit_mask()),
    };
    integer_result_range(range, scalar.order_bias(), scalar.bit_width())
}
fn integer_result_range((lower, upper): (i128, i128), bias: Bits, width: u32) -> IntegerRanges {
    // 按有序周期投影；不足一个完整周期的跨断点结果保留两侧区间。
    let period = 1i128 << width;
    let mask = (period - 1) as Bits;
    let offset = i128::from(bias);
    if upper - lower >= period - 1 {
        return IntegerRanges::interval(0, mask);
    }
    let (a, b) = ((lower as Bits & mask) ^ bias, (upper as Bits & mask) ^ bias);
    if (lower + offset).div_euclid(period) == (upper + offset).div_euclid(period) {
        IntegerRanges::interval(a, b)
    } else {
        IntegerRanges::from_intervals([(0, b), (a, mask)])
    }
}
fn unsigned_range(op: &str, (al, ah): (Bits, Bits), (bl, bh): (Bits, Bits), scalar: Scalar) -> IntegerRanges {
    if op == "-" {
        return integer_result_range((i128::from(al)-i128::from(bh), i128::from(ah)-i128::from(bl)), 0, scalar.bit_width());
    }
    let (al, ah, bl, bh) = (
        u128::from(al),
        u128::from(ah),
        u128::from(bl),
        u128::from(bh),
    );
    let range = match op {
        "min" => (al.min(bl), ah.min(bh)),
        "max" => (al.max(bl), ah.max(bh)),
        "+" => (al + bl, ah + bh),
        "*" => (al * bl, ah * bh),
        "/" if bl > 0 => (al / bh, ah / bl),
        "%" if bl > 0 => (0, ah.min(bh - 1)),
        ">>" => (al >> bh, ah >> bl),
        "<<" => (al << bl, ah << bh),
        _ => return IntegerRanges::interval(0, scalar.bit_mask()),
    };
    // u64乘法的完整区间可能超过i128；按无符号数学值投影，避免先截断符号。
    let period = 1u128 << scalar.bit_width();
    if range.1 - range.0 >= period - 1 {
        return IntegerRanges::interval(0, scalar.bit_mask());
    }
    let (lower, upper) = (range.0 as Bits & scalar.bit_mask(), range.1 as Bits & scalar.bit_mask());
    if range.0 / period == range.1 / period {
        IntegerRanges::interval(lower, upper)
    } else {
        IntegerRanges::from_intervals([(0, upper), (lower, scalar.bit_mask())])
    }
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
            IndexDomain::Upper(limit, distance) => distance.checked_add(u32::from(strict)).map(|d| IndexDomain::Upper(limit, d)),
            domain @ IndexDomain::Room(_) => Some(domain),
            IndexDomain::Shifted(base, limit, distance) => distance.checked_add(u32::from(strict)).map(|d| IndexDomain::Shifted(base, limit, d)),
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
            if matches!(place.storage(), Place::Element { .. }) && !place.overlaps(&owner) {
                continue;
            }
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
                    Fact::Minimum(ValueIdentity::DataLength(self.domain), Scalar::U32, 1),
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
        if value.proof.domains(0).contains(&domain) { return true; }
        let lengths = self.resource_length_proofs(domain);
        let minimum = if let IndexDomain::Fixed(count) = domain { count } else { minimum };
        let limit = lengths.iter().map(|(proof, component)| proof.unsigned_minimum(*component)).fold(minimum, u32::max);
        value.component_ranges(0).0.iter().all(|&(_, upper)| upper < Bits::from(limit))
            || lengths.into_iter().any(|(proof, component)| value.proof.below_length(&value.component_origins(0), proof, component))
    }
    fn resource_length_proofs(&self, domain: IndexDomain) -> Vec<(&Proof, usize)> {
        let length = match domain {
            IndexDomain::Data(b) | IndexDomain::DataLength(b) => IndexDomain::DataLength(b),
            IndexDomain::Tile | IndexDomain::TileLength => IndexDomain::TileLength,
            IndexDomain::View(id) | IndexDomain::ViewLength(id) => IndexDomain::ViewLength(id),
            IndexDomain::ApplicationData(handle) | IndexDomain::ApplicationLength(handle) => IndexDomain::ApplicationLength(handle),
            _ => return Vec::new(),
        };
        let mut proofs = Vec::new();
        for binding in self.env.values() {
            let mut value = None;
            match binding {
                Binding::Value(v) => value = Some((&v.ty, &v.proof)),
                Binding::Owned(owner) => value = Some((owner.place.ty(), &owner.proof)),
                Binding::OwnedValue(owner) => {
                    value = Some((&owner.value.ty, &owner.value.proof));
                    for data in owner.resources() {
                        if data.length.proof.domains(0).contains(&length) { proofs.push((&data.length.proof, 0)); }
                    }
                }
                Binding::ApplicationData(data) => {
                    if data.length.proof.domains(0).contains(&length) { proofs.push((&data.length.proof, 0)); }
                }
                Binding::DataReference(data) => {
                    if data.length.proof.domains(0).contains(&length) { proofs.push((&data.length.proof, 0)); }
                }
                Binding::Data(DatasetView {length: proof, ..}) | Binding::Tile(TileView {length: proof, ..}) => {
                    if proof.domains(0).contains(&length) { proofs.push((proof, 0)); }
                }
                _ => {}
            }
            if let Some((ty, proof)) = value {
                proofs.extend(ty.components.iter().enumerate().filter(|(i,(scalar,_))| *scalar == Scalar::U32 && proof.domains(*i).contains(&length)).map(|(i,_)| (proof,i)));
            }
        }
        proofs
    }
    pub(super) fn resource_minimum(&self, domain: IndexDomain, minimum: u32) -> u32 {
        if let IndexDomain::Fixed(count) = domain { return count; }
        self.resource_length_proofs(domain).into_iter().map(|(proof, component)| proof.unsigned_minimum(component)).fold(minimum, u32::max)
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
        let environment = environment_facts(&self.env);
        let mut consumers = self.env.values_mut().filter(|binding| match binding {
            Binding::Value(value) => Proof::fact_components(&value.ty).next().is_some(),
            Binding::Owned(owner) => matches!(owner.place.storage(), Place::ApplicationElement { .. })
                || Proof::fact_components(owner.place.ty()).next().is_some(),
            Binding::OwnedValue(_) | Binding::Data(_)
            | Binding::ApplicationData(_) | Binding::Tile(_) => true,
            _ => false,
        });
        if let Some(first) = consumers.next() {
            let Some(facts) = self.path.required_in(&environment) else {
                return;
            };
            let roots = equality_roots(&facts);
            for binding in std::iter::once(first).chain(consumers) {
                match &mut *binding {
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
                for fact in &facts {
                    let Some(identity) = fact.identity() else {
                        continue;
                    };
                    match &mut *binding {
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
    }
    pub(super) fn join_environment_selected(
        &mut self,
        other: &mut BTreeMap<String, Binding>,
        when:&owned_presence::Presence,
    ) {
        let bindings=BTreeMap::new();
        let mut names=[BTreeMap::new(),BTreeMap::new()];
        let mut aliases=[bounds::Clause::default(),bounds::Clause::default()];
        for(name,binding)in &mut self.env {
            let Some((a,_,place))=binding.proof_storage()else{continue};
            let Some((b,_,_))=other.get_mut(name).and_then(Binding::proof_storage)else{continue};
            let ty=place.ty();let width=ty.components.len();
            // Stable fields retain their value identity; snapshot only actual fact changes.
            let components=Proof::fact_components(ty).filter(|&i|
                !a.known_identity(i).zip(b.known_identity(i)).is_some_and(|(a,b)|a==b)
            ).collect::<Vec<_>>();
            if components.is_empty(){continue;}
            let regs=components.iter().map(|&i|{let reg=self.registers.len();self.registers.push(ty.components[i].0);reg}).collect::<Vec<_>>();
            for((proof,alias),names)in [a,b].into_iter().zip(&mut aliases).zip(&mut names){
                let selected=proof.project(&components,width);
                selected.output_bindings(&regs,0,names);
                *alias=bounds::Clause::both(alias,&selected.output_aliases(&regs,&self.registers,&bindings));
                proof.origins.resize_with(width,Vec::new);
                for(&component,&reg)in components.iter().zip(&regs){proof.origins[component].push(ValueIdentity::Register(reg));}
            }
        }
        for((environment,names),alias)in [&mut self.env,&mut *other].into_iter().zip(&names).zip(&mut aliases){
            let cases=environment.values_mut().filter_map(|binding|match binding{
                Binding::Value(value)=>Some(&mut value.proof),
                binding=>binding.proof_storage().map(|(proof,_,_)|proof),
            }).flat_map(|proof|proof.conditions.iter_mut().flatten())
                .flat_map(|condition|condition.cases.iter_mut()).collect::<Vec<_>>();
            // Keep all source roots alive while the shared graph is renamed once.
            let roots=std::iter::once(alias.clone()).chain(cases.iter().map(|case|(**case).clone())).collect::<Vec<_>>();
            let bound=bounds::Clause::output_bounds(&roots,names);
            *alias=bounds::Clause::both(alias,&bound[0]);
            for(case,bound)in cases.into_iter().zip(bound.into_iter().skip(1)){*case=bound;}
        }
        let choice=when.condition();
        if !bounds::Condition::paths(aliases[0].clone(),aliases[1].clone()).unconstrained(){
            self.path=bounds::Clause::both(&self.path,&bounds::Clause::either(&choice.assume(&aliases[0],true),&choice.assume(&aliases[1],false)));
        }
        for (name, binding) in &mut self.env {
            match (binding, &other[name]) {
                (Binding::Value(a), Binding::Value(b)) => {
                    a.proof.join_condition(&b.proof,a.regs.len(),Some(&choice));
                    a.uniform &= b.uniform;
                }
                (Binding::Owned(a), Binding::Owned(b)) => {
                    a.place.join_position_range(&b.place);
                    a.proof
                        .join_condition(&b.proof,a.place.ty().components.len(),Some(&choice));
                    a.uniform &= b.uniform;
                }
                (Binding::Data(a), Binding::Data(b)) => a.length.join(&b.length, 1, None),
                (Binding::OwnedValue(a), Binding::OwnedValue(b)) => {
                    let mut value=returns::Returned::OwnedValue(a.clone());value.join_presence(returns::Returned::OwnedValue(b.clone()),Some(when));let returns::Returned::OwnedValue(value)=value else{unreachable!()};*a=value;
                }
                (Binding::ApplicationData(a), Binding::ApplicationData(b)) => {
                    let mut value=returns::Returned::Dataset(a.clone());value.join_presence(returns::Returned::Dataset(b.clone()),Some(when));let returns::Returned::Dataset(value)=value else{unreachable!()};*a=value;
                }
                (Binding::Tile(a), Binding::Tile(b)) => a.length.join(&b.length, 1, None),
                _ => {}
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
                    EncodedClause::Both(a, b) => Clause(Arc::new(ClauseNode::Both(node(&nodes, a)?, node(&nodes, b)?))),
                    EncodedClause::Either(a, b) => {
                        Clause(Arc::new(ClauseNode::Either(node(&nodes, a)?, node(&nodes, b)?)))
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
crate::module::codec::variants!(Fact{0=>Equal(a,b),1=>Index(value,domain),2=>Minimum(value,scalar,lower),3=>Maximum(value,scalar,upper),4=>NonZero(value),5=>Boolean(value,truth),6=>Below(lower,upper,strict,scalar),7=>Outside(value,domain),8=>Zero(value),9=>NotEqual(a,b)});
