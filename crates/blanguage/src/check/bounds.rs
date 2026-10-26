//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
use super::*;

pub(super) fn for_cursor(begin: &Value, limit: &Value, body: &[Stmt]) -> Proof {
    fn seeks(body: &[Stmt]) -> bool {
        body.iter().any(|statement| match &statement.kind {
            StmtKind::Seek(..) => true,
            StmtKind::If(_, yes, no) => seeks(yes) || seeks(no),
            // Nested loops own their reentry; their seek cannot replace this cursor.
            _ => false,
        })
    }
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
}
type Facts = BTreeSet<Fact>;
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
    condition: Option<Box<Condition>>,
    // 整数按数值顺序编码；i32 翻转符号位，复用相同的区间与路径合流。
    ranges: Vec<IntegerRanges>,
    nonzero: Vec<bool>,
    domains: Vec<Domains>,
}
impl Proof {
    pub(super) fn known_value(&self, ty: &Ty) -> Option<Vec<u32>> {
        if *ty == Ty::scalar(Scalar::Bool) {
            return match &self.condition.as_deref()?.cases {
                [None, Some(_)] => Some(vec![1]),
                [Some(_), None] => Some(vec![0]),
                _ => None,
            };
        }
        ty.components
            .iter()
            .enumerate()
            .map(|(component, (scalar, _))| {
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
    pub(super) fn assign(&mut self, components: &[usize], width: usize, fresh: Self) {
        if components.iter().copied().eq(0..width) {
            *self = fresh;
        } else {
            self.origins.resize_with(width, Vec::new);
            self.nonzero.resize(width, false);
            self.ranges.resize_with(width, IntegerRanges::default);
            self.domains.resize(width, Domains::new());
            for (j, &i) in components.iter().enumerate() {
                self.origins[i] = fresh.origins.get(j).cloned().unwrap_or_default();
                self.nonzero[i] = fresh.nonzero.get(j) == Some(&true);
                self.ranges[i] = fresh.range_set(j);
                self.domains[i] = fresh.domains(j);
            }
            self.condition = None;
        }
    }
    pub(super) fn join(&mut self, other: &Self, width: usize, branch: Option<&Value>) {
        let condition = if self.condition.is_some() || other.condition.is_some() {
            let a = self.condition.as_deref().cloned().unwrap_or_default();
            let b = other.condition.as_deref().cloned().unwrap_or_default();
            Some(Box::new(if let Some(c) = branch {
                Condition::branch(Condition::of(c), a, b)
            } else {
                Condition::common(a, b)
            }))
        } else {
            None
        };
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
        self.condition = condition;
    }
    pub(super) fn project(&self, components: &[usize], width: usize) -> Self {
        let mut proof = if components.iter().copied().eq(0..width) {
            self.clone()
        } else {
            Self::default()
        };
        proof.origins = components
            .iter()
            .map(|&i| self.origins.get(i).cloned().unwrap_or_default())
            .collect();
        proof.nonzero = components
            .iter()
            .map(|&i| self.nonzero.get(i) == Some(&true))
            .collect();
        proof.ranges = components.iter().map(|&i| self.range_set(i)).collect();
        proof.domains = components.iter().map(|&i| self.domains(i)).collect();
        proof
    }
    pub fn condition(condition: Condition) -> Self {
        Self {
            condition: Some(Box::new(condition)),
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
    pub(super) fn domains(&self, component: usize) -> Domains {
        self.domains.get(component).cloned().unwrap_or_default()
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
        if value.ty == Ty::scalar(Scalar::Bool) {
            proof.condition = Some(Box::new(Condition::of(value)));
        }
        proof
    }
    pub fn common(a: &Value, b: &Value) -> Self {
        Self {
            origins: (0..a.regs.len())
                .map(|i| {
                    let other = b.component_origins(i);
                    a.component_origins(i)
                        .into_iter()
                        .filter(|id| other.contains(id))
                        .collect()
                })
                .collect(),
            condition: (a.ty == Ty::scalar(Scalar::Bool))
                .then(|| Box::new(Condition::common(Condition::of(a), Condition::of(b)))),
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
    fn facts(&self, ty: &Ty) -> Facts {
        let mut facts = Facts::new();
        for (component, (scalar, _)) in ty.components.iter().enumerate() {
            let Some(origins) = self.origins.get(component) else {
                continue;
            };
            for pair in origins.windows(2) {
                facts.insert(Fact::equal(pair[0], pair[1]));
            }
            for &identity in origins {
                if matches!(scalar, Scalar::U32 | Scalar::I32)
                    && self.nonzero.get(component) == Some(&true)
                {
                    facts.insert(Fact::NonZero(identity));
                }
                if matches!(scalar, Scalar::U32 | Scalar::I32) {
                    let (minimum, maximum) = self.range(component);
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
    pub(super) fn returning(value: &Value, environment: &BTreeMap<String, Binding>) -> Self {
        let mut proof = value.proof.clone();
        if value.ty == Ty::scalar(Scalar::Bool) {
            let facts = environment_facts(environment);
            let mut condition = Condition::of(value);
            for case in condition.cases.iter_mut().flatten() {
                case.extend(facts.iter().copied());
            }
            proof.condition = Some(Box::new(condition));
        }
        proof
    }
    pub fn append(&mut self, value: &Value) {
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
            | Self::Minimum(id, _)
            | Self::Maximum(id, _)
            | Self::NonZero(id) => Some(id),
            Self::Equal(_, _) => None,
        }
    }
    fn at(self, id: ValueIdentity) -> Self {
        match self {
            Self::Index(_, domain) => Self::Index(id, domain),
            Self::Minimum(_, bound) => Self::Minimum(id, bound),
            Self::Maximum(_, bound) => Self::Maximum(id, bound),
            Self::NonZero(_) => Self::NonZero(id),
            equal @ Self::Equal(_, _) => equal,
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
            origins: indices.iter().map(|i| self.component_origins(*i)).collect(),
            condition: None,
            ranges: indices.iter().map(|i| self.component_ranges(*i)).collect(),
            nonzero: indices.iter().map(|i| self.component_nonzero(*i)).collect(),
            domains: indices.iter().map(|i| self.proof.domains(*i)).collect(),
        }
    }
}
#[derive(Clone)]
pub(super) struct Condition {
    // false／true 各自必然成立的事实；None 表示该结果不可能。
    cases: [Option<Facts>; 2],
}
impl Default for Condition {
    fn default() -> Self {
        Self {
            cases: [Some(Facts::new()), Some(Facts::new())],
        }
    }
}
fn both(a: &Option<Facts>, b: &Option<Facts>) -> Option<Facts> {
    Some(a.as_ref()?.union(b.as_ref()?).copied().collect())
}
fn either(a: Option<Facts>, b: Option<Facts>) -> Option<Facts> {
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
impl Condition {
    fn common(a: Self, b: Self) -> Self {
        let [af, at] = a.cases;
        let [bf, bt] = b.cases;
        Self {
            cases: [either(af, bf), either(at, bt)],
        }
    }
    pub fn of(value: &Value) -> Self {
        let mut condition = value
            .proof
            .condition
            .as_deref()
            .cloned()
            .unwrap_or_default();
        // 常量排除相反结果，但不抹掉成立结果已经持有的范围与长度事实。
        if let Some(bits) = &value.constant {
            condition.cases[usize::from(bits[0] == 0)] = None;
        }
        condition
    }
    pub fn negate(mut self) -> Self {
        self.cases.swap(0, 1);
        self
    }
    pub fn logical(and: bool, a: &Value, b: &Value) -> Self {
        let a = Self::of(a);
        let b = Self::of(b);
        Self {
            cases: if and {
                [
                    either(a.cases[0].clone(), both(&a.cases[1], &b.cases[0])),
                    both(&a.cases[1], &b.cases[1]),
                ]
            } else {
                [
                    both(&a.cases[0], &b.cases[0]),
                    either(a.cases[1].clone(), both(&a.cases[0], &b.cases[1])),
                ]
            },
        }
    }
    pub fn select(c: &Value, a: &Value, b: &Value) -> Self {
        Self::branch(Self::of(c), Self::of(a), Self::of(b))
    }
    fn branch(c: Self, a: Self, b: Self) -> Self {
        Self {
            cases: std::array::from_fn(|truth| {
                either(
                    both(&c.cases[1], &a.cases[truth]),
                    both(&c.cases[0], &b.cases[truth]),
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
                    either(
                        both(&a.cases[0], &b.cases[1]),
                        both(&a.cases[1], &b.cases[0]),
                    ),
                    either(
                        both(&a.cases[0], &b.cases[0]),
                        both(&a.cases[1], &b.cases[1]),
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
        let mut facts = [Facts::new(), Facts::new()];
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
        Self {
            cases: facts.map(Some),
        }
    }
}
fn constrain(facts: &mut Facts, index: &Value, limit: &Value, strict: bool) {
    if let Some(upper) = limit.component_range(0).1.checked_sub(u32::from(strict)) {
        if upper != u32::MAX {
            facts.insert(Fact::Maximum(index.identity(), upper));
        }
    }
    if index.ty == Ty::scalar(Scalar::U32) {
        facts.extend(
            indices_below(&limit.proof.domains(0), strict)
                .into_iter()
                .map(|domain| Fact::Index(index.identity(), domain)),
        );
    }
    if let Some(lower) = index
        .component_range(0)
        .0
        .checked_add(u32::from(strict))
        .filter(|&lower| lower != 0)
    {
        facts.insert(Fact::Minimum(limit.identity(), lower));
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
                    if matches!(op, "-" | "^" | "&" | "|" | "/" | "%")
                        && a.same_component(ai, b, bi)
                    {
                        // 同值快照只取共同可达片段；空路径仍为空。
                        let shared = left.combine(&right, |(al, ah), (bl, bh)| {
                            IntegerRanges::interval(al.max(bl), ah.min(bh))
                        });
                        return match op {
                            "&" | "|" => shared,
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
    if matches!(op, "&" | "|") && a.same_component(ai, b, bi) {
        return Some((a, ai));
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
    pub(super) fn refine_bounds(&mut self, value: &Value, truth: bool) {
        let condition = Condition::of(value);
        let Some(facts) = &condition.cases[usize::from(truth)] else {
            return;
        };
        let mut facts = facts.clone();
        facts.extend(environment_facts(&self.env));
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
                        for component in 0..owner.proof.origins.len() {
                            if owner.proof.origins[component].contains(&identity) {
                                owner.proof.refine(owner.place.ty(), component, *fact);
                            }
                        }
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
        let projected = self.env.values_mut().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let components = place.components_in(&owner.place)?;
            Some((owner, components))
        });
        let Some((owner, components)) = projected else {
            return (Proof::default(), false);
        };
        let width = owner.place.ty().components.len();
        owner.proof.origins.resize_with(width, Vec::new);
        for (component, reg) in components.iter().zip(regs) {
            if owner.proof.origins[*component].is_empty() {
                owner.proof.origins[*component].push(ValueIdentity::Register(*reg));
            }
        }
        (owner.proof.project(&components, width), owner.uniform)
    }
    pub(super) fn assign_owned_proof(&mut self, place: &Place, fresh: Proof, uniform: bool) {
        if let Place::Indexed { base, .. } = place {
            for binding in self.env.values_mut() {
                if let Binding::Owned(owner) = binding
                    && let Some(components) = base.components_in(&owner.place)
                {
                    owner.proof.assign(
                        &components,
                        owner.place.ty().components.len(),
                        Proof::default(),
                    );
                    owner.uniform = false;
                }
            }
        }
        let projected = self.env.values_mut().find_map(|binding| {
            let Binding::Owned(owner) = binding else {
                return None;
            };
            let components = place.components_in(&owner.place)?;
            Some((owner, components))
        });
        let Some((owner, components)) = projected else {
            return;
        };
        owner.uniform = uniform;
        let width = owner.place.ty().components.len();
        owner.proof.assign(&components, width, fresh);
    }
}
