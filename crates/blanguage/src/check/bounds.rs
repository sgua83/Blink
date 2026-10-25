//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
use super::*;
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fact {
    Index(ValueIdentity, IndexDomain),
    Minimum(ValueIdentity, u32),
    Maximum(ValueIdentity, u32),
    NonZero(ValueIdentity),
}
type Facts = BTreeSet<Fact>;
#[derive(Clone, Default)]
pub(super) struct Proof {
    origins: Vec<Option<ValueIdentity>>,
    condition: Option<Box<Condition>>,
    ranges: Vec<(u32, u32)>,
    nonzero: Vec<bool>,
    domains: Vec<Domains>,
}
impl Proof {
    pub(super) fn join(&mut self, other: &Self, width: usize) {
        self.origins = (0..width)
            .map(|i| {
                let a = self.origins.get(i).copied().flatten();
                if a == other.origins.get(i).copied().flatten() {
                    a
                } else {
                    None
                }
            })
            .collect();
        self.nonzero = (0..width)
            .map(|i| self.nonzero.get(i) == Some(&true) && other.nonzero.get(i) == Some(&true))
            .collect();
        self.ranges = (0..width)
            .map(|i| {
                let (a, b) = self.range(i);
                let (c, d) = other.range(i);
                (a.min(c), b.max(d))
            })
            .collect();
        self.domains = (0..width)
            .map(|i| {
                self.domains(i)
                    .intersection(&other.domains(i))
                    .copied()
                    .collect()
            })
            .collect();
        self.condition = None;
    }
    pub(super) fn project(&self, components: &[usize], width: usize) -> Self {
        let mut proof = if components.iter().copied().eq(0..width) {
            self.clone()
        } else {
            Self::default()
        };
        proof.origins = components
            .iter()
            .map(|&i| self.origins.get(i).copied().flatten())
            .collect();
        proof.nonzero = components
            .iter()
            .map(|&i| self.nonzero.get(i) == Some(&true))
            .collect();
        proof.ranges = components.iter().map(|&i| self.range(i)).collect();
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
            ranges: vec![(minimum, maximum)],
            ..Self::default()
        }
    }
    pub fn with_domains(mut self, domains: Domains) -> Self {
        self.domains = vec![domains];
        self
    }
    pub fn with_identity(mut self, identity: ValueIdentity) -> Self {
        self.origins = vec![Some(identity)];
        self
    }
    pub(super) fn domains(&self, component: usize) -> Domains {
        self.domains.get(component).cloned().unwrap_or_default()
    }
    fn range(&self, component: usize) -> (u32, u32) {
        self.ranges.get(component).copied().unwrap_or((0, u32::MAX))
    }
    pub fn integer_bits(value: &Value) -> Self {
        Self {
            nonzero: (0..value.regs.len())
                .map(|i| value.component_nonzero(i))
                .collect(),
            ..Self::default()
        }
    }
    pub fn capture(value: &Value) -> Self {
        let mut proof = value.proof.clone();
        proof.ranges = (0..value.regs.len())
            .map(|i| value.component_range(i))
            .collect();
        proof.nonzero = (0..value.regs.len())
            .map(|i| value.component_nonzero(i))
            .collect();
        proof.origins = (0..value.regs.len())
            .map(|i| Some(value.component_identity(i)))
            .collect();
        if value.ty == Ty::scalar(Scalar::Bool) {
            proof.condition = Some(Box::new(Condition::of(value)));
        }
        proof
    }
    pub fn common(a: &Value, b: &Value) -> Self {
        Self {
            condition: (a.ty == Ty::scalar(Scalar::Bool))
                .then(|| Box::new(Condition::common(Condition::of(a), Condition::of(b)))),
            ranges: (0..a.regs.len())
                .map(|i| {
                    let (al, ah) = a.component_range(i);
                    let (bl, bh) = b.component_range(i);
                    (al.min(bl), ah.max(bh))
                })
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
            let Some(identity) = self.origins.get(component).copied().flatten() else {
                continue;
            };
            if matches!(scalar, Scalar::U32 | Scalar::I32)
                && self.nonzero.get(component) == Some(&true)
            {
                facts.insert(Fact::NonZero(identity));
            }
            if *scalar == Scalar::U32 {
                let (minimum, maximum) = self.range(component);
                if minimum != 0 {
                    facts.insert(Fact::Minimum(identity, minimum));
                }
                if maximum != u32::MAX {
                    facts.insert(Fact::Maximum(identity, maximum));
                }
                facts.extend(
                    self.domains(component)
                        .into_iter()
                        .map(|d| Fact::Index(identity, d)),
                );
            }
        }
        facts
    }
    pub(super) fn returning(value: &Value, environment: &BTreeMap<String, Binding>) -> Self {
        let mut proof = value.proof.clone();
        if value.ty == Ty::scalar(Scalar::Bool) {
            let mut facts = Facts::new();
            for binding in environment.values() {
                facts.extend(match binding {
                    Binding::Value(value) => Self::capture(value).facts(&value.ty),
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
                });
            }
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
            .extend((0..value.regs.len()).map(|i| Some(value.component_identity(i))));
        self.nonzero
            .extend((0..value.regs.len()).map(|i| value.component_nonzero(i)));
        self.ranges
            .extend((0..value.regs.len()).map(|i| value.component_range(i)));
        self.domains
            .extend((0..value.regs.len()).map(|i| value.proof.domains(i)));
    }
    pub fn splat(&mut self, width: usize) {
        self.origins = vec![self.origins[0]; width];
        self.nonzero = vec![self.nonzero[0]; width];
        self.ranges = vec![self.ranges[0]; width];
        self.domains = vec![self.domains[0].clone(); width];
    }
    pub(super) fn mark_nonzero(&mut self, component: usize) {
        self.nonzero
            .resize(self.nonzero.len().max(component + 1), false);
        self.nonzero[component] = true;
    }
    pub(super) fn mark_domain(&mut self, component: usize, domain: IndexDomain) {
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
            Fact::Minimum(_, lower) if ty.components[component].0 == Scalar::U32 => {
                self.ranges
                    .resize(self.ranges.len().max(component + 1), (0, u32::MAX));
                self.ranges[component].0 = self.ranges[component].0.max(lower);
                if lower != 0 {
                    self.mark_nonzero(component);
                }
            }
            Fact::Maximum(_, upper) if ty.components[component].0 == Scalar::U32 => {
                self.ranges
                    .resize(self.ranges.len().max(component + 1), (0, u32::MAX));
                self.ranges[component].1 = self.ranges[component].1.min(upper);
            }
            Fact::NonZero(_) if matches!(ty.components[component].0, Scalar::U32 | Scalar::I32) => {
                self.mark_nonzero(component);
            }
            _ => {}
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
                || (self.ty.components[component].0 == Scalar::U32
                    && self.component_range(component).0 != 0)
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
        if self.ty.components[component].0 != Scalar::U32 {
            return (0, u32::MAX);
        }
        let (lower, upper) = self.proof.range(component);
        let lower = lower.max(u32::from(self.proof.nonzero.get(component) == Some(&true)));
        if let Some(bits) = &self.constant {
            (lower.max(bits[component]), upper.min(bits[component]))
        } else {
            (lower, upper)
        }
    }
    fn identity(&self) -> ValueIdentity {
        self.component_identity(0)
    }
    fn component_identity(&self, index: usize) -> ValueIdentity {
        match self.proof.origins.get(index).copied().flatten() {
            Some(origin) => origin,
            None => ValueIdentity::Register(self.regs[index]),
        }
    }
    pub(super) fn view_identity(&self) -> [ValueIdentity; 3] {
        std::array::from_fn(|i| self.component_identity(i))
    }
    pub(super) fn projected_proof(&self, indices: &[usize]) -> Proof {
        Proof {
            origins: indices
                .iter()
                .map(|i| Some(self.component_identity(*i)))
                .collect(),
            condition: None,
            ranges: indices.iter().map(|i| self.component_range(*i)).collect(),
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
            let mut common: Facts = a.intersection(&b).copied().collect();
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
        let c = Self::of(c);
        let a = Self::of(a);
        let b = Self::of(b);
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
            if signed {
                for (value, other, relation) in [
                    (a, b, op),
                    (
                        b,
                        a,
                        match op {
                            Comparison::Less => Comparison::Greater,
                            Comparison::Greater => Comparison::Less,
                            Comparison::LessEqual => Comparison::GreaterEqual,
                            Comparison::GreaterEqual => Comparison::LessEqual,
                            other => other,
                        },
                    ),
                ] {
                    let known = match relation {
                        Comparison::Equal => other.is_nonzero(),
                        Comparison::NotEqual => other.constant.as_deref() == Some(&[0]),
                        relation => other.constant.as_deref().is_some_and(|bits| {
                            let c = bits[0] as i32;
                            match relation {
                                Comparison::Less => c <= 0,
                                Comparison::LessEqual => c < 0,
                                Comparison::Greater => c >= 0,
                                Comparison::GreaterEqual => c > 0,
                                Comparison::Equal | Comparison::NotEqual => false,
                            }
                        }),
                    };
                    if known {
                        facts.insert(Fact::NonZero(value.identity()));
                    }
                }
                continue;
            }
            match op {
                Comparison::Less => constrain(facts, a, b, true),
                Comparison::Greater => constrain(facts, b, a, true),
                Comparison::LessEqual => constrain(facts, a, b, false),
                Comparison::GreaterEqual => constrain(facts, b, a, false),
                Comparison::Equal => {
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
                    for (value, lower) in [(a, b.minimum()), (b, a.minimum())] {
                        if lower != 0 {
                            facts.insert(Fact::Minimum(value.identity(), lower));
                        }
                    }
                    for (value, upper) in [(a, b.maximum()), (b, a.maximum())] {
                        if upper != u32::MAX {
                            facts.insert(Fact::Maximum(value.identity(), upper));
                        }
                    }
                }
                Comparison::NotEqual => {
                    if b.constant.as_deref() == Some(&[0]) {
                        facts.insert(Fact::Minimum(a.identity(), 1));
                    }
                    if a.constant.as_deref() == Some(&[0]) {
                        facts.insert(Fact::Minimum(b.identity(), 1));
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
    if let Some(upper) = limit.maximum().checked_sub(u32::from(strict)) {
        if upper != u32::MAX {
            facts.insert(Fact::Maximum(index.identity(), upper));
        }
    }
    facts.extend(
        indices_below(&limit.proof.domains(0), strict)
            .into_iter()
            .map(|domain| Fact::Index(index.identity(), domain)),
    );
    if let Some(lower) = index
        .minimum()
        .checked_add(u32::from(strict))
        .filter(|&lower| lower != 0)
    {
        facts.insert(Fact::Minimum(limit.identity(), lower));
    }
}
/// 数值区间逐分量计算；资源索引域只沿已证明的无符号关系传递。
pub(super) fn arithmetic(op: &str, a: &Value, b: &Value) -> Proof {
    let count = a.regs.len().max(b.regs.len());
    Proof {
        ranges: if a.ty.components[0].0 == Scalar::U32 {
            (0..count)
                .map(|i| {
                    unsigned_range(
                        op,
                        a.component_range(i % a.regs.len()),
                        b.component_range(i % b.regs.len()),
                    )
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
    }
}
fn arithmetic_domains(op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let ai = component % a.regs.len();
    let bi = component % b.regs.len();
    let left = a.proof.domains(ai);
    let right = b.proof.domains(bi);
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
        return indices_below(&left, false);
    }
    let Some(constant) = b.constant.as_ref().map(|values| values[bi]) else {
        return Domains::new();
    };
    let strict = match op {
        "-" if a.component_range(ai).0 >= constant => constant != 0,
        ">>" if constant < 32 => false,
        _ => return Domains::new(),
    };
    indices_below(&left, strict)
}
pub(super) fn unary(op: UnaryOp, value: &Value) -> Proof {
    if op == UnaryOp::LogicalNot {
        return Proof::condition(Condition::of(value).negate());
    }
    let mut proof = if matches!(op, UnaryOp::SignedNegate | UnaryOp::UnsignedNegate) {
        Proof::integer_bits(value)
    } else {
        Proof::default()
    };
    if value.ty.components[0].0 == Scalar::U32 {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                let (lower, upper) = value.component_range(i);
                if lower > upper {
                    (1, 0)
                } else {
                    match op {
                        UnaryOp::BitNot => (!upper, !lower),
                        UnaryOp::UnsignedNegate if lower > 0 => {
                            (0u32.wrapping_sub(upper), 0u32.wrapping_sub(lower))
                        }
                        UnaryOp::UnsignedNegate if upper == 0 => (0, 0),
                        _ => (0, u32::MAX),
                    }
                }
            })
            .collect();
    }
    proof
}
fn unsigned_range(op: &str, (al, ah): (u32, u32), (bl, bh): (u32, u32)) -> (u32, u32) {
    // 矛盾条件形成空区间；空集合的算术仍为空，不对不存在的端点做除法。
    if al > ah || bl > bh {
        return (1, 0);
    }
    let checked = || -> Option<(u32, u32)> {
        Some(match op {
            "+" => (al.checked_add(bl)?, ah.checked_add(bh)?),
            "-" => (al.checked_sub(bh)?, ah.checked_sub(bl)?),
            "*" => (al.checked_mul(bl)?, ah.checked_mul(bh)?),
            "/" if bl > 0 => (al / bh, ah / bl),
            "%" if bl > 0 => (0, ah.min(bh - 1)),
            "&" => (0, ah.min(bh)),
            "|" | "^" => {
                let bits = ah | bh;
                let upper = if bits == 0 {
                    0
                } else {
                    u32::MAX >> bits.leading_zeros()
                };
                (if op == "|" { al.max(bl) } else { 0 }, upper)
            }
            ">>" if bh < 32 => (al >> bh, ah >> bl),
            ">>" => (0, ah),
            "<<" if bh < 32 => (al.checked_mul(1 << bl)?, ah.checked_mul(1 << bh)?),
            "<<" if bl == bh => {
                let shift = bl & 31;
                (al.checked_mul(1 << shift)?, ah.checked_mul(1 << shift)?)
            }
            _ => return None,
        })
    };
    checked().unwrap_or((0, u32::MAX))
}
pub(super) fn indices_below(domains: &Domains, strict: bool) -> Domains {
    domains
        .iter()
        .filter_map(|domain| match *domain {
            IndexDomain::DataLength(binding) if strict => Some(IndexDomain::Data(binding)),
            IndexDomain::TileLength if strict => Some(IndexDomain::Tile),
            IndexDomain::ViewLength(origin) if strict => Some(IndexDomain::View(origin)),
            domain @ (IndexDomain::Data(_) | IndexDomain::Tile | IndexDomain::View(_)) => {
                Some(domain)
            }
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
        value.maximum() < self.resource_minimum(domain, minimum)
    }
    pub(super) fn resource_minimum(&self, domain: IndexDomain, minimum: u32) -> u32 {
        let length = match domain {
            IndexDomain::Fixed(count) => return count,
            IndexDomain::Data(b) | IndexDomain::DataLength(b) => IndexDomain::DataLength(b),
            IndexDomain::Tile | IndexDomain::TileLength => IndexDomain::TileLength,
            IndexDomain::View(id) | IndexDomain::ViewLength(id) => IndexDomain::ViewLength(id),
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
                        .resize(owner.place.ty().components.len(), None);
                    for (origin, current) in
                        owner.proof.origins.iter_mut().zip(&current.proof.origins)
                    {
                        if origin.is_none() {
                            *origin = *current;
                        }
                    }
                }
            }
        }
        self.env = environment;
    }
    pub(super) fn invalidate_names(&mut self, names: &BTreeSet<String>) {
        for name in effects::modified_owners(&self.env, names) {
            if let Some(Binding::Owned(owner)) = self.env.get_mut(&name) {
                owner.uniform = false;
                owner.proof = Proof::default();
            }
        }
    }
    pub(super) fn invalidate_expression(&mut self, expr: &Expr) {
        let mut names = BTreeSet::new();
        expression_assignments(self.world, &self.definition, expr, &mut names);
        self.invalidate_names(&names);
    }
    pub(super) fn invalidate_assigned(&mut self, body: &[Stmt]) {
        let mut names = BTreeSet::new();
        assigned_names(self.world, &self.definition, body, &mut names);
        self.invalidate_names(&names);
    }
    pub(super) fn refine_bounds(&mut self, value: &Value, truth: bool) {
        let condition = Condition::of(value);
        let Some(facts) = &condition.cases[usize::from(truth)] else {
            return;
        };
        for fact in facts {
            let (Fact::Index(identity, _)
            | Fact::Minimum(identity, _)
            | Fact::Maximum(identity, _)
            | Fact::NonZero(identity)) = *fact;
            for binding in self.env.values_mut() {
                match binding {
                    Binding::Value(value) => {
                        for component in 0..value.regs.len() {
                            if value.component_identity(component) == identity {
                                value.proof.refine(&value.ty, component, *fact);
                            }
                        }
                    }
                    Binding::Owned(owner) => {
                        for component in 0..owner.proof.origins.len() {
                            if owner.proof.origins[component] == Some(identity) {
                                owner.proof.refine(owner.place.ty(), component, *fact);
                            }
                        }
                    }
                    Binding::Data(view) if identity == ValueIdentity::DataLength(view.binding) => {
                        view.length.refine(&Ty::scalar(Scalar::U32), 0, *fact);
                    }
                    Binding::Tile(view) if identity == ValueIdentity::TileLength => {
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
        owner.proof.origins.resize(width, None);
        for (component, reg) in components.iter().zip(regs) {
            owner.proof.origins[*component].get_or_insert(ValueIdentity::Register(*reg));
        }
        (owner.proof.project(&components, width), owner.uniform)
    }
    pub(super) fn assign_owned_proof(&mut self, place: &Place, fresh: Proof, uniform: bool) {
        if let Place::Indexed { base, .. } = place {
            for binding in self.env.values_mut() {
                if let Binding::Owned(owner) = binding
                    && base.components_in(&owner.place).is_some()
                {
                    owner.proof = Proof::default();
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
        if components.iter().copied().eq(0..width) {
            owner.proof = fresh;
        } else {
            let proof = &mut owner.proof;
            proof.origins.resize(width, None);
            proof.nonzero.resize(width, false);
            proof.ranges.resize(width, (0, u32::MAX));
            proof.domains.resize(width, Domains::new());
            for (j, &i) in components.iter().enumerate() {
                proof.origins[i] = fresh.origins.get(j).copied().flatten();
                proof.nonzero[i] = fresh.nonzero.get(j) == Some(&true);
                proof.ranges[i] = fresh.range(j);
                proof.domains[i] = fresh.domains(j);
            }
            proof.condition = None;
        }
    }
}
