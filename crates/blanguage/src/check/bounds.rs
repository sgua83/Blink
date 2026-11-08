//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
use super::*;
mod floating;
use std::{borrow::Cow, collections::{HashMap, HashSet}, mem::ManuallyDrop, sync::Arc};

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
    Predicate(ScalarPredicate, bool),
}
// 运算关系只引用已求值的快照；常量使用实际类型与位模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Operand {
    Value(ValueIdentity),
    Constant(Scalar, Bits),
}
impl Operand {
    pub(super) fn of(value: &Value, component: usize) -> Self {
        match &value.constant {
            Some(bits) => Self::Constant(value.ty.components[component].0, bits[component]),
            None => Self::Value(value.component_identity(component)),
        }
    }
    pub(super) fn map(self, identity: impl Fn(ValueIdentity) -> Option<ValueIdentity>) -> Option<Self> {
        Some(match self {
            Self::Value(value) => Self::Value(identity(value)?),
            constant => constant,
        })
    }
    pub(super) fn at_call(self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Option<Self> {
        Some(match self {
            Self::Value(id) => {
                let proof = bindings.get(&id)?;
                match (proof.known_identity(0), proof.range_set(0).0.as_slice()) {
                    (Some(id), _) => Self::Value(id),
                    (None, [(a, b)]) if a == b => Self::Constant(Scalar::U32, *a),
                    _ => return None,
                }
            }
            constant => constant,
        })
    }
}
// Predicates retain the scalar type through substitution. Floating results
// retain unordered semantics: NaN makes !(a < b) different from a >= b.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ScalarPredicate {
    op: Comparison,
    scalar: Scalar,
    a: Operand,
    b: Operand,
}
impl ScalarPredicate {
    fn new(mut op: Comparison, scalar: Scalar, mut a: Operand, mut b: Operand) -> Self {
        match op {
            Comparison::Greater => { op = Comparison::Less; std::mem::swap(&mut a, &mut b); }
            Comparison::GreaterEqual => { op = Comparison::LessEqual; std::mem::swap(&mut a, &mut b); }
            Comparison::Equal | Comparison::NotEqual if b < a => std::mem::swap(&mut a, &mut b),
            _ => {}
        }
        Self { op, scalar, a, b }
    }
    fn map(self, identity: impl Fn(ValueIdentity) -> Option<ValueIdentity>) -> Option<Self> {
        Some(Self::new(self.op, self.scalar, self.a.map(&identity)?, self.b.map(&identity)?))
    }
    fn mentions(self, identity: ValueIdentity) -> bool {
        [self.a, self.b].contains(&Operand::Value(identity))
    }
    fn at_call(self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Option<Self> {
        let operand = |operand| Some(match operand {
            Operand::Value(id) => {
                let proof = bindings.get(&id)?;
                // Capture already preserves a constant's exact storage bits.
                // This is identity substitution, never floating range inference.
                let bits = proof.range_set(0);
                match bits.0.as_slice() {
                    [(a, b)] if a == b => Operand::Constant(self.scalar, if self.scalar.is_float(){floating::bits(self.scalar,*a)}else{*a^self.scalar.order_bias()}),
                    _ => Operand::Value(proof.known_identity(0)?),
                }
            }
            constant => constant,
        });
        Some(Self::new(self.op, self.scalar, operand(self.a)?, operand(self.b)?))
    }
}
crate::module::codec::variants!(Comparison{0=>Equal,1=>NotEqual,2=>Less,3=>Greater,4=>LessEqual,5=>GreaterEqual});
crate::module::codec::record!(ScalarPredicate { op, scalar, a, b });
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum IntegerOperation { Add, Subtract, Multiply, Divide, Remainder, And, Or, Xor, ShiftLeft, ShiftRight }
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum IntegerExpression {
    Binary(IntegerOperation, Scalar, Operand, Operand),
}
impl IntegerExpression {
    fn map(self, identity: impl Fn(ValueIdentity) -> Option<ValueIdentity>) -> Option<Self> {
        Some(match self {
            Self::Binary(op, scalar, a, b) => Self::Binary(op, scalar, a.map(&identity)?, b.map(&identity)?),
        })
    }
    fn at_call(self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Option<Self> {
        let Self::Binary(op, scalar, a, b) = self;
        let operand = |value| Some(match value {
            Operand::Value(id) => {
                let proof = bindings.get(&id)?;
                match proof.range_set(0).restrict(0, scalar.bit_mask()).0.as_slice() {
                    [(a, b)] if a == b => Operand::Constant(scalar, *a ^ scalar.order_bias()),
                    _ => Operand::Value(proof.known_identity(0)?),
                }
            }
            constant => constant,
        });
        Some(Self::Binary(op, scalar, operand(a)?, operand(b)?))
    }
    fn constant(self) -> Option<(Scalar, Bits)> {
        let Self::Binary(op, scalar, Operand::Constant(_, a), Operand::Constant(_, b)) = self else { return None; };
        let operation = match op {
            IntegerOperation::Add => BinaryOp::IAdd,
            IntegerOperation::Subtract => BinaryOp::ISub,
            IntegerOperation::Multiply => BinaryOp::IMul,
            IntegerOperation::And => BinaryOp::And,
            IntegerOperation::Or => BinaryOp::Or,
            IntegerOperation::Xor => BinaryOp::Xor,
            IntegerOperation::ShiftLeft => BinaryOp::ShiftLeft,
            IntegerOperation::ShiftRight if scalar.is_signed() => BinaryOp::ShiftRightArithmetic,
            IntegerOperation::ShiftRight => BinaryOp::ShiftRightLogical,
            IntegerOperation::Divide | IntegerOperation::Remainder => return std::num::NonZeroU64::new(b)
                .map(|divisor| (scalar, constant::divide(scalar, a, divisor, op == IntegerOperation::Remainder))),
        };
        constant::binary(operation, scalar, a, b).map(|bits| (scalar, bits))
    }
    fn ranges_at_call(self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Option<ScalarRanges> {
        let Self::Binary(op, scalar, a, b) = self;
        let range = |operand| Some(match operand {
            Operand::Value(id) => bindings.get(&id)?.range_set(0).restrict(0, scalar.bit_mask()),
            Operand::Constant(_, bits) => ScalarRanges::interval(bits ^ scalar.order_bias(), bits ^ scalar.order_bias()),
        });
        let left = range(a)?;
        let right = range(b)?;
        if let ([(al, ah)], [(bl, bh)]) = (left.0.as_slice(), right.0.as_slice())
            && al == ah && bl == bh
        {
            let exact = Self::Binary(op, scalar,
                Operand::Constant(scalar, *al ^ scalar.order_bias()),
                Operand::Constant(scalar, *bl ^ scalar.order_bias()));
            let (_, bits) = exact.constant()?;
            let ordered = bits ^ scalar.order_bias();
            return Some(ScalarRanges::interval(ordered, ordered));
        }
        let operation = match op {
            IntegerOperation::Add => "+", IntegerOperation::Subtract => "-",
            IntegerOperation::Multiply => "*", IntegerOperation::Divide => "/",
            IntegerOperation::Remainder => "%", IntegerOperation::And => "&",
            IntegerOperation::Or => "|", IntegerOperation::Xor => "^",
            IntegerOperation::ShiftLeft => "<<", IntegerOperation::ShiftRight => ">>",
        };
        Some(arithmetic_ranges(operation, scalar, left, right))
    }
}
crate::module::codec::variants!(Operand{0=>Value(identity),1=>Constant(scalar,bits)});
crate::module::codec::variants!(IntegerOperation{0=>Add,1=>Subtract,2=>Multiply,3=>Divide,4=>Remainder,5=>And,6=>Or,7=>Xor,8=>ShiftLeft,9=>ShiftRight});
crate::module::codec::variants!(IntegerExpression{0=>Binary(op,scalar,a,b)});
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
fn map_domain(domain:IndexDomain, identity:&impl Fn(ValueIdentity)->ValueIdentity)->IndexDomain {
    match domain {
        IndexDomain::View(ids) => IndexDomain::View(ids.map(|id| identity(id))),
        IndexDomain::ViewLength(ids) => {
            IndexDomain::ViewLength(ids.map(|id| identity(id)))
        }
        IndexDomain::ApplicationData(id) => {
            IndexDomain::ApplicationData(identity(id))
        }
        IndexDomain::ApplicationLength(id) => {
            IndexDomain::ApplicationLength(identity(id))
        }
        IndexDomain::Upper(limit, distance) => IndexDomain::Upper(identity(limit), distance),
        IndexDomain::Lower(base, distance) => IndexDomain::Lower(identity(base), distance),
        IndexDomain::Room(base) => IndexDomain::Room(identity(base)),
        IndexDomain::Sum(left, right) => IndexDomain::Sum(identity(left), identity(right)),
        IndexDomain::Offset(base, amount) => IndexDomain::Offset(identity(base), amount),
        IndexDomain::Shifted(base, limit, distance) => IndexDomain::Shifted(identity(base), identity(limit), distance),
        IndexDomain::Computed(expression) => IndexDomain::Computed(expression.map(|id| Some(identity(id))).unwrap()),
        IndexDomain::CommonRange(source,from,low,extended,to) => IndexDomain::CommonRange(identity(source),from,low,extended,to),
        other => other,
    }
}
fn cast_domain(domain:IndexDomain,source:Scalar,target:Scalar)->Option<IndexDomain> {
    match domain {
        IndexDomain::KnownBits(from,mask,value)=>{
            if !source.is_integer()||!target.is_integer(){return None;}
            let bits=KnownBits{scalar:from,mask,value}.cast(source).cast(target);
            (bits.mask!=0).then(||bits.domain())
        },
        IndexDomain::CommonRange(id,base,mut low,mut extended,_) if source.is_integer()&&target.is_integer()=>{
            let unsigned=|scalar|match scalar{Scalar::I8=>Scalar::U8,Scalar::I16=>Scalar::U16,Scalar::I32=>Scalar::U32,Scalar::I64=>Scalar::U64,other=>other};
            if target.size()<=low.size(){low=target;extended=unsigned(target);}
            else if target.size()<extended.size(){extended=unsigned(target);}
            else if source.is_signed()&&source.size()==extended.size(){
                if source.size()==low.size(){low=source;}
                extended=unsigned(target);
            }
            Some(IndexDomain::CommonRange(id,base,low,extended,target))
        }
        domain if source==target&&common_range_domain(domain)||source.is_integer()&&target.is_integer()&&!source.is_signed()&&!target.is_signed()&&target.size()>source.size()=>Some(domain),
        _=>None,
    }
}
fn numeric_domain(domain: &IndexDomain) -> bool {
    matches!(domain, IndexDomain::Upper(..) | IndexDomain::Lower(..) | IndexDomain::Room(..) | IndexDomain::Sum(..) | IndexDomain::Shifted(..) | IndexDomain::Offset(..) | IndexDomain::CommonRange(..) | IndexDomain::Computed(..) | IndexDomain::KnownBits(..))
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
struct ScalarRanges(Vec<(Bits, Bits)>);
impl Default for ScalarRanges {
    fn default() -> Self {
        Self(vec![(0, Bits::MAX)])
    }
}
impl ScalarRanges {
    fn cast(&self,source:Scalar,target:Scalar,nonzero:bool)->Self {
        let range=self.restrict(0,source.bit_mask());
        let range=if source.is_integer()&&nonzero{range.without(source.order_bias())}else{range};
        if source==target{return range;}
        range.map(|(lower,upper)|{
            let decode=|bits|if source.is_signed(){i128::from(source.signed(bits^source.order_bias()))}else{i128::from(bits)};
            integer_result_range((decode(lower),decode(upper)),target.order_bias(),target.bit_width())
        })
    }
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
#[derive(Clone,Copy,PartialEq,Eq)]
struct KnownBits { scalar:Scalar, mask:Bits, value:Bits }
impl KnownBits {
    fn domain(self)->IndexDomain {IndexDomain::KnownBits(self.scalar,self.mask,self.value)}
    fn cast(self,scalar:Scalar)->Self {
        let mut mask=self.mask&scalar.bit_mask();let mut value=self.value&mask;
        if scalar.size()>self.scalar.size(){
            let extra=scalar.bit_mask()&!self.scalar.bit_mask();
            if !self.scalar.is_signed(){mask|=extra;}
            else {let sign=1u64<<(self.scalar.bit_width()-1);if self.mask&sign!=0{mask|=extra;if self.value&sign!=0{value|=extra;}}}
        }
        Self{scalar,mask,value}
    }
    fn combined(self,other:Self)->Self {
        let scalar=if self.scalar.size()>=other.scalar.size(){self.scalar}else{other.scalar};
        let a=self.cast(scalar);let b=other.cast(scalar);
        Self{scalar,mask:a.mask|b.mask,value:a.value|b.value}
    }
    fn common(self,other:Self)->Option<Self> {
        let other=other.cast(self.scalar);let mask=self.mask&other.mask&!(self.value^other.value);
        (mask!=0).then_some(Self{scalar:self.scalar,mask,value:self.value&mask})
    }
    fn facts(scalar:Scalar,id:ValueIdentity,sets:[&Facts;2])->Option<Self> {
        let (mut lower,mut upper)=(0,scalar.bit_mask());
        for facts in sets {
            if let Some(Fact::Minimum(_,_,bound))=facts.range(Fact::Minimum(id,scalar,0)..=Fact::Minimum(id,scalar,scalar.bit_mask())).next_back(){lower=lower.max(*bound);}
            if let Some(Fact::Maximum(_,_,bound))=facts.range(Fact::Maximum(id,scalar,0)..=Fact::Maximum(id,scalar,scalar.bit_mask())).next(){upper=upper.min(*bound);}
            if facts.contains(&Fact::Zero(id)){lower=lower.max(scalar.order_bias());upper=upper.min(scalar.order_bias());}
        }
        let mut bits=Self::ranges(scalar,ScalarRanges::interval(lower,upper));
        for facts in sets {for fact in facts.range(Fact::Index(id,IndexDomain::KnownBits(scalar,0,0))..=Fact::Index(id,IndexDomain::KnownBits(scalar,Bits::MAX,Bits::MAX))) {
            if let Fact::Index(_,IndexDomain::KnownBits(from,mask,value))=*fact {
                let incoming=Self{scalar:from,mask,value}.cast(scalar);
                bits=Some(match bits{Some(old)=>old.combined(incoming),None=>incoming});
            }
        }}
        bits
    }
    fn ranges(scalar:Scalar,ranges:ScalarRanges)->Option<Self> {
        let mut result:Option<Self>=None;
        for(lower,upper)in ranges.restrict(0,scalar.bit_mask()).0 {
            let different=lower^upper;let mask=scalar.bit_mask()&if different==0{Bits::MAX}else{!(Bits::MAX>>different.leading_zeros())};
            if mask==0{return None;}
            let value=Self{scalar,mask,value:(lower^scalar.order_bias())&mask};
            result=Some(match result {Some(previous)=>previous.common(value)?,None=>value});
        }
        result
    }
}
#[derive(Clone, Default)]
pub(crate) struct Proof {
    // 首项是实际来源，其余是当前路径已经证明相等的快照。
    pub(super) origins: Vec<Vec<ValueIdentity>>,
    conditions: Vec<Option<Box<Condition>>>,
    // 标量按数值次序编码；整数符号偏置和可逆浮点有序位复用同一区间集合。
    ranges: Vec<ScalarRanges>,
    nonzero: Vec<bool>,
    domains: Vec<Domains>,
    // view分量的实际借用来源，与数值事实分别合流；只存在于编译期。
    views: Vec<view::Borrow>,
    // Address bits describe storage numerically; they grant no borrow or lifetime.
    pub(super) addresses: Vec<Option<address::Identity>>,
}
impl PartialEq for Proof {
    fn eq(&self,other:&Self)->bool {
        use crate::module::codec::Encode;
        // Compare the existing canonical conditions, not equivalent DAG shapes.
        self.origins==other.origins && self.conditions.bytes()==other.conditions.bytes()
            && self.ranges.iter().map(|range|&range.0).eq(other.ranges.iter().map(|range|&range.0))
            && self.nonzero==other.nonzero && self.domains==other.domains
            && self.views==other.views && self.addresses==other.addresses
    }
}
impl Eq for Proof {}
impl Proof {
    // 阶段完成只交接共同数值和逻辑数据目标；值身份、路径及借用不跨阶段。
    pub(super) fn element_facts(&self, target: impl Fn(usize) -> Self) -> Self {
        let mut facts = Self {
            ranges: self.ranges.clone(),
            domains: vec![Domains::new(); self.domains.len()],
            ..Self::default()
        };
        facts.ranges.resize_with(self.ranges.len().max(self.domains.len()), ScalarRanges::default);
        for (component, domains) in self.domains.iter().enumerate() {
            for domain in domains {
                if matches!(domain,IndexDomain::KnownBits(..)){facts.mark_domain(component,*domain);}
                if let IndexDomain::Data(binding) = domain {
                    let bound = target(*binding);
                    facts.domains[component].extend(bound.domains(0));
                    facts.ranges[component] = facts.range_set(component).combine(&bound.range_set(0),
                        |(a, b), (c, d)| ScalarRanges::interval(a.max(c), b.min(d)));
                }
            }
        }
        facts
    }
    fn element_range(&self, component: usize, scalar: Scalar) -> ScalarRanges {
        let ranges = self.range_set(component).restrict(0, scalar.bit_mask());
        if scalar.is_integer() && self.nonzero.get(component) == Some(&true) {
            ranges.without(scalar.order_bias())
        } else { ranges }
    }
    pub(super) fn element_ranges(&self, binding: usize, ty: &Ty) -> Vec<ElementRange> {
        ty.components.iter().enumerate().filter_map(|(component, &(scalar, offset))| {
            let intervals = self.element_range(component, scalar).0;
            (intervals != [(0, scalar.bit_mask())]).then_some(ElementRange { binding, offset, scalar, intervals })
        }).collect()
    }
    pub(super) fn element_guarantees(&self, binding: usize, ty: &Ty, selected: impl Fn(u32, u32) -> bool) -> Vec<ElementFact> {
        let mut facts = Vec::new();
        for (component, &(scalar, offset)) in ty.components.iter().enumerate() {
            if !selected(offset, scalar.size()) { continue; }
            let intervals = self.element_range(component, scalar).0;
            if intervals != [(0, scalar.bit_mask())] {
                facts.push(ElementFact::Range(ElementRange { binding, offset, scalar, intervals }));
            }
            if scalar == Scalar::U32 {
                facts.extend(self.domains(component).into_iter().filter_map(|domain| match domain {
                    IndexDomain::Data(target) => Some(ElementFact::Index { binding, offset, target }),
                    _ => None,
                }));
            }
        }
        facts
    }
    pub(super) fn constrain_element_range(&mut self, component: usize, range: &ElementRange) {
        self.ranges.resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
        self.ranges[component] = self.range_set(component).combine(&ScalarRanges::from_intervals(range.intervals.iter().copied()),
            |(a,b),(c,d)| ScalarRanges::interval(a.max(c), b.min(d)));
    }
    pub(super) fn holds_element_range(&self, component: usize, range: &ElementRange) -> bool {
        self.element_range(component, range.scalar).0.iter().all(|&(a,b)|
            range.intervals.iter().any(|&(c,d)| c <= a && b <= d))
    }
    pub(super) fn assign_elements(&mut self, components: &[usize], width: usize, fresh: &Self) {
        self.origins.resize_with(width, Vec::new);
        self.ranges.resize_with(width, ScalarRanges::default);
        self.domains.resize_with(width, Domains::new);
        self.conditions.resize_with(width, || None);
        self.nonzero.resize(width,false);
        for (source, &component) in components.iter().enumerate() {
            self.nonzero[component]=fresh.nonzero.get(source)==Some(&true);
            self.origins[component] = fresh.origins.get(source).cloned().unwrap_or_default();
            self.ranges[component] = fresh.range_set(source);
            self.domains[component] = fresh.domains(source);
            self.conditions[component] = fresh.conditions.get(source).cloned().flatten();
        }
    }
    pub(super) fn constrain_common_ranges(&mut self, other: &Self, width: usize) {
        self.domains.resize_with(width, Domains::new);
        self.ranges.resize_with(width, ScalarRanges::default);
        self.conditions.resize_with(width,||None);
        for component in 0..width {
            for domain in other.domains(component){self.mark_domain(component,domain);}
            self.ranges[component] = self.range_set(component).combine(&other.range_set(component),
                |(a, b), (c, d)| ScalarRanges::interval(a.max(c), b.min(d)));
            if let Some(incoming)=other.component_condition(component){
                let condition=self.conditions[component].get_or_insert_with(||Box::new(Condition::default()));
                for(target,incoming)in condition.cases.iter_mut().zip(&incoming.cases){*target=Clause::both(target,incoming);}
            }
        }
    }
    pub(super) fn assume_same_length(&mut self, other: &Self) {
        self.origins.resize_with(self.origins.len().max(1), Vec::new);
        for &identity in other.origins.first().into_iter().flatten() {
            if !self.origins[0].contains(&identity) { self.origins[0].push(identity); }
        }
        let range = self.range_set(0).combine(&other.range_set(0),
            |(a,b),(c,d)| ScalarRanges::interval(a.max(c),b.min(d)));
        self.ranges.resize_with(self.ranges.len().max(1), ScalarRanges::default);
        self.ranges[0] = range;
        self.nonzero.resize(self.nonzero.len().max(1), false);
        self.nonzero[0] |= other.nonzero.first() == Some(&true);
        self.domains.resize_with(self.domains.len().max(1), Domains::new);
        self.domains[0].extend(other.domains(0));
    }
    pub(super) fn borrow_state(&self)->Self {Self{views:self.views.clone(),..Self::default()}}
    pub(super) fn numeric_facts(&self)->Self {
        Self{origins:self.origins.clone(),conditions:self.conditions.clone(),ranges:self.ranges.clone(),nonzero:self.nonzero.clone(),domains:self.domains.clone(),..Self::default()}
    }
    pub(super) fn typed_view_borrow(&self,ty:&Ty)->view::Borrow {view_contracts::components(ty).into_iter().fold(view::Borrow::default(),|mut all,component|{all.union(&self.view_borrow(component));all})}
    pub(super) fn select_view_borrows(&mut self,values:&[(Condition,Self)]){
        let width=values.iter().map(|(_,proof)|proof.views.len()).max().unwrap_or(0);
        self.views=(0..width).map(|component|{let mut value=view::Borrow::default();for(when,proof)in values{value.union(&proof.view_borrow(component).restricted(&when.assume(&Clause::default(),true)));}value}).collect();
    }
    pub(super) fn join_view_borrows(&mut self,a:&Self,b:&Self,choice:&Condition){
        self.views=(0..a.views.len().max(b.views.len())).map(|component|{
            let a=a.view_borrow(component);let b=b.view_borrow(component);
            view::Borrow::selected(choice,&a,&b)
        }).collect();
    }
    pub(super) fn retain_view_borrows(&mut self,ty:&Ty) {let selected=view_contracts::components(ty);for(component,borrow)in self.views.iter_mut().enumerate(){if !selected.contains(&component){*borrow=view::Borrow::default();}}}
    pub(super) fn view_borrow(&self,component:usize)->view::Borrow {self.views.get(component).cloned().unwrap_or_default()}
    pub(super) fn borrowed_view(&self)->view::Borrow {self.views.iter().fold(view::Borrow::default(),|mut all,value|{all.union(value);all})}
    pub(super) fn borrowed_views(&self)->BTreeSet<view::Source> {self.borrowed_view().sources.entries().filter_map(|(source,when)|when.possible().then_some(source.clone())).collect()}
    pub(super) fn with_view_sources(self,component:usize,sources:BTreeSet<view::Source>)->Self {self.with_view_borrow(component,view::Borrow::from_sources(sources))}
    pub(super) fn with_external_borrows(mut self,ty:&Ty)->Self {
        for (offset,field) in ty.borrow_fields() {
            if field.is_address() {
                let component=ty.components.iter().position(|(_,at)|*at==offset).unwrap();
                let borrow=self.view_borrow(component).joined(view::Borrow::source(view::Source::External));
                self=self.with_view_borrow(component,borrow);
            }
        }
        self
    }
    pub(super) fn with_view_borrow(mut self,component:usize,borrow:view::Borrow)->Self {
        self.views.resize_with(self.views.len().max(component+1),view::Borrow::default);self.views[component]=borrow;self
    }
    pub(super) fn view_parameters(mut self, ty: &Ty, indices:&[Option<Operand>], identity: impl Fn(usize) -> ValueIdentity) -> Self {
        for at in view_contracts::components(ty) {
            self = self.with_view_sources(at, BTreeSet::from([view::Source::Parameter(identity(at),indices.to_vec())]));
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
        for(component,reg)in registers.iter().enumerate(){if types[*reg]==Scalar::Bool && let Some(condition)=outputs.component_condition(component){
            let cases=std::array::from_fn::<_,2,_>(|truth|Clause::both(&condition.cases[truth],&Clause::facts([Fact::Boolean(ValueIdentity::Register(*reg),truth!=0)].into())));
            aliases=Clause::both(&aliases,&Clause::either(&cases[0],&cases[1]));
        }}aliases
    }
    pub(super) fn output_bound(&self,bindings:&OutputBindings)->Self{
        let mut proof=self.clone();
        proof.addresses=self.addresses.iter().map(|value|value.as_ref().map(|value|value.output_bound(bindings))).collect();
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
                .map(|d| map_domain(*d,&|id|output_identity(id,bindings)))
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
    pub(super) fn scalar_identities(&self)->BTreeSet<ValueIdentity> {
        let ids=std::cell::RefCell::new(self.origins.iter().flatten().copied().collect::<BTreeSet<_>>());
        let identity=|id|{ids.borrow_mut().insert(id);id};
        for domain in self.domains.iter().flatten(){map_domain(*domain,&identity);}
        for condition in self.conditions.iter().flatten(){condition.identities(&identity);}
        for address in self.addresses.iter().flatten(){if let address::Identity::Value(values)=address{for id in values{identity(*id);}}}
        ids.into_inner()
    }
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,Proof>)->Self {
        let mut bindings=self.scalar_identities().into_iter().map(|id|(id,Proof::default().with_identity(id).with_view_borrow(0,view::Borrow::source(view::Source::Parameter(id,Vec::new()))))).collect::<BTreeMap<_,_>>();
        bindings.extend(values.iter().map(|(id,proof)|(*id,proof.clone())));
        let mut proof=self.at_call(&bindings);
        proof.views=self.views.iter().map(|borrow|borrow.substitute_values(values)).collect();
        for(component,address)in self.addresses.iter().enumerate(){
            if let Some(address)=address{
                proof.addresses.resize_with(proof.addresses.len().max(component+1),||None);
                proof.addresses[component]=Some(address.substitute_values(values));
            }
        }
        proof
    }
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
                IndexDomain::KnownBits(..)=>value,
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
                IndexDomain::Lower(base, distance) => IndexDomain::Lower(identity(base, bindings)?, distance),
                IndexDomain::Room(base) => IndexDomain::Room(identity(base, bindings)?),
                IndexDomain::Sum(left, right) => IndexDomain::Sum(identity(left, bindings)?, identity(right, bindings)?),
                IndexDomain::Offset(base, amount) => IndexDomain::Offset(identity(base, bindings)?, amount),
                IndexDomain::Shifted(base, limit, distance) => IndexDomain::Shifted(identity(base, bindings)?, identity(limit, bindings)?, distance),
                IndexDomain::Computed(expression) => IndexDomain::Computed(expression.at_call(bindings)?),
                _ => return None,
            })
        }
        fn common_bits(input:&Proof,from:Scalar,low:Scalar,extended:Scalar,to:Scalar)->Option<KnownBits> {
            if ![from,low,extended,to].into_iter().all(Scalar::is_integer){return None;}
            let bits=input.known_bits(0,from)?.cast(low).cast(extended).cast(to);
            (bits.mask!=0).then_some(bits)
        }
        fn fact(value: Fact, bindings: &BTreeMap<ValueIdentity, Proof>) -> Clause {
            let translated = match value {
                Fact::Boolean(id, _) | Fact::Zero(id) | Fact::NonZero(id) => {
                    let truth=match value {Fact::Boolean(_,truth)=>truth,Fact::NonZero(_)=>true,_=>false};
                    let Some(input) = bindings.get(&id) else {
                        return Clause::facts(Facts::new());
                    };
                    if !truth && input.nonzero.first()==Some(&true) {
                        return Clause::impossible();
                    }
                    let condition = input.component_condition(0).cloned().unwrap_or_default();
                    let mut clause = condition.cases[usize::from(truth)].clone();
                    if let Some(id) = input.known_identity(0) {
                        clause = Clause::both(&clause,&Clause::facts([value.at(id)].into()));
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
                Fact::Equal(a, b) => {
                    let Some((left,right))=identity(a,bindings).zip(identity(b,bindings)) else{return Clause::default();};
                    let mut facts=Facts::from([Fact::equal(left,right)]);
                    for(source,target)in [(a,right),(b,left)] {
                        facts.extend(bindings[&source].domains(0).into_iter().map(|domain|Fact::Index(target,domain)));
                    }
                    let mut clause=Clause::facts(facts);
                    for source in [a,b] {
                        if let Some(condition)=bindings[&source].component_condition(0){
                            clause=Clause::both(&clause,&Clause::either(&condition.cases[0],&condition.cases[1]));
                        }
                    }
                    return clause;
                },
                Fact::NotEqual(a, b) => {
                    if let Some((left, right)) = bindings.get(&a).zip(bindings.get(&b)) {
                        let same = left.known_identity(0).zip(right.known_identity(0))
                            .is_some_and(|(a, b)| a == b);
                        let conditions = left.component_condition(0).and_then(Condition::boolean_constant)
                            .zip(right.component_condition(0).and_then(Condition::boolean_constant));
                        let (al, ah) = left.range(0);
                        let (bl, bh) = right.range(0);
                        if same || conditions == Some((false,false))
                            || (al == ah && bl == bh && al == bl) {
                            return Clause::impossible();
                        }
                        if conditions.is_some_and(|(a,b)| a != b) || ah < bl || bh < al {
                            return Clause::default();
                        }
                    }
                    identity(a, bindings).zip(identity(b, bindings))
                        .map(|(a,b)| Fact::NotEqual(a.min(b), a.max(b)))
                },
                Fact::Index(id, IndexDomain::CommonRange(source,from,low,extended,to)) => {
                    let mut facts=Facts::new();
                    if let Some(target)=identity(id,bindings) && let Some(input)=bindings.get(&source){
                        let range=input.range_set(0).cast(from,low,input.nonzero.first()==Some(&true)).cast(low,extended,false).cast(extended,to,false);
                        if to==Scalar::Bool && let Some(truth)=input.component_condition(0).and_then(Condition::boolean_constant){facts.insert(Fact::Boolean(target,truth));}
                        let nonzero=if to.is_integer(){!range.contains(to.order_bias())||common_bits(input,from,low,extended,to).is_some_and(|bits|bits.value!=0)}else{input.nonzero.first()==Some(&true)};
                        if nonzero{facts.insert(Fact::NonZero(target));}
                        if to.is_integer()||to.is_float(){let(minimum,maximum)=range.hull();if minimum!=0{facts.insert(Fact::Minimum(target,to,minimum));}if maximum!=to.bit_mask(){facts.insert(Fact::Maximum(target,to,maximum));}}
                        facts.extend(input.domains(0).into_iter().filter_map(|domain|cast_domain(domain,from,low).and_then(|domain|cast_domain(domain,low,extended)).and_then(|domain|cast_domain(domain,extended,to))).map(|domain|Fact::Index(target,domain)));
                        facts.extend(input.origins.first().into_iter().flatten().filter(|source|!matches!(source,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)|ValueIdentity::ElementIndex(_))).map(|source|Fact::Index(target,IndexDomain::CommonRange(*source,from,low,extended,to))));
                    }
                    return Clause::facts(facts);
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
                Fact::Outside(id,d)=>identity(id,bindings).zip(domain(d,bindings)).map(|(id,d)|Fact::Outside(id,d)),
                Fact::Predicate(predicate,truth)=>{
                    let Some(mapped)=predicate.at_call(bindings)else{return Clause::default()};
                    let mut facts=Facts::from([Fact::Predicate(mapped,truth)]);
                    for operand in [predicate.a,predicate.b]{
                        if let Operand::Value(source)=operand && let Some(target)=bindings[&source].known_identity(0){facts.extend(bindings[&source].domains(0).into_iter().filter(|domain|matches!(domain,IndexDomain::CommonRange(..))).map(|domain|Fact::Index(target,domain)));}
                    }
                    return Clause::facts(facts);
                },
            };
            Clause::facts(translated.into_iter().collect())
        }
        let mut translated = self.clone();
        translated.views = self.views.iter().map(|borrow|borrow.at_call(bindings)).collect();
        translated.addresses=self.addresses.iter().map(|value|value.as_ref().map(|value|value.at_call(bindings))).collect();
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
                .resize_with(translated.ranges.len().max(i + 1), ScalarRanges::default);
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
                if let Some(address)=value.addresses.first().cloned().flatten(){translated.addresses.resize_with(translated.addresses.len().max(i+1),||None);translated.addresses[i]=Some(address);}
                let incoming = value.range_set(0);
                translated.ranges[i] = ScalarRanges::from_intervals(
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
        for (component, domains) in self.domains.iter().enumerate() {
            for domain in domains {
                if let IndexDomain::CommonRange(source,from,low,extended,to) = domain {
                    if let Some(incoming) = bindings.get(source) {
                        let range=incoming.range_set(0).cast(*from,*low,incoming.nonzero.first()==Some(&true)).cast(*low,*extended,false).cast(*extended,*to,false);
                        if let Some(bits)=common_bits(incoming,*from,*low,*extended,*to){translated.mark_domain(component,bits.domain());}
                        translated.ranges.resize_with(translated.ranges.len().max(component + 1), ScalarRanges::default);
                        translated.ranges[component] = translated.ranges[component].combine(&range,
                            |(al, ah), (bl, bh)| ScalarRanges::interval(al.max(bl), ah.min(bh)));
                        translated.nonzero.resize(translated.nonzero.len().max(component + 1), false);
                        translated.nonzero[component] |= if to.is_integer(){!range.contains(to.order_bias())}else{incoming.nonzero.first()==Some(&true)};
                        translated.domains.resize_with(translated.domains.len().max(component + 1), Domains::new);
                        translated.domains[component].extend(incoming.domains(0).into_iter().filter_map(|domain|cast_domain(domain,*from,*low).and_then(|domain|cast_domain(domain,*low,*extended)).and_then(|domain|cast_domain(domain,*extended,*to))));
                        translated.domains[component].extend(incoming.origins.first().into_iter().flatten()
                            .filter(|id|!matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)|ValueIdentity::ElementIndex(_)))
                            .map(|id|IndexDomain::CommonRange(*id,*from,*low,*extended,*to)));
                    }
                }
                if let IndexDomain::Computed(expression) = domain
                    && let Some(range) = expression.ranges_at_call(bindings)
                {
                    translated.ranges.resize_with(translated.ranges.len().max(component + 1), ScalarRanges::default);
                    translated.ranges[component] = translated.ranges[component].combine(&range,
                        |(al, ah), (bl, bh)| ScalarRanges::interval(al.max(bl), ah.min(bh)));
                }
            }
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
                let mapped = match &**clause.0 {
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
        for component in 0..translated.conditions.len(){
            if translated.component_condition(component).is_some_and(|condition|!condition.cases[0].possible()){translated.mark_nonzero(component);}
        }
        translated
    }
    pub(super) fn known_value(&self, ty: &Ty) -> Option<Vec<Bits>> {
        ty.components
            .iter()
            .enumerate()
            .map(|(component, (scalar, _))| {
                if *scalar == Scalar::Bool {
                    return self.known_boolean_component(component).map(|(bits, _)| bits);
                }
                self.known_integer_component(*scalar, component).map(|(bits, _)| bits)
            })
            .collect()
    }
    fn known_boolean_component(&self, component: usize) -> Option<(Bits, [bool; 2])> {
        let cases = self.component_condition(component)?.cases.each_ref().map(Clause::possible);
        let bits = match cases { [false, true] => 1, [true, false] => 0, _ => return None };
        Some((bits, cases))
    }
    pub(super) fn known_boolean_value(&self, width: usize) -> Option<Vec<(Bits, [bool; 2])>> {
        (0..width).map(|component| self.known_boolean_component(component)).collect()
    }
    fn known_integer_component(&self, scalar: Scalar, component: usize) -> Option<(Bits, IntegerKnownValueBasis)> {
        if !scalar.is_integer() { return None; }
        let bias = scalar.order_bias();
        match self.range_set(component).restrict(0, scalar.bit_mask()).0.as_slice() {
            [(lower, upper)] if lower == upper => Some((lower ^ bias, IntegerKnownValueBasis::SingletonRange)),
            _ if self.component_condition(component).is_some_and(|condition|!condition.cases[1].possible()&&condition.cases[0].possible()) => Some((0, IntegerKnownValueBasis::ZeroCondition)),
            _ => None,
        }
    }
    pub(super) fn known_integer_value(&self, ty: &Ty) -> Option<Vec<(Bits, IntegerKnownValueBasis)>> {
        ty.components.iter().enumerate().map(|(component, (scalar, _))| self.known_integer_component(*scalar, component)).collect()
    }
    pub(super) fn known_identity(&self, component: usize) -> Option<ValueIdentity> {
        self.origins
            .get(component)
            .and_then(|origins| origins.first())
            .copied()
    }
    // 数值事实失效不改变仍存在的view所借用的实际存储。
    // initial为实际标量的共有数值界；当前值不继承入口常量、相等身份或变体选择。
    pub(super) fn retain_loop_bounds(&mut self, initial: &Self, component: usize, lower: bool, upper: bool) {
        self.ranges.resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
        if lower {
            self.ranges[component] = self.ranges[component].restrict(Bits::from(initial.unsigned_minimum(0)), Bits::from(u32::MAX));
            if initial.unsigned_minimum(0) != 0 { self.mark_nonzero(component); }
            for domain in initial.domains(0) {
                if let IndexDomain::Lower(base, distance) | IndexDomain::Offset(base, distance) = domain {
                    self.mark_domain(component, IndexDomain::Lower(base, distance));
                }
            }
        }
        if upper {
            self.ranges[component] = self.ranges[component].restrict(0, Bits::from(initial.unsigned_maximum(0)));
            for domain in indices_below(&initial.domains(0), false) { self.mark_domain(component, domain); }
            if let Some(id) = initial.known_identity(0) { self.mark_domain(component, IndexDomain::Upper(id, 0)); }
        }
    }
    fn known_bits(&self,component:usize,scalar:Scalar)->Option<KnownBits> {
        let mut bits=KnownBits::ranges(scalar,self.range_set(component));
        for domain in self.domains.get(component).into_iter().flatten(){
            let IndexDomain::KnownBits(from,mask,value)=*domain else{continue;};
            let incoming=KnownBits{scalar:from,mask,value}.cast(scalar);
            if incoming.mask!=0{bits=Some(match bits{Some(old)=>old.combined(incoming),None=>incoming});}
        }
        bits
    }
    pub(super) fn written_range(&self,component:usize,scalar:Scalar,bytes:u8)->Self {
        if !scalar.is_integer()||bytes==((1u16<<scalar.size())-1) as u8{return Self::default();}
        let mask=(0..scalar.size()).filter(|byte|bytes&(1<<byte)!=0).fold(0,|mask,byte|mask|(255u64<<(byte*8)));
        let kept=scalar.bit_mask()&!mask;let ordered=kept^scalar.order_bias();
        let range=arithmetic_ranges("&",scalar,self.range_set(component).restrict(0,scalar.bit_mask()),ScalarRanges::interval(ordered,ordered));
        let changed=ScalarRanges::interval(0,mask).cast(Scalar::U64,scalar,false);
        let mut proof=Self{ranges:vec![arithmetic_ranges("|",scalar,range,changed)],..Self::default()};
        if let Some(mut bits)=self.known_bits(component,scalar){bits.mask&=kept;bits.value&=bits.mask;if bits.mask!=0{proof.mark_domain(0,bits.domain());}}
        proof
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
            self.ranges.resize_with(width, ScalarRanges::default);
            self.domains.resize(width, Domains::new());
            if !self.addresses.is_empty()||!fresh.addresses.is_empty(){self.addresses.resize_with(width,||None);}
            for (j, &i) in components.iter().enumerate() {
                if !self.addresses.is_empty(){self.addresses[i]=fresh.addresses.get(j).cloned().flatten();}
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
        if !self.addresses.is_empty()||!other.addresses.is_empty(){self.addresses=(0..width).map(|i|address::Identity::selected(branch,self.addresses.get(i).and_then(Option::as_ref),other.addresses.get(i).and_then(Option::as_ref))).collect();}
        let conditions = (0..width)
            .map(|i| {
                if self.component_condition(i).is_none() && other.component_condition(i).is_none() {
                    return None;
                }
                let condition=|proof:&Self|match proof.component_condition(i){
                    Some(value)=>value.clone(),
                    None if proof.nonzero.get(i)==Some(&true)=>Condition::constant(true),
                    None=>Condition::default(),
                };
                let a=condition(self);
                let b=condition(other);
                Some(Box::new(if let Some(c) = branch {
                    Condition::branch(c.clone(), a, b)
                } else {
                    Condition::common(a, b)
                }))
            })
            .collect();
        self.views=(0..width).map(|i|if let Some(choice)=branch{view::Borrow::selected(choice,&self.view_borrow(i),&other.view_borrow(i))}else{self.view_borrow(i).joined(other.view_borrow(i))}).collect();
        self.domains = (0..width).map(|i| {
            let a=self.domains(i);let b=other.domains(i);
            let mut common=a.intersection(&b).copied().collect::<Domains>();
            for domain in a.union(&b) {
                let IndexDomain::Upper(limit,_) = domain else {continue;};
                let distances=[(&a,self.origins.get(i)),(&b,other.origins.get(i))].map(|(domains,origins)| {
                    domains.iter().filter_map(|domain|match domain {
                        IndexDomain::Upper(other,distance) if other==limit=>Some(*distance),
                        _=>None,
                    }).max().or_else(||origins.is_some_and(|items|items.contains(limit)).then_some(0))
                });
                if let [Some(a),Some(b)]=distances {common.insert(IndexDomain::Upper(*limit,a.min(b)));}
            }
            if let Some(scalar)=a.union(&b).find_map(|domain|match domain{IndexDomain::KnownBits(scalar,..)=>Some(*scalar),_=>None}){
                common.retain(|domain|!matches!(domain,IndexDomain::KnownBits(..)));
                if let Some(bits)=self.known_bits(i,scalar).zip(other.known_bits(i,scalar)).and_then(|(a,b)|a.common(b)){common.insert(bits.domain());}
            }
            common
        }).collect();
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
            .map(|i| self.nonzero.get(i) == Some(&true) && other.nonzero.get(i) == Some(&true) || self.domains(i).iter().any(|domain|matches!(domain,IndexDomain::KnownBits(_,_,value)if *value!=0)))
            .collect();
        self.ranges = (0..width)
            .map(|i| self.range_set(i).union(&other.range_set(i)))
            .collect();
        self.conditions = conditions;
    }
    pub(super) fn project(&self, components: &[usize], width: usize) -> Self {
        if components.iter().copied().eq(0..width) {
            return self.clone();
        }
        Self {
            views: components.iter().map(|&i| self.view_borrow(i)).collect(),
            addresses: if self.addresses.is_empty(){Vec::new()}else{components.iter().map(|&i|self.addresses.get(i).cloned().flatten()).collect()},
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
    pub(super) fn condition(condition: Condition) -> Self {
        Self {
            conditions: vec![Some(Box::new(condition))],
            ..Self::default()
        }
    }
    pub fn unsigned(minimum: u32, maximum: u32) -> Self {
        Self {
            ranges: vec![ScalarRanges::interval(Bits::from(minimum), Bits::from(maximum))],
            ..Self::default()
        }
    }
    pub(super) fn unsigned_shifted(&self, low: i128, high: i128) -> Self {
        let scalar = Scalar::U32;
        Self {
            ranges: vec![arithmetic_ranges("+", scalar,
                self.range_set(0).cast(scalar,scalar,self.nonzero.get(0)==Some(&true)),
                integer_result_range((low,high),0,scalar.bit_width()))],
            ..Self::default()
        }
    }
    pub(super) fn unsigned_reflected(&self, amount: u32) -> Self {
        let amount = Bits::from(amount);
        Self {
            ranges: vec![arithmetic_ranges("-", Scalar::U32,
                ScalarRanges::interval(amount, amount), self.range_set(0))],
            ..Self::default()
        }
    }
    pub(super) fn with_domains(mut self, mut domains: Domains) -> Self {
        // 存储版本更换资源域，数值仍是实际同一份长度快照。
        domains.extend(self.domains(0).into_iter().filter(numeric_domain));
        self.domains = vec![domains];
        self
    }
    pub(super) fn below_length(&self, identities: &[ValueIdentity], length: &Self, component: usize, strict: bool) -> bool {
        let bounds = self.domains(0);
        let origins = length.origins.get(component).map(Vec::as_slice).unwrap_or_default();
        if bounds.iter().any(|bound|match *bound{
            IndexDomain::Data(binding)=>origins.contains(&ValueIdentity::DataLength(binding)),
            IndexDomain::Tile=>origins.contains(&ValueIdentity::TileLength),
            IndexDomain::View(value)=>origins.contains(&value[2]),
            _=>false,
        }){return true;}
        if length.domains(component).iter().any(|domain| {
            let IndexDomain::Offset(base, amount) = *domain else { return false; };
            amount >= u32::from(strict) && identities.contains(&base) || bounds.iter().any(|bound| match *bound {
                IndexDomain::Offset(source, offset) => source == base && if strict { offset < amount } else { offset <= amount },
                IndexDomain::Upper(limit, distance) => limit == base && (!strict || amount != 0 || distance != 0),
                _ => false,
            })
        }) { return true; }
        bounds.iter().any(|bound| match *bound {
            IndexDomain::Upper(limit, distance) => distance >= u32::from(strict) && origins.contains(&limit),
            IndexDomain::Shifted(base, limit, distance) if distance >= u32::from(strict) => length.domains(component).iter().any(|sum| {
                matches!(*sum, IndexDomain::Sum(left, right) if (left == base && right == limit) || (left == limit && right == base))
            }),
            _ => false,
        })
    }
    pub(super) fn with_component_identity(mut self,component:usize,identity:ValueIdentity)->Self {
        self.origins.resize_with(self.origins.len().max(component+1),Vec::new);
        self.origins[component]=vec![identity];self
    }
    pub(super) fn with_identity(mut self, identity: ValueIdentity) -> Self {
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
    pub(super) fn component_condition(&self, component: usize) -> Option<&Condition> {
        self.conditions.get(component)?.as_deref()
    }
    fn value_condition(value: &Value, component: usize) -> Option<Box<Condition>> {
        if value.ty.components[component].0 == Scalar::Bool || value.ty.components[component].0.is_integer() {
            Some(Box::new(Condition::component(value, component)))
        } else {
            value.proof.conditions.get(component).cloned().flatten()
        }
    }
    fn range(&self, component: usize) -> (Bits, Bits) {
        self.range_set(component).hull()
    }
    fn range_set(&self, component: usize) -> ScalarRanges {
        self.ranges.get(component).cloned().unwrap_or_default()
    }
    pub(super) fn cast(value: &Value, target: Scalar) -> Self {
        let mut proof = Self::capture(value);
        for (i, &(source, _)) in value.ty.components.iter().enumerate() {
            if source == target { continue; }
            if !(source.is_integer()&&target.is_integer()&&source.size()==8&&target.size()==8)&&let Some(address)=proof.addresses.get_mut(i){*address=None;}
            let integers = source.is_integer() && target.is_integer();
            let same_value = integers && !source.is_signed() && !target.is_signed() && target.size() > source.size();
            proof.ranges[i] = if integers {
                proof.ranges[i].cast(source,target,proof.nonzero[i])
            } else if source == Scalar::Bool && target.is_integer() {
                ScalarRanges::interval(target.order_bias(), target.order_bias() + 1)
            } else { floating::cast(value, i, target) };
            proof.nonzero[i] = integers && !proof.ranges[i].contains(target.order_bias());
            let domains=proof.domains(i).into_iter().filter_map(|domain|cast_domain(domain,source,target))
                .chain(proof.origins[i].iter().copied().filter(|_|integers&&!same_value).filter_map(|id|cast_domain(IndexDomain::CommonRange(id,source,source,source,source),source,target))).collect::<Domains>();
            if let Some(current)=proof.domains.get_mut(i){current.clear();}
            for domain in domains{proof.mark_domain(i,domain);}
            if !same_value{proof.origins[i].clear();}
            if !integers || target.size() < source.size() { proof.conditions[i] = None; }
        }
        proof
    }
    pub(super) fn capture(value: &Value) -> Self {
        let mut proof = value.proof.clone();
        proof.ranges = (0..value.regs.len()).map(|i| {
            value.component_ranges(i)
        }).collect();
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
    pub(super) fn common(a: &Value, b: &Value, choice: Option<&Condition>, outputs: Option<&[Reg]>) -> Self {
        let mut proof=Self::capture(a);let mut other=Self::capture(b);
        let aliases=[Clause::facts(proof.facts(&a.ty)),Clause::facts(other.facts(&b.ty))];
        for(component,(scalar,_))in a.ty.components.iter().enumerate(){
            if scalar.is_integer(){
                for((target,aliases),source)in [&mut proof,&mut other].into_iter().zip(&aliases).zip([a,b]){
                    for condition in target.conditions[component].iter_mut(){
                        for case in &mut condition.cases{*case=Clause::both(case,aliases);}
                        if let Some(regs)=outputs{
                            **condition=condition.output_bound(&BTreeMap::from([(source.component_identity(component),ValueIdentity::Register(regs[component]))]));
                        }
                    }
                }
            }
        }
        proof.join_condition(&other,a.regs.len(),choice);
        proof
    }
    pub(super) fn comparison(op: Comparison, a: &Value, b: &Value) -> Self {
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
                        if matches!(&**condition.cases[usize::from(!truth)].0, ClauseNode::Impossible) {
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
                    if !scalar.is_signed() {
                        for domain in self.domains(component) {
                            facts.insert(Fact::Index(identity, domain));
                            match domain {
                                IndexDomain::Offset(base, amount) => {
                                    facts.insert(Fact::Below(base, identity, amount != 0, *scalar));
                                }
                                IndexDomain::Upper(limit, distance) => {
                                    facts.insert(Fact::Below(identity, limit, distance != 0, *scalar));
                                }
                                _ => {}
                            }
                        }
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
            if (*scalar == Scalar::Bool || scalar.is_integer()) && self.conditions[component].is_none() {
                let mut condition = Condition::default();
                if let Some(identity) = self.known_identity(component) {
                    for (truth, case) in condition.cases.iter_mut().enumerate() {
                        let fact=if *scalar==Scalar::Bool{Fact::Boolean(identity,truth!=0)}else if truth!=0{Fact::NonZero(identity)}else{Fact::Zero(identity)};
                        *case = Clause::facts([fact].into());
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
    pub(super) fn append(&mut self, value: &Value) {
        if !self.addresses.is_empty()||!value.proof.addresses.is_empty(){self.addresses.resize_with(self.origins.len(),||None);self.addresses.extend((0..value.regs.len()).map(|i|value.proof.addresses.get(i).cloned().flatten()));}
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
        if !self.addresses.is_empty(){self.addresses=vec![self.addresses[0].clone();width];}
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
        if matches!(domain,IndexDomain::KnownBits(_,_,value)if value!=0){self.nonzero.resize(self.nonzero.len().max(component+1),false);self.nonzero[component]=true;}
        let count = match domain {
            IndexDomain::Fixed(count) => Some(count),
            // 这些动态资源的当前长度ABI为u32，索引严格小于长度。
            IndexDomain::Data(_) | IndexDomain::Tile | IndexDomain::View(_) | IndexDomain::ApplicationData(_) => Some(u32::MAX),
            _ => None,
        };
        if let Some(count) = count {
            self.ranges
                .resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
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
        let component_scalar = ty.components[component].0;
        match fact {
            Fact::Boolean(_, truth) if component_scalar == Scalar::Bool => {
                self.conditions
                    .resize_with(self.conditions.len().max(component + 1), || None);
                let condition = self.conditions[component]
                    .get_or_insert_with(|| Box::new(Condition::default()));
                condition.cases[usize::from(!truth)] = Clause::impossible();
            }
            Fact::Index(_, domain) if component_scalar == Scalar::U32 || component_scalar.is_integer() && !component_scalar.is_signed() && numeric_domain(&domain) => {
                self.mark_domain(component, domain);
            }
            Fact::Minimum(_, scalar, lower)
                if ty.components[component].0 == scalar =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
                self.ranges[component] = self.ranges[component].restrict(lower, scalar.bit_mask());
                if !scalar.is_signed() && lower != 0 {
                    self.mark_nonzero(component);
                }
            }
            Fact::Maximum(_, scalar, upper)
                if ty.components[component].0 == scalar =>
            {
                self.ranges
                    .resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
                self.ranges[component] = self.ranges[component].restrict(0, upper);
            }
            Fact::Zero(_) if ty.components[component].0.is_integer()=>{
                let zero=ty.components[component].0.order_bias();
                self.ranges.resize_with(self.ranges.len().max(component+1),ScalarRanges::default);
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
            Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => owner.proof.facts(owner.place.ty()),
            Binding::OwnedValue(owner) => owner.value.proof.facts(&owner.value.ty),
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
    fn map(self,identity:&impl Fn(ValueIdentity)->ValueIdentity)->Self {
        match self {
            Self::Equal(a,b)=>Self::equal(identity(a),identity(b)),
            Self::NotEqual(a,b)=>{let(a,b)=(identity(a),identity(b));Self::NotEqual(a.min(b),a.max(b))},
            Self::Index(id,domain)=>Self::Index(identity(id),map_domain(domain,identity)),
            Self::Outside(id,domain)=>Self::Outside(identity(id),map_domain(domain,identity)),
            Self::Minimum(id,scalar,n)=>Self::Minimum(identity(id),scalar,n),
            Self::Maximum(id,scalar,n)=>Self::Maximum(identity(id),scalar,n),
            Self::NonZero(id)=>Self::NonZero(identity(id)),
            Self::Zero(id)=>Self::Zero(identity(id)),
            Self::Boolean(id,truth)=>Self::Boolean(identity(id),truth),
            Self::Predicate(predicate,truth)=>Self::Predicate(predicate.map(|id|Some(identity(id))).unwrap(),truth),
            Self::Below(a,b,strict,scalar)=>Self::Below(identity(a),identity(b),strict,scalar),
        }
    }

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
            Self::Equal(_, _) | Self::NotEqual(_, _) | Self::Below(..) | Self::Predicate(..) => None,
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
            predicate @ Self::Predicate(..) => predicate,
        }
    }
}
fn equality_roots<'a, I>(facts: I) -> BTreeMap<ValueIdentity, ValueIdentity>
where I: IntoIterator<Item = &'a Fact>, I::IntoIter: Clone {
    let facts = facts.into_iter();
    let mut roots = BTreeMap::new();
    extend_equality_roots(&mut roots, facts.clone());
    extend_computed_roots(&mut roots, facts);
    roots
}
// 同一确定运算在相等输入上产生相等结果；嵌套结果沿本次查询收敛。
fn extend_computed_roots<'a>(
    roots: &mut BTreeMap<ValueIdentity, ValueIdentity>,
    facts: impl IntoIterator<Item = &'a Fact>,
) {
    let expressions: Vec<_> = facts.into_iter().filter_map(|fact| match *fact {
        Fact::Index(result, IndexDomain::Computed(expression)) => Some((result, expression)),
        _ => None,
    }).collect();
    loop {
        let mut produced = BTreeMap::new();
        let mut equalities = Facts::new();
        for &(result, expression) in &expressions {
            let result = roots.get(&result).copied().unwrap_or(result);
            let expression = expression.map(|id| Some(roots.get(&id).copied().unwrap_or(id))).unwrap();
            match produced.entry(expression) {
                std::collections::btree_map::Entry::Vacant(entry) => { entry.insert(result); },
                std::collections::btree_map::Entry::Occupied(entry) if *entry.get() != result => {
                    equalities.insert(Fact::equal(*entry.get(), result));
                },
                _ => {},
            }
        }
        if equalities.is_empty() { break; }
        extend_equality_roots(roots, &equalities);
    }
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
// environment 在本轮固定点内不变；节点合并本路径新增的相等与整数运算关系。
fn additional_equality_roots<'a>(
    environment: &Facts,
    environment_roots: &'a BTreeMap<ValueIdentity, ValueIdentity>,
    additions: &Facts,
) -> Cow<'a, BTreeMap<ValueIdentity, ValueIdentity>> {
    if !additions.iter().any(|fact| matches!(fact, Fact::Equal(..) | Fact::Index(_, IndexDomain::Computed(_)))) {
        return Cow::Borrowed(environment_roots);
    }
    let mut roots = environment_roots.clone();
    extend_equality_roots(&mut roots, additions);
    extend_computed_roots(&mut roots, environment.union(additions));
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
pub(super) const INTEGER_DIVISOR_MESSAGE:&str="整数除数的每个分量都必须是非零常量或在当前作用域已证明非零的值";
pub(super) fn integer_divisors(value:&Value,path:&Clause)->Option<Vec<IntegerDivisor>> {
    if let Some(constants)=value.constant.as_ref().and_then(|values|values.iter().copied().map(std::num::NonZeroU64::new).collect::<Option<Vec<_>>>()){
        return Some(constants.into_iter().map(IntegerDivisor::Constant).collect());
    }
    if value.is_nonzero()||value.constant.is_none()&&path.possible()&&value.ty.components.iter().all(|(scalar,offset)|value.nonzero_bytes(*offset,scalar.size(),path)){
        return Some(value.regs.iter().copied().map(IntegerDivisor::Value).collect());
    }
    None
}
impl Value {
    pub(super) fn is_nonzero(&self) -> bool {
        (self.ty.is_numeric() || self.ty.is_c_function())
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
    // A complete byte range is nonzero when its contained integer words cannot all be zero.
    pub(super) fn nonzero_bytes(&self, offset: u32, size: u32, path: &Clause) -> bool {
        let mut zero = Facts::new();
        let mut path = path.clone();
        for (component, (scalar, at)) in self.ty.components.iter().enumerate() {
            if !scalar.is_integer() || *at < offset || at + scalar.size() > offset + size { continue; }
            if self.component_nonzero(component) { return true; }
            if let Some(condition) = self.proof.component_condition(component) { path = condition.assume(&path, false); }
            zero.extend(self.component_origins(component).into_iter().map(Fact::Zero));
            zero.insert(Fact::Zero(ValueIdentity::Register(self.regs[component])));
        }
        !zero.is_empty() && !Clause::both(&path, &Clause::facts(zero)).possible()
    }
    pub(super) fn retain_payload_nonzero(&mut self, payload: &Value, offset: u32) {
        let mut required = Clause::default();
        for (component, (scalar, at)) in payload.ty.components.iter().enumerate() {
            if !scalar.is_integer() || scalar.size() <= 4 || !payload.component_nonzero(component) { continue; }
            let mut either = Clause::impossible();
            for (word, (part, position)) in self.ty.components.iter().enumerate().skip(1) {
                if *position < offset + at || position + part.size() > offset + at + scalar.size() { continue; }
                either = Clause::either(&either, &Clause::facts(
                    [Fact::NonZero(ValueIdentity::Register(self.regs[word]))].into()));
            }
            required = Clause::both(&required, &either);
        }
        if matches!(&**required.0, ClauseNode::Facts(facts) if facts.is_empty()) { return; }
        let mut condition = Condition::component(self, 0);
        for case in &mut condition.cases { *case = Clause::both(case, &required); }
        self.proof.conditions.resize_with(self.regs.len(), || None);
        self.proof.conditions[0] = Some(Box::new(condition));
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
    pub(super) fn component_range_intervals(&self, component: usize) -> Vec<(Bits, Bits)> {
        let scalar = self.ty.components[component].0;
        let bits = |ordered| if scalar.is_float() { floating::bits(scalar, ordered) } else { ordered ^ scalar.order_bias() };
        self.component_ranges(component).0.into_iter().map(|(lower, upper)| (bits(lower), bits(upper))).collect()
    }
    fn component_ranges(&self, component: usize) -> ScalarRanges {
        let scalar = self.ty.components[component].0;
        if scalar.is_float() { return floating::ranges(self, component); }
        if !scalar.is_integer() { return ScalarRanges::default(); }
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
    pub(super) fn component_origins(&self, index: usize) -> Vec<ValueIdentity> {
        self.proof
            .origins
            .get(index)
            .filter(|origins| !origins.is_empty())
            .cloned()
            .unwrap_or_else(|| vec![ValueIdentity::Register(self.regs[index])])
    }
    pub(super) fn same_component(&self, index: usize, other: &Self, other_index: usize) -> bool {
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
            addresses: if self.proof.addresses.is_empty(){Vec::new()}else{indices.iter().map(|i|self.proof.addresses.get(*i).cloned().flatten()).collect()},
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
pub(super) struct Clause(ManuallyDrop<Arc<ClauseNode>>);
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
impl Drop for Clause {
    fn drop(&mut self) {
        // 同一引用只消费一次；ManuallyDrop字段不会在这里再次自动析构。
        let mut next = Some(unsafe { ManuallyDrop::take(&mut self.0) });
        let mut pending: Vec<Arc<ClauseNode>> = Vec::new();
        while let Some(node) = next.take().or_else(|| pending.pop()) {
            let Some(node) = Arc::into_inner(node) else { continue; };
            match node {
                ClauseNode::Both(a, b) | ClauseNode::Either(a, b) => {
                    pending.push(b.into_node());
                    next = Some(a.into_node());
                }
                ClauseNode::Branch(parent, condition, _) => {
                    if let Some(Condition { cases }) = Arc::into_inner(condition) {
                        pending.extend(cases.map(Self::into_node));
                    }
                    next = Some(parent.into_node());
                }
                ClauseNode::Impossible | ClauseNode::Facts(_) => {}
            }
        }
    }
}
impl Clause {
    pub(super) fn values<'a>(values:impl IntoIterator<Item=(Scalar,ValueIdentity,&'a Proof)>)->Self {
        Self::facts(values.into_iter().flat_map(|(scalar,id,proof)|proof.clone().with_identity(id).facts(&Ty::scalar(scalar))).collect())
    }
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,Proof>)->Self {
        Condition::guard(self.clone()).substitute_values(values).assume(&Clause::default(),true)
    }

    fn node(value: ClauseNode) -> Self {
        Self(ManuallyDrop::new(Arc::new(value)))
    }
    fn into_node(self) -> Arc<ClauseNode> {
        let mut owned = ManuallyDrop::new(self);
        // 已消费Clause不再运行Drop，直接转交它原来持有的Arc。
        unsafe { ManuallyDrop::take(&mut owned.0) }
    }
    fn negate(&self)->Self{
        fn fact(value:Fact)->Clause{match value{
            Fact::Minimum(_, _, 0)=>Clause::impossible(),
            Fact::Maximum(_, scalar, n) if n==scalar.bit_mask()=>Clause::impossible(),
            Fact::Minimum(id, scalar, n)=>Clause::facts([Fact::Maximum(id, scalar, n-1)].into()),
            Fact::Maximum(id, scalar, n)=>Clause::facts([Fact::Minimum(id, scalar, n+1)].into()),
            Fact::NonZero(id)=>Clause::facts([Fact::Zero(id)].into()),
            Fact::Zero(id)=>Clause::facts([Fact::NonZero(id)].into()),
            Fact::Boolean(id,truth)=>Clause::facts([Fact::Boolean(id,!truth)].into()),
            Fact::Predicate(predicate,truth)=>Clause::facts([Fact::Predicate(predicate,!truth)].into()),
            Fact::Equal(a,b)=>Clause::facts([Fact::NotEqual(a,b)].into()),
            Fact::NotEqual(a,b)=>Clause::facts([Fact::equal(a,b)].into()),
            Fact::Below(a,b,strict,signed)=>Clause::facts([Fact::Below(b,a,!strict,signed)].into()),
            Fact::Index(id,IndexDomain::Fixed(n))=>Clause::facts([Fact::Minimum(id, Scalar::U32, Bits::from(n))].into()),
            Fact::Index(id,d)=>Clause::facts([Fact::Outside(id,d)].into()),
            Fact::Outside(id,d)=>Clause::facts([Fact::Index(id,d)].into()),
        }}
        let mut values=BTreeMap::new();
        for clause in Self::ordered(std::slice::from_ref(self)){
            let next=match &**clause.0{
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
                match &**clause.0 {
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
            let next = match &**clause.0 {
                ClauseNode::Impossible => clause.clone(),
                ClauseNode::Facts(facts) => {
                    let renamed: Facts = facts
                        .iter()
                        .map(|fact|fact.map(&identity))
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
    pub(super) fn selection(index: ownership::ApplicationIndex, element: usize) -> Self {
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
        Self::node(ClauseNode::Impossible)
    }
    fn facts(facts: Facts) -> Self {
        if contradictory(&facts) {
            Self::impossible()
        } else {
            Self::node(ClauseNode::Facts(facts))
        }
    }
    pub(super) fn both(a: &Self, b: &Self) -> Self {
        match (&**a.0, &**b.0) {
            (ClauseNode::Impossible, _) | (_, ClauseNode::Impossible) => Self::impossible(),
            (ClauseNode::Facts(facts), _) if facts.is_empty() => b.clone(),
            (_, ClauseNode::Facts(facts)) if facts.is_empty() => a.clone(),
            (ClauseNode::Facts(a), ClauseNode::Facts(b)) => {
                Self::facts(a.union(b).copied().collect())
            }
            _ if Arc::ptr_eq(&a.0, &b.0) => a.clone(),
            _ => Self::node(ClauseNode::Both(a.clone(), b.clone())),
        }
    }
    fn observed(parent:&Self,facts:Facts)->Self {
        let known=Arc::new(Condition{cases:[Self::impossible(),Self::facts(facts)]});
        Self::branch(parent,&known,true)
    }
    fn branch(parent: &Self, condition: &Arc<Condition>, truth: bool) -> Self {
        if matches!(&**parent.0, ClauseNode::Impossible)
            || matches!(
                &**condition.cases[usize::from(truth)].0,
                ClauseNode::Impossible
            )
        {
            return Self::impossible();
        }
        Self::node(ClauseNode::Branch(
            parent.clone(),
            condition.clone(),
            truth,
        ))
    }
    fn joined_branches(a: &Self, b: &Self) -> Option<Self> {
        match (&**a.0, &**b.0) {
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
        if let ClauseNode::Either(x, y) = &**a.0 {
            if let Some(parent) = Self::joined_branches(y, b) {
                return Self::either(x, &parent);
            }
            if let Some(parent) = Self::joined_branches(x, b) {
                return Self::either(&parent, y);
            }
        }
        if let ClauseNode::Either(x, y) = &**b.0 {
            if let Some(parent) = Self::joined_branches(a, x) {
                return Self::either(&parent, y);
            }
            if let Some(parent) = Self::joined_branches(a, y) {
                return Self::either(x, &parent);
            }
        }
        match (&**a.0, &**b.0) {
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
            _ if matches!(a.query_inputs(), Some((left,right,false))
                if Arc::ptr_eq(&left.0,&b.0)||Arc::ptr_eq(&right.0,&b.0)) => b.clone(),
            _ if matches!(b.query_inputs(), Some((left,right,false))
                if Arc::ptr_eq(&left.0,&a.0)||Arc::ptr_eq(&right.0,&a.0)) => a.clone(),
            _ => Self::node(ClauseNode::Either(a.clone(), b.clone())),
        }
    }
    fn query_inputs(&self) -> Option<(&Self, &Self, bool)> {
        match &**self.0 {
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
                match &**clause.0 {
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
        fn insert(terms:&mut BTreeSet<Facts>,facts:Facts)->bool{
            if terms.iter().any(|old|old.is_subset(&facts)||!Clause::facts(facts.clone()).overlaps(&Clause::facts(old.clone()).negate())){return false;}
            terms.retain(|old|!facts.is_subset(old)&&Clause::facts(old.clone()).overlaps(&Clause::facts(facts.clone()).negate()));terms.insert(facts)
        }
        fn facts(value:&Facts)->Option<Facts>{
            if contradictory(value){return None;}
            let lower=minimums(value);
            let mut next=value.iter().filter(|fact|!matches!(fact,Fact::Below(a,b,false,scalar)if a==b&&scalar.is_integer())&&!matches!(fact,Fact::Index(id,IndexDomain::Upper(limit,0))if id==limit)&&!matches!(fact,Fact::Minimum(..)|Fact::Maximum(..))&&!matches!(fact,Fact::NonZero(id)if lower.iter().any(|((other,scalar),n)|other==id&&*n>1&&scalar.is_integer()&&!scalar.is_signed()))).copied().collect::<Facts>();
            next.extend(lower.into_iter().filter(|((id,scalar),n)|*n!=0&&!(scalar.is_integer()&&value.contains(&Fact::Zero(*id)))).map(|((id,scalar),n)|if n==1&&scalar.is_integer()&&!scalar.is_signed(){Fact::NonZero(id)}else{Fact::Minimum(id, scalar, n)}));
            next.extend(maximums(value).into_iter().filter(|((id,scalar),n)|*n!=scalar.bit_mask()&&!(scalar.is_integer()&&value.contains(&Fact::Zero(*id)))).map(|((id,scalar),n)|if n==0&&scalar.is_integer()&&!scalar.is_signed(){Fact::Zero(id)}else{Fact::Maximum(id, scalar, n)}));
            Some(next)
        }
        let mut values:BTreeMap<usize,BTreeSet<Facts>>=BTreeMap::new();
        for clause in Self::ordered(std::slice::from_ref(self)){
            let next=match &**clause.0{
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
        let mut terms=values.remove(&self.key()).unwrap();
        drop(values);
        // Consensus removes the short-circuit partition: A || (!A && B)
        // has the same complete sum as A || B. Negation stays with Clause.
        loop {
            let mut additions=BTreeSet::new();
            for left in &terms {
                for right in terms.range((std::ops::Bound::Excluded(left),std::ops::Bound::Unbounded)) {
                    for fact in left {
                        let opposite=Self::facts([*fact].into()).negate();
                        let ClauseNode::Facts(opposite)=&**opposite.0 else{continue};
                        let Some(other)=opposite.first().filter(|_|opposite.len()==1)else{continue};
                        if !right.contains(other){continue;}
                        let combined=left.iter().filter(|value|*value!=fact)
                            .chain(right.iter().filter(|value|*value!=other)).copied().collect();
                        if let Some(value)=facts(&combined){insert(&mut additions,value);}
                    }
                }
            }
            let mut changed=false;
            for value in additions{changed|=insert(&mut terms,value);}
            if !changed{break;}
        }
        terms.into_iter().fold(Self::impossible(),|all,facts|Self::either(&all,&Self::facts(facts)))
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
                match &**facts[index].clause.0 {
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
                            if (matches!(&**facts[left].clause.0, ClauseNode::Either(..)) && additions_conflict(&environment, &a))
                                || (matches!(&**facts[right].clause.0, ClauseNode::Either(..)) && additions_conflict(&environment, &b))
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
        match &**self.0 {
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
                match &**clause.0 {
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
        Fact::Predicate(predicate, truth) => Fact::Predicate(predicate.map(|id|Some(root(id))).unwrap(), truth),
        Fact::Index(id, domain) => Fact::Index(root(id), map_domain(domain,&root)),
        Fact::Outside(id, domain) => Fact::Outside(root(id), map_domain(domain,&root)),
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
        let mut lower = match select(Fact::Minimum(id, scalar, scalar.bit_mask()), true) {
            Some(Fact::Minimum(other, actual, bound)) if (other, actual) == (id, scalar) => bound,
            _ => 0,
        };
        let upper = match select(Fact::Maximum(id, scalar, 0), false) {
            Some(Fact::Maximum(other, actual, bound)) if (other, actual) == (id, scalar) => bound,
            _ => scalar.bit_mask(),
        };
        if scalar.is_integer() && !scalar.is_signed() && contains(Fact::NonZero(id)) { lower=lower.max(1); }
        if contains(Fact::Zero(id)) { (lower.max(scalar.order_bias()),upper.min(scalar.order_bias())) } else { (lower,upper) }
    };
    let conflict = |fact| match fact {
        Fact::Boolean(id, truth) => contains(Fact::Boolean(id, !truth)),
        Fact::Predicate(predicate, truth) => contains(Fact::Predicate(predicate, !truth)) || {
            let [a,b]=[predicate.a,predicate.b].map(|operand|match operand {
                Operand::Constant(_,bits)=>Some(bits),
                Operand::Value(id)=>{let(a,b)=bounds(id,predicate.scalar);(a==b).then(||if predicate.scalar.is_float(){floating::bits(predicate.scalar,a)}else{a^predicate.scalar.order_bias()})},
            });
            a.zip(b).and_then(|(a,b)|constant::binary(BinaryOp::Compare(predicate.op),predicate.scalar,a,b)).is_some_and(|value|value!=Bits::from(truth))
        },
        Fact::Zero(id) => contains(Fact::NonZero(id)),
        Fact::NonZero(id) => contains(Fact::Zero(id)),
        Fact::Index(id, domain) => contains(Fact::Outside(id, domain)),
        Fact::Outside(id, domain) => matches!(domain,IndexDomain::Upper(limit,0)if id==limit)||contains(Fact::Index(id, domain)),
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
            || contains(Fact::Below(b, a, true, scalar))
            || (strict && contains(Fact::Below(b, a, false, scalar))) || {
                let (lower,_)=bounds(a,scalar);let (_,upper)=bounds(b,scalar);
                lower>upper || strict && lower==upper
            },
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
        Fact::Predicate(predicate, truth) => other.contains(&Fact::Predicate(predicate, !truth)),
        Fact::NonZero(id) => other.contains(&Fact::Zero(id)),
        Fact::Zero(id) => other.contains(&Fact::NonZero(id)),
        Fact::Index(id, domain) => other.contains(&Fact::Outside(id, domain)),
        Fact::Outside(id, domain) => other.contains(&Fact::Index(id, domain)),
        Fact::Below(a, b, strict, scalar) => {
            other.contains(&Fact::Below(b, a, true, scalar))
                || (strict && (other.contains(&Fact::Below(b, a, false, scalar))
                    || other.contains(&Fact::equal(a, b))))
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
            let left = additional_equality_roots(environment, environment_roots, &a);
            let right = additional_equality_roots(environment, environment_roots, &b);
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
            for (id,scalar) in a.union(&b).filter_map(|fact|match fact{Fact::Index(id,IndexDomain::KnownBits(scalar,..))=>Some((*id,*scalar)),_=>None}) {
                if let Some(bits)=KnownBits::facts(scalar,id,[environment,&a]).zip(KnownBits::facts(scalar,id,[environment,&b])).and_then(|(a,b)|a.common(b)) {
                    let fact=Fact::Index(id,bits.domain());if !environment.contains(&fact){common.insert(fact);}
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
impl Proof{
    pub(super) fn element_operand(index:Operand,range:(u32,u32))->Self{
        let mut proof=match index{
            Operand::Value(id)=>Self::unsigned(range.0,range.1).with_identity(id),
            Operand::Constant(_,bits)=>Self::unsigned(bits as u32,bits as u32),
        };
        let condition=match index{
            Operand::Value(id)=>Condition::guard(Clause::facts([Fact::NonZero(id)].into())),
            Operand::Constant(_,bits)=>Condition::constant(bits!=0),
        };
        proof.conditions=vec![Some(Box::new(condition))];proof
    }
    pub(super) fn scope_element_indices(&self,depth:usize)->Self{
        let shifts=self.scalar_identities().iter().filter_map(|id|if let ValueIdentity::ElementIndex(index)=id{Some((*id,ValueIdentity::ElementIndex(index+depth)))}else{None}).collect();
        self.output_bound(&shifts)
    }
}
impl Condition {
    pub(super) fn scope_element_indices(&self,depth:usize)->Self {
        let names=Proof::condition(self.clone()).scalar_identities().into_iter().filter_map(|id|match id{
            ValueIdentity::ElementIndex(index)=>Some((id,ValueIdentity::ElementIndex(index+depth))),_=>None,
        }).collect();
        self.output_bound(&names)
    }
    pub(super) fn nonzero_value(value:&Value)->Self {
        if value.is_nonzero(){return Self::constant(true);}
        if (0..value.regs.len()).any(|i|value.component_ranges(i).0.as_slice()==[(value.ty.components[i].0.order_bias(),value.ty.components[i].0.order_bias())]){return Self::constant(false);}
        (0..value.regs.len()).fold(Self::constant(true),|condition,i|condition.logical(Self::component(value,i),false))
    }
    pub(super) fn nonzero_proof(proof:&Proof,component:usize)->Self {
        match proof.known_identity(component){Some(id)=>Self::guard(Clause::facts([Fact::NonZero(id)].into())),None=>Self::default()}
    }
    pub(super) fn index_value(value:&Value,domain:IndexDomain)->Self {
        Self::guard(Clause::facts((0..value.regs.len()).map(|i|Fact::Index(value.component_identity(i),domain)).collect()))
    }
    pub(super) fn same_length_proofs(a:&Proof,b:&Proof)->Self {
        match(a.known_identity(0),b.known_identity(0)){(Some(a),Some(b))=>Self::guard(Clause::facts([Fact::Equal(a,b)].into())),_=>Self::default()}
    }

    fn identities(&self,identity:&impl Fn(ValueIdentity)->ValueIdentity) {
        for clause in Clause::ordered(&self.cases){if let ClauseNode::Facts(facts)=&**clause.0{for fact in facts{fact.map(identity);}}}
    }
    pub(super) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,Proof>)->Self {
        let proof=Proof::condition(self.clone()).substitute_values(values);
        proof.conditions.into_iter().next().flatten().map(|value|*value).unwrap()
    }

    pub(super) fn element_index(depth:usize,index:Operand)->Self {
        Self::index_equal(Operand::Value(ValueIdentity::ElementIndex(depth)),index)
    }
    pub(super) fn index_equal(a:Operand,b:Operand)->Self {
        let facts=match(a,b){
            (Operand::Constant(_,a),Operand::Constant(_,b))=>return Self::constant(a==b),
            (Operand::Value(a),Operand::Value(b))=>{
                let predicate=ScalarPredicate::new(Comparison::Equal,Scalar::U32,Operand::Value(a),Operand::Value(b));
                return Self{cases:[
                    Clause::facts([Fact::NotEqual(a.min(b),a.max(b)),Fact::Predicate(predicate,false)].into()),
                    Clause::facts([Fact::equal(a,b),Fact::Predicate(predicate,true)].into()),
                ]};
            },
            (Operand::Value(id),Operand::Constant(_,bits))|(Operand::Constant(_,bits),Operand::Value(id))=>[Fact::Minimum(id,Scalar::U32,bits),Fact::Maximum(id,Scalar::U32,bits)].into(),
        };
        Self::guard(Clause::facts(facts))
    }
    pub(super) fn index_below((a,a_proof):(Operand,&Proof),(b,b_proof):(Operand,&Proof),strict:bool)->Self {
        let mut possible=if strict{
            [a_proof.unsigned_maximum(0)>=b_proof.unsigned_minimum(0),a_proof.unsigned_minimum(0)<b_proof.unsigned_maximum(0)]
        }else{
            [a_proof.unsigned_maximum(0)>b_proof.unsigned_minimum(0),a_proof.unsigned_minimum(0)<=b_proof.unsigned_maximum(0)]
        };
        let mut facts=[Facts::new(),Facts::new()];
        match(a,b){
            (Operand::Constant(_,a),Operand::Constant(_,b))=>return Self::constant(if strict{a<b}else{a<=b}),
            (Operand::Constant(..),Operand::Value(_))=>return Self::index_below((b,b_proof),(a,a_proof),!strict).negate(),
            (Operand::Value(a),Operand::Constant(_,b))=>{
                let b=b as u32;
                if let Some(lower)=b.checked_add(u32::from(!strict)){facts[0].insert(Fact::Minimum(a,Scalar::U32,Bits::from(lower)));}else{possible[0]=false;}
                if let Some(upper)=b.checked_sub(u32::from(strict)){facts[1].insert(Fact::Maximum(a,Scalar::U32,Bits::from(upper)));}else{possible[1]=false;}
            },
            (Operand::Value(a),Operand::Value(b))=>{
                if a==b{return Self::constant(!strict);}
                constrain_proofs(&mut facts[0],(b,b_proof),(a,a_proof),!strict,Scalar::U32);
                constrain_proofs(&mut facts[1],(a,a_proof),(b,b_proof),strict,Scalar::U32);
            },
        }
        Self{cases:std::array::from_fn(|i|if possible[i]{Clause::facts(std::mem::take(&mut facts[i]))}else{Clause::impossible()})}
    }
    pub(super) fn guard(path:Clause)->Self{Self{cases:[path.negate(),path]}}
    pub(super) fn execution(path:&Clause)->Self {
        let mut conditions:BTreeMap<usize,Self>=BTreeMap::new();
        for clause in Clause::ordered(std::slice::from_ref(path)){
            let condition=match &**clause.0 {
                ClauseNode::Impossible=>Self::constant(false),
                ClauseNode::Facts(_)=>Self::guard(clause.clone()),
                ClauseNode::Both(a,b)=>conditions[&a.key()].clone().logical(conditions[&b.key()].clone(),false),
                ClauseNode::Either(a,b)=>conditions[&a.key()].clone().logical(conditions[&b.key()].clone(),true),
                ClauseNode::Branch(parent,condition,truth)=>conditions[&parent.key()].clone().logical(if *truth{condition.as_ref().clone()}else{condition.as_ref().clone().negate()},false),
            };
            conditions.insert(clause.key(),condition);
        }
        conditions.remove(&path.key()).unwrap()
    }
    pub(super) fn guaranteed_at_call(&self,bindings:&BTreeMap<ValueIdentity,Proof>)->Self {
        let value=self.at_call(bindings);
        Self::guard(value.cases[0].clone()).negate()
    }
    pub(super) fn guard_at_call(&self,bindings:&BTreeMap<ValueIdentity,Proof>)->Self{
        // 先投影真实输入关系，再求补集；不可用的私有布尔不能成为接口条件。
        let value=self.at_call(bindings);Self::guard(value.cases[1].clone())
    }
    pub(super) fn covers(&self,other:&Self)->bool{!self.cases[0].overlaps(&other.cases[1])}
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
    pub(super) fn unconstrained(&self)->bool{self.cases.iter().all(|case|matches!(&**case.0,ClauseNode::Facts(facts)if facts.is_empty()))}
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
    pub(super) fn mentions(&self,identity:ValueIdentity)->bool{Clause::ordered(&self.cases).iter().any(|clause|matches!(&**clause.0,ClauseNode::Facts(facts)if facts.iter().any(|fact|match fact{Fact::Equal(a,b)|Fact::NotEqual(a,b)|Fact::Below(a,b,..)=>*a==identity||*b==identity,Fact::Predicate(predicate,_)=>predicate.mentions(identity),Fact::Index(a,_)|Fact::Outside(a,_)|Fact::Zero(a)|Fact::Minimum(a,..)|Fact::Maximum(a,..)|Fact::NonZero(a)|Fact::Boolean(a,_)=>*a==identity})))}
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
            .map(|case| matches!(&**case.0, ClauseNode::Impossible))
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
        let scalar = value.ty.components[component].0;
        let identity = value.component_identity(component);
        for (truth, case) in condition.cases.iter_mut().enumerate() {
            let fact = if scalar == Scalar::Bool {
                Fact::Boolean(identity, truth != 0)
            } else if scalar.is_integer() {
                if truth != 0 { Fact::NonZero(identity) } else { Fact::Zero(identity) }
            } else { continue; };
            let mut facts = Facts::from([fact]);
            facts.extend(value.proof.domains(component).into_iter().filter(|domain|matches!(domain,IndexDomain::CommonRange(..))).map(|domain|Fact::Index(identity,domain)));
            if scalar.is_integer() {
                let (minimum, maximum) = value.component_range(component);
                if minimum != 0 { facts.insert(Fact::Minimum(identity, scalar, minimum)); }
                if maximum != scalar.bit_mask() { facts.insert(Fact::Maximum(identity, scalar, maximum)); }
            }
            *case = Clause::both(case, &Clause::facts(facts));
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
    pub(super) fn branch(c: Self, a: Self, b: Self) -> Self {
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
        let mut result = Self::compare(op, a, b);
        if a.ty.components.len() == 1 && a.ty == b.ty
            && a.ty.components[0].0.is_integer() && a.ty.components[0].0 != Scalar::Bool {
            for value in [a, b] {
                let condition = Self::component(value, 0);
                let facts = Clause::either(&condition.cases[0], &condition.cases[1]);
                for case in &mut result.cases { *case = Clause::both(case, &facts); }
            }
        }
        result
    }
    fn compare(op: Comparison, a: &Value, b: &Value) -> Self {
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
        if a.ty.components.len() != 1 || b.ty != a.ty { return Self::default(); }
        if scalar.is_float() {
            let predicate = ScalarPredicate::new(if op == Comparison::NotEqual { Comparison::Equal } else { op },
                scalar, Operand::of(a, 0), Operand::of(b, 0));
            let condition = Self { cases: std::array::from_fn(|truth|
                Clause::facts(std::iter::once(Fact::Predicate(predicate, truth != 0)).chain([a,b].into_iter().flat_map(|value|value.proof.domains(0).into_iter().filter(|domain|matches!(domain,IndexDomain::CommonRange(..))).map(|domain|Fact::Index(value.component_identity(0),domain)))).collect())) };
            return if op == Comparison::NotEqual { condition.negate() } else { condition };
        }
        if !scalar.is_integer() { return Self::default(); }
        if op==Comparison::NotEqual{return Self::compare(Comparison::Equal,a,b).negate();}
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
                        if other.component_range(0)==(scalar.order_bias(),scalar.order_bias()) {
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
        if op==Comparison::Equal&&a.component_range(0)!=(scalar.order_bias(),scalar.order_bias())&&b.component_range(0)!=(scalar.order_bias(),scalar.order_bias()){cases[0]=Clause::both(&cases[0],&Clause::either(&Self::compare(Comparison::Less,a,b).cases[1],&Self::compare(Comparison::Greater,a,b).cases[1]));}
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
    if !scalar.is_signed() {
        facts.insert(Fact::Index(index_id, IndexDomain::Upper(limit_id, u32::from(strict))));
    }
    if scalar == Scalar::U32 {
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
pub(super) fn math(op: MathOp, args: &[Value]) -> Proof {
    let width = args.iter().map(|value| value.regs.len()).max().unwrap();
    Proof {
        ranges: (0..width).map(|i| floating::math(op, args, i)).collect(),
        ..Proof::default()
    }
}
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
                    let right = b.component_ranges(bi);
                    let signed = scalar.is_signed();
                    let bias = scalar.order_bias();
                    if op == "/" && !signed && a.proof.domains.get(ai).is_some_and(|domains| {
                        domains.iter().any(|domain| matches!(domain,
                            IndexDomain::Computed(IntegerExpression::Binary(
                                IntegerOperation::Remainder, source, _, divisor
                            )) if *source == scalar && *divisor == Operand::of(b, bi)))
                    }) {
                        return left.combine(&right, |_, _| ScalarRanges::interval(bias, bias));
                    }
                    if matches!(op, "-" | "^" | "&" | "|" | "/" | "%" | "min" | "max")
                        && a.same_component(ai, b, bi)
                    {
                        // 同值快照只取共同可达片段；空路径仍为空。
                        let shared = left.combine(&right, |(al, ah), (bl, bh)| {
                            ScalarRanges::interval(al.max(bl), ah.min(bh))
                        });
                        return match op {
                            "&" | "|" | "min" | "max" => shared,
                            _ => {
                                // 整数除余在进入范围计算前已由 binary 证明除数非零。
                                let value = if op == "/" { bias ^ 1 } else { bias };
                                shared.map(|_| ScalarRanges::interval(value, value))
                            }
                        };
                    }
                    let range = arithmetic_ranges(op, scalar, left, right);
                    if !signed && op == "-" {
                        let left_origins = a.component_origins(ai);
                        let right_origins = b.component_origins(bi);
                        if let Some(minimum) = a.proof.domains(ai).into_iter().filter_map(|domain| match domain {
                            IndexDomain::Lower(base, distance) | IndexDomain::Offset(base, distance) if right_origins.contains(&base) => Some(distance),
                            _ => None,
                        }).chain(b.proof.domains(bi).into_iter().filter_map(|domain| match domain {
                            IndexDomain::Upper(limit, distance) if left_origins.contains(&limit) => Some(distance),
                            _ => None,
                        })).max().filter(|&distance| distance != 0) { return range.restrict(Bits::from(minimum), scalar.bit_mask()); }
                    }
                    range
                })
                .collect()
        } else if scalar.is_float() {
            (0..count).map(|i| floating::arithmetic(op, a, b, i)).collect()
        } else { Vec::new() },
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
                        ScalarRanges::interval(al.max(bl), ah.min(bh))
                    });
                exact.domains[0].extend(proof.domains(component));
                proof.assign(&[component], count, exact);
            }
        }
    }
    proof
}
fn arithmetic_ranges(op: &str, scalar: Scalar, left: ScalarRanges, mut right: ScalarRanges) -> ScalarRanges {
    let signed = scalar.is_signed();
    if matches!(op, "<<" | ">>") {
        right = right.shift_counts(scalar);
    }
    let range = left.combine(&right, |left, right| {
        let bitwise = |operation: fn(Bits, Bits) -> Bits| {
            let (lower, upper) = bitwise_range(left, right, scalar, operation);
            ScalarRanges::interval(lower, upper)
        };
        match op {
            "&" => bitwise(|a, b| a & b),
            "|" => bitwise(|a, b| a | b),
            "^" => bitwise(|a, b| a ^ b),
            _ if signed => signed_range(op, left, right, scalar),
            _ => unsigned_range(op, left, right, scalar),
        }
    });
    range
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
    if op == UnaryOp::FloatNegate {
        proof.ranges = (0..value.regs.len()).map(|i| floating::negate(value, i)).collect();
    } else if scalar.is_integer() && !scalar.is_signed() {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => ScalarRanges::interval(!upper & scalar.bit_mask(), !lower & scalar.bit_mask()),
                    UnaryOp::UnsignedNegate => {
                        integer_result_range((-i128::from(upper), -i128::from(lower)), 0, scalar.bit_width())
                    }
                    _ => ScalarRanges::default(),
                })
            })
            .collect();
    } else if scalar.is_integer() && scalar.is_signed() {
        proof.ranges = (0..value.regs.len())
            .map(|i| {
                value.component_ranges(i).map(|(lower, upper)| match op {
                    UnaryOp::BitNot => ScalarRanges::interval(!upper & scalar.bit_mask(), !lower & scalar.bit_mask()),
                    UnaryOp::SignedNegate => integer_result_range(
                        (
                            -i128::from(scalar.signed(upper ^ scalar.order_bias())),
                            -i128::from(scalar.signed(lower ^ scalar.order_bias())),
                        ),
                        scalar.order_bias(), scalar.bit_width(),
                    ),
                    _ => ScalarRanges::default(),
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
fn signed_range(op: &str, (al, ah): (Bits, Bits), (bl, bh): (Bits, Bits), scalar: Scalar) -> ScalarRanges {
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
        _ => return ScalarRanges::interval(0, scalar.bit_mask()),
    };
    integer_result_range(range, scalar.order_bias(), scalar.bit_width())
}
fn integer_result_range((lower, upper): (i128, i128), bias: Bits, width: u32) -> ScalarRanges {
    // 按有序周期投影；不足一个完整周期的跨断点结果保留两侧区间。
    let period = 1i128 << width;
    let mask = (period - 1) as Bits;
    let offset = i128::from(bias);
    if upper - lower >= period - 1 {
        return ScalarRanges::interval(0, mask);
    }
    let (a, b) = ((lower as Bits & mask) ^ bias, (upper as Bits & mask) ^ bias);
    if (lower + offset).div_euclid(period) == (upper + offset).div_euclid(period) {
        ScalarRanges::interval(a, b)
    } else {
        ScalarRanges::from_intervals([(0, b), (a, mask)])
    }
}
fn unsigned_range(op: &str, (al, ah): (Bits, Bits), (bl, bh): (Bits, Bits), scalar: Scalar) -> ScalarRanges {
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
        _ => return ScalarRanges::interval(0, scalar.bit_mask()),
    };
    // u64乘法的完整区间可能超过i128；按无符号数学值投影，避免先截断符号。
    let period = 1u128 << scalar.bit_width();
    if range.1 - range.0 >= period - 1 {
        return ScalarRanges::interval(0, scalar.bit_mask());
    }
    let (lower, upper) = (range.0 as Bits & scalar.bit_mask(), range.1 as Bits & scalar.bit_mask());
    if range.0 / period == range.1 / period {
        ScalarRanges::interval(lower, upper)
    } else {
        ScalarRanges::from_intervals([(0, upper), (lower, scalar.bit_mask())])
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
    pub(super) fn length_proofs_equal(&self, a: &Proof, b: &Proof) -> bool {
        let equal = |a: &Proof, b: &Proof| {
            let ar = a.range_set(0).hull();
            let br = b.range_set(0).hull();
            ar.0 == ar.1 && ar == br
                || a.origins.first().into_iter().flatten()
                    .any(|id| b.origins.first().is_some_and(|origins| origins.contains(id)))
        };
        if equal(a,b) { return true; }
        let mut environment = environment_facts(&self.env);
        environment.extend(a.facts(&Ty::scalar(Scalar::U32)));
        environment.extend(b.facts(&Ty::scalar(Scalar::U32)));
        let Some(facts) = self.path.required_in(&environment) else { return false; };
        let roots = equality_roots(&facts);
        let mut a = a.clone();
        let mut b = b.clone();
        for proof in [&mut a, &mut b] {
            proof.extend_equalities(&roots);
            for fact in &facts {
                if fact.identity().is_some_and(|identity| proof.origins.first().is_some_and(|origins| origins.contains(&identity))) {
                    proof.refine(&Ty::scalar(Scalar::U32),0,*fact);
                }
            }
        }
        equal(&a,&b)
    }
    pub(super) fn lengths_equal(&self, a: &Value, b: &Value) -> bool {
        self.length_proofs_equal(&Proof::capture(a), &Proof::capture(b))
    }
    pub(super) fn observe_integer_binary(&mut self, op: &str, a: &Value, b: &Value, result: &mut Value) {
        let scalar = a.ty.components[0].0;
        if !scalar.is_integer() || result.ty.components[0].0 == Scalar::Bool { return; }
        let operation = match op {
            "+" => IntegerOperation::Add, "-" => IntegerOperation::Subtract,
            "*" => IntegerOperation::Multiply, "/" => IntegerOperation::Divide,
            "%" => IntegerOperation::Remainder, "&" => IntegerOperation::And,
            "|" => IntegerOperation::Or, "^" => IntegerOperation::Xor,
            "<<" => IntegerOperation::ShiftLeft, ">>" => IntegerOperation::ShiftRight,
            _ => return,
        };
        let facts = (0..result.regs.len()).map(|component| {
            let left = Operand::of(a, component % a.regs.len());
            let right = Operand::of(b, component % b.regs.len());
            let domain = IndexDomain::Computed(IntegerExpression::Binary(operation, scalar, left, right));
            result.proof.mark_domain(component, domain);
            Fact::Index(result.component_identity(component), domain)
        }).collect();
        self.path = Clause::observed(&self.path,facts);
    }
    pub(super) fn observe_bounds(&mut self, value: &Value) {
        self.path = Clause::observed(&self.path,Proof::capture(value).facts(&value.ty));
    }
    fn selected_proof(
        path: &Clause,
        mut environment: Facts,
        proof: &Proof,
        source: &Ty,
        ty: &Ty,
        choices: &[(Vec<usize>, Clause)],
        registers: Option<&[Reg]>,
    ) -> (Proof, Clause) {
        environment.extend(proof.facts(source));
        let mut selected: Option<Proof> = None;
        let mut related = Clause::impossible();
        for (components, condition) in choices {
            let mut next = proof.project(components, source.components.len());
            if let Some(registers) = registers {
                next.origins.resize_with(ty.components.len(), Vec::new);
                for (origins, register) in next.origins.iter_mut().zip(registers) {
                    let identity = ValueIdentity::Register(*register);
                    origins.retain(|origin| *origin != identity);
                    origins.insert(0, identity);
                }
            }
            // 一次聚合读取的所有分量属于同一候选；只求解本候选的结果绑定。
            let binding = Clause::both(condition, &Clause::facts(next.facts(ty)));
            related = Clause::either(&related, &binding);
            let candidate_path = Clause::both(path, &binding);
            let Some(context) = candidate_path.required_in(&environment) else {
                continue;
            };
            next.extend_equalities(&equality_roots(&context));
            for (component, _) in ty.components.iter().enumerate() {
                for fact in &context {
                    if fact.identity().is_some_and(|identity| next.origins.get(component).is_some_and(|origins| origins.contains(&identity))) {
                        next.refine(ty, component, *fact);
                    }
                }
            }
            next.under_path(ty, &candidate_path);
            match &mut selected {
                Some(selected) => selected.join(&next, ty.components.len(), None),
                None => selected = Some(next),
            }
        }
        let path = if registers.is_some() { Clause::both(path, &related) } else { path.clone() };
        (selected.unwrap_or_default(), path)
    }
    pub(super) fn array_value_proof(
        &mut self,
        proof: &Proof,
        array: &Ty,
        index: ownership::ApplicationIndex,
        registers: &[Reg],
    ) -> Proof {
        let (element, count) = array.indexed_element().unwrap();
        let width = element.components.len();
        let choices: Vec<_> = (0..count as usize)
            .map(|i| ((i * width..(i + 1) * width).collect(), Clause::selection(index, i)))
            .collect();
        let (result, path) = Self::selected_proof(
            &self.path, environment_facts(&self.env), proof, array, &element, &choices, Some(registers),
        );
        self.path = path;
        result
    }
    pub(super) fn projected_owned_proof(
        &mut self,
        place: &Place,
        registers: Option<&[Reg]>,
    ) -> Option<Proof> {
        fn collect(
            positions: &[&Position],
            owner: &[Position],
            selections: &[(ownership::ApplicationIndex, usize)],
            path: Clause,
            choices: &mut Vec<(Vec<usize>, Clause)>,
        ) -> Option<()> {
            let mut components = Vec::with_capacity(positions.len());
            let mut unresolved = None;
            for position in positions {
                let mut position = *position;
                loop {
                    if let Some(component) = owner.iter().position(|candidate| candidate == position) {
                        components.push(component);
                        break;
                    }
                    let Position::Indexed(elements, index) = position else {
                        return None;
                    };
                    if let Some((_, selected)) = selections.iter().find(|(selected, _)| selected == index) {
                        // 同一实际下标必须同时存在于本次读取的各分量候选中。
                        let Some(element) = elements.get(*selected) else { return Some(()); };
                        position = element;
                    } else {
                        unresolved = Some((elements, *index));
                        break;
                    }
                }
                if unresolved.is_some() { break; }
            }
            if let Some((elements, index)) = unresolved {
                for i in 0..elements.len() {
                    let mut selections = selections.to_vec();
                    selections.push((index, i));
                    collect(
                        positions, owner, &selections,
                        Clause::both(&path, &Clause::selection(index, i)), choices,
                    )?;
                }
            } else {
                choices.push((components, path));
            }
            Some(())
        }
        let positions = place.owned_positions()?;
        if positions.is_empty() {
            return None;
        }
        let (name, source, choices) = self.env.iter().find_map(|(name, binding)| {
            let owner = binding.stored_place()?;
            if matches!(place.storage(), Place::Element { .. }) && !place.overlaps(&owner) {
                return None;
            }
            let owner_positions = self.positions(&owner);
            let mut choices = Vec::new();
            collect(&positions.iter().collect::<Vec<_>>(), &owner_positions, &[], Clause::default(), &mut choices)?;
            Some((name.clone(), owner.ty().clone(), choices))
        })?;
        let environment = environment_facts(&self.env);
        let (proof, _, _) = self.env.get_mut(&name)?.proof_storage()?;
        if registers.is_some() {
            proof.origins.resize_with(source.components.len(), Vec::new);
            for (components, _) in &choices {
                for component in components {
                    if proof.origins[*component].is_empty() {
                        let register = self.registers.len();
                        self.registers.push(source.components[*component].0);
                        proof.origins[*component].push(ValueIdentity::Register(register));
                    }
                }
            }
        }
        let (result, path) = Self::selected_proof(
            &self.path, environment, proof, &source, place.ty(), &choices, registers,
        );
        self.path = path;
        Some(result)
    }
    pub(super) fn refine_parallel_domain(&mut self) {
        for binding in self.env.values_mut() {
            if let Binding::Data(view) = binding
                && self.program.same_length(view.binding, self.domain)
            {
                view.length.refine(
                    &Ty::scalar(Scalar::U32),
                    0,
                    Fact::Minimum(ValueIdentity::DataLength(self.domain), Scalar::U32, 1),
                );
            }
        }
    }
    pub(super) fn dataset_view(&self, binding: usize, access: Access, whole: bool) -> DatasetView {
        DatasetView {
            binding,
            access,
            whole,
            projection: Vec::new(),
            length: {
                let mut proof = Proof::default().with_identity(ValueIdentity::DataLength(binding));
                for (other, buffer) in self.program.buffers.iter().enumerate() {
                    if self.program.same_length(binding, other) {
                        let related = buffer.count.map_or_else(Proof::default, |n| Proof::unsigned(n,n))
                            .with_domains(Domains::from([IndexDomain::DataLength(other)]))
                            .with_identity(ValueIdentity::DataLength(other));
                        proof.assume_same_length(&related);
                    }
                }
                proof
            },
        }
    }
    pub(super) fn data_length(&mut self, view: &DatasetView) -> Value {
        let r = self.reg(Scalar::U32);
        self.code
            .push(Inst::new(InstKind::Uniform(r, 4 * (2 + view.binding as u32))));
        Value {
            proof: view.length.clone(),
            literal: false,
            ty: Ty::scalar(Scalar::U32),
            regs: vec![r],
            uniform: true,
            constant: None,
        }
    }
    pub(super) fn bound_value(&self, value: &mut Value) {
        for (component, &(scalar, _)) in value.ty.components.iter().enumerate() {
            if scalar != Scalar::U32 { continue; }
            let maximum = value.component_range(component).1;
            let mut below = |source: &Proof, at: usize, minimum: Bits, identity| {
                if maximum <= minimum {
                    let distance = (minimum - maximum) as u32;
                    value.proof.mark_domain(component, IndexDomain::Upper(identity, distance));
                    for domain in indices_below(&source.domains(at), distance != 0) {
                        value.proof.mark_domain(component, domain);
                    }
                }
            };
            for binding in self.env.values() {
                if let Binding::Value(source) = binding {
                    for (at, &(word, _)) in source.ty.components.iter().enumerate() {
                        if word == Scalar::U32 {
                            below(&source.proof, at, source.component_range(at).0, source.component_identity(at));
                        }
                    }
                }
            }
            if !self.in_function && self.global_index != usize::MAX {
                let index = self.path_proof(&Ty::scalar(Scalar::U32), self.data_index_proof(self.domain)
                    .with_domains(Domains::from([IndexDomain::Data(self.domain)]))
                    .with_identity(ValueIdentity::Register(self.global_index)));
                below(&index, 0, Bits::from(index.unsigned_minimum(0)), ValueIdentity::Register(self.global_index));
            }
        }
    }
    pub(super) fn path_proof(&self, ty: &Ty, proof: Proof) -> Proof { scoped_proof(ty, proof, &self.path, &self.env) }
    pub(super) fn data_index_proof(&self, binding: usize) -> Proof {
        let count = self.program.buffers[binding].count.unwrap_or_else(||
            self.resource_length_proofs(IndexDomain::Data(binding)).into_iter()
                .map(|(proof, component)| self.path_proof(&Ty::scalar(Scalar::U32),
                    proof.project(&[component], proof.width())).unsigned_maximum(0))
                .min().unwrap_or(u32::MAX));
        Proof::unsigned(0, count.saturating_sub(1))
    }
    pub(super) fn index_known(&self, value: &Value, domain: IndexDomain, limit: u32, lengths: &[(&Proof, usize)]) -> bool {
        value.proof.domains(0).iter().any(|source| {
            *source == domain || matches!((domain, source),
                (IndexDomain::Data(target), IndexDomain::Data(other)) if self.program.same_length(target, *other))
        })
            || value.component_ranges(0).0.iter().all(|&(_, upper)| upper < Bits::from(limit))
            || lengths.iter().any(|(proof, component)| value.proof.below_length(&value.component_origins(0), proof, *component, true))
    }
    pub(super) fn index_in(&self, value: &Value, domain: IndexDomain, minimum: u32, selected_length: Option<&Proof>) -> bool {
        let mut lengths = self.resource_length_proofs(domain);
        lengths.extend(selected_length.map(|proof| (proof, 0)));
        let minimum = if let IndexDomain::Fixed(count) = domain { count } else { minimum };
        let limit = lengths.iter().map(|(proof, component)| proof.unsigned_minimum(*component)).fold(minimum, u32::max);
        if self.index_known(value, domain, limit, &lengths) { return true; }
        // 非固定范围消费同一条原路径；运算仍只求值一次，不新增运行时检查。
        let mut value = value.clone();
        value.proof = Proof::capture(&value);
        let mut environment = environment_facts(&self.env);
        environment.extend(value.proof.facts(&value.ty));
        let mut known=Clause::both(&self.path,&Clause::facts(environment));
        for(proof,component)in std::iter::once((&value.proof,0)).chain(lengths.iter().map(|(proof,component)|(*proof,*component))){
            if let Some(condition)=proof.component_condition(component){known=Clause::both(&known,&Clause::either(&condition.cases[0],&condition.cases[1]));}
        }
        let id=value.identity();
        let mut outside=Clause::facts([Fact::Outside(id,domain),Fact::Minimum(id,Scalar::U32,Bits::from(limit))].into());
        for(proof,component)in lengths {
            known=Clause::both(&known,&Clause::facts(proof.project(&[component],proof.width()).facts(&Ty::scalar(Scalar::U32))));
            for target in proof.origins.get(component).into_iter().flatten(){
                outside=Clause::both(&outside,&Clause::facts([Fact::Below(*target,id,false,Scalar::U32)].into()));
            }
        }
        !known.overlaps(&outside)
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
                Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => value = Some((owner.place.ty(), &owner.proof)),
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
            if let Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } = binding {
                if let Some(Binding::Owned(current) | Binding::ViewElement { element: current, .. }) = self.env.get(name) {
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
            Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => matches!(owner.place.storage(), Place::ApplicationElement { .. })
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
                    Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => owner.proof.extend_equalities(&roots),
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
                        Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => {
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
            // Only differing existing identities need an output name; unread values are named at their actual read.
            let components=Proof::fact_components(ty).filter(|&i|
                a.known_identity(i).zip(b.known_identity(i)).is_some_and(|(a,b)|a!=b)
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
                (Binding::Owned(a), Binding::Owned(b))
                | (Binding::ViewElement { element: a, .. }, Binding::ViewElement { element: b, .. }) => {
                    a.place.join_position_range(&b.place);
                    a.proof
                        .join_condition(&b.proof,a.place.ty().components.len(),Some(&choice));
                    a.uniform &= b.uniform;
                }
                (Binding::Data(a), Binding::Data(b)) => a.length.join(&b.length, 1, None),
                (Binding::OwnedValue(a), Binding::OwnedValue(b)) => {
                    let mut value=returns::Returned::OwnedValue(a.clone());value.join_presence(returns::Returned::OwnedValue(b.clone()),when);let returns::Returned::OwnedValue(value)=value else{unreachable!()};*a=value;
                }
                (Binding::ApplicationData(a), Binding::ApplicationData(b)) => {
                    let mut value=returns::Returned::Dataset(a.clone());value.join_presence(returns::Returned::Dataset(b.clone()),when);let returns::Returned::Dataset(value)=value else{unreachable!()};*a=value;
                }
                (Binding::Tile(a), Binding::Tile(b)) => a.length.join(&b.length, 1, None),
                _ => {}
            }
        }
    }
    pub(super) fn common_element_proof(&self, binding: usize, words: &[u32]) -> Option<Proof> {
        let common = self.element_facts?.get(&binding)?;
        let ty = &self.program.buffers[binding].ty;
        let components = words.iter().map(|word| ty.components.iter().position(|(_, at)| at == word).unwrap()).collect::<Vec<_>>();
        let mut common = common.project(&components, ty.components.len());
        let changed = components.iter().enumerate().filter_map(|(component, &source)| {
            let (scalar, offset) = ty.components[source];
            self.memory_regions.get(&binding).is_some_and(|(_, writes)| writes.keys().any(|&(start, end)|
                u64::from(offset) < u64::from(end) && u64::from(start) < u64::from(offset)+u64::from(scalar.size()))).then_some(component)
        }).collect::<Vec<_>>();
        common.forget_value(&changed, words.len());
        Some(common.element_facts(|target| self.data_index_proof(target)
            .with_domains(Domains::from([IndexDomain::Data(target)]))))
    }
    pub(super) fn read_owned_proof(&mut self, place: &Place, regs: &[Reg], reads: &[(usize,usize)]) -> Result<(Proof, bool)> {
        let (snapshots,common)=match place {
            Place::ApplicationElement{..}=>{
                let snapshots=regs.iter().map(|reg|self.application_read_snapshot(*reg)).collect::<Vec<_>>();
                let common=self.application_element_proof(place,&snapshots,reads)?;(snapshots,common)
            },
            _=>(Vec::new(),None),
        };
        let mut stored = None;
        for binding in self.env.values_mut() {
            let Some((proof, uniform, owner)) = binding.proof_storage() else {
                continue;
            };
            let Some(components) = place.components_in(&owner) else {
                continue;
            };
            let width = owner.ty().components.len();
            proof.origins.resize_with(width, Vec::new);
            for (target,(component,reg)) in components.iter().zip(regs).enumerate() {
                if proof.origins[*component].is_empty() {
                    proof.origins[*component].push(ValueIdentity::Register(*reg));
                    if let Some(common)=&common{*proof=std::mem::take(proof).with_view_borrow(*component,common.view_borrow(target));}
                }
            }
            stored = Some((proof.project(&components, width), *uniform));
            break;
        }
        Ok(match place {
            Place::ApplicationElement { .. } => {
                let mut proof = match (stored, common) {
                    (Some((mut proof, _)), Some(common)) => {
                        proof.constrain_common_ranges(&common, regs.len());
                        proof
                    }
                    (Some((proof, _)), None) | (None, Some(proof)) => proof,
                    (None, None) => self.projected_owned_proof(place, Some(regs)).unwrap_or_default(),
                };
                proof.origins.resize_with(regs.len(), Vec::new);
                for (origins, snapshot) in proof.origins.iter_mut().zip(snapshots) {
                    let identity = ValueIdentity::Register(snapshot);
                    origins.retain(|origin| *origin != identity);
                    origins.insert(0, identity);
                }
                (self.path_proof(place.ty(), proof), false)
            }
            Place::Element { binding, words, .. } => {
                let (mut proof, uniform) = match stored {
                    Some(stored) => stored,
                    None => (self.projected_owned_proof(place, Some(regs)).unwrap_or_default(), false),
                };
                if let Some(common) = self.common_element_proof(*binding, words) {
                    proof.constrain_common_ranges(&common, regs.len());
                }
                (proof, uniform)
            }
            _ => match stored {
                Some(stored) => stored,
                None => (self.projected_owned_proof(place, Some(regs)).unwrap_or_default(), false),
            },
        })
    }
    pub(super) fn update_application_contents(&mut self, data: &application::DataReference, changed: &[u32], update: application::ElementUpdate<'_>)->Result<()> {
        self.update_application_elements(&data.sources,&view::resource_borrow(&data.data),changed,update)?;
        for binding in self.env.values_mut() {
            let Some((proof, uniform, owner)) = binding.proof_storage() else {
                continue;
            };
            let Place::ApplicationElement { sources, words, .. } = owner.storage() else {
                continue;
            };
            if sources.iter().any(|source| data.sources.contains(source)) {
                let components = words.iter().enumerate()
                    .filter_map(|(i, word)| changed.contains(word).then_some(i))
                    .collect::<Vec<_>>();
                if !components.is_empty() {
                    proof.forget_value(&components, owner.ty().components.len());
                    *uniform = false;
                }
            }
        }
        Ok(())
    }
    pub(super) fn assign_owned_proof(&mut self, place: &Place, fresh: Proof, uniform: bool)->Result<()> {
        if let Place::ApplicationElement{sources,resource,words,index_value,index_range,..}=place {self.update_application_elements(sources,resource,words,application::ElementUpdate::Element{proof:&fresh,index:Some((*index_value,*index_range)),when:None})?;}
        if let Place::Indexed{locations,..}=place && let Place::ApplicationElement{sources,resource,..}=place.storage() {
            for(component,position)in locations.iter().enumerate(){
                let mut selected=BTreeSet::new();owned_borrows::positions(position,&mut selected);
                let proof=fresh.project(&[component],place.ty().components.len());
                for position in selected{let Position::Borrowed(_,_,word)=position else{unreachable!()};self.update_application_elements(sources,resource,&[word],application::ElementUpdate::Element{proof:&proof,index:None,when:None})?;}
            }
        }
        let positions=if place.ty().contains_borrows(){self.positions(place)}else{Vec::new()};
        let owners:BTreeMap<_,_>=self.env.iter().filter_map(|(name,binding)| {
            let owner=binding.stored_place()?;
            owner.ty().contains_borrows().then(||(name.clone(),self.positions(&owner)))
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
        Ok(())
    }
}

impl crate::module::codec::Encode for ScalarRanges {
    fn encode(&self, w: &mut crate::module::codec::Writer) {
        crate::module::codec::Encode::encode(&self.0, w);
    }
}
impl crate::module::codec::Decode for ScalarRanges {
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
                let node = match &**clause.0 {
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
        // 缺失分量与原读取规则的空借用相同；容器尾部空槽不属于合同身份。
        let width = self.views.iter()
            .rposition(|borrow| !borrow.sources.is_empty())
            .map_or(0, |index| index + 1);
        self.views[..width].encode(w);
        let width=self.addresses.iter().rposition(Option::is_some).map_or(0,|i|i+1);self.addresses[..width].encode(w);
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
                    EncodedClause::Both(a, b) => Clause::node(ClauseNode::Both(node(&nodes, a)?, node(&nodes, b)?)),
                    EncodedClause::Either(a, b) => {
                        Clause::node(ClauseNode::Either(node(&nodes, a)?, node(&nodes, b)?))
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
            addresses: crate::module::codec::Decode::decode(r)?,
        })
    }
}
crate::module::codec::variants!(Fact{0=>Equal(a,b),1=>Index(value,domain),2=>Minimum(value,scalar,lower),3=>Maximum(value,scalar,upper),4=>NonZero(value),5=>Boolean(value,truth),6=>Below(lower,upper,strict,scalar),7=>Outside(value,domain),8=>Zero(value),9=>NotEqual(a,b),10=>Predicate(predicate,truth)});

pub(super) fn scoped_proof(ty: &Ty, mut proof: Proof, path: &Clause, env: &BTreeMap<String, Binding>) -> Proof {
    let environment=environment_facts(env);
    if let Some(facts) = path.required_in(&environment) {
        proof.extend_equalities(&equality_roots(&facts));
        for component in 0..ty.components.len() {
            for domain in proof.domains(component) {
                if let IndexDomain::CommonRange(id,from,low,extended,to)=domain
                    && [from,low,extended,to].into_iter().all(Scalar::is_integer)
                    && let Some(bits)=KnownBits::facts(from,id,[&environment,&facts]) {
                    let bits=bits.cast(low).cast(extended).cast(to);
                    if bits.mask!=0{proof.mark_domain(component,bits.domain());}
                }
            }
            for fact in &facts {
                if fact.identity().is_some_and(|identity| proof.origins.get(component).is_some_and(|origins|origins.contains(&identity))) {
                    proof.refine(ty, component, *fact);
                }
            }
        }
    }
    proof
}

fn common_range_domain(domain: IndexDomain) -> bool {
    match domain {
        IndexDomain::Data(_) | IndexDomain::Fixed(_) | IndexDomain::Tile
        | IndexDomain::View(_) | IndexDomain::ApplicationData(_)
        | IndexDomain::Upper(..) | IndexDomain::Lower(..) | IndexDomain::Room(_)
        | IndexDomain::Shifted(..) | IndexDomain::CommonRange(..) | IndexDomain::KnownBits(..) => true,
        IndexDomain::DataLength(_) | IndexDomain::TileLength | IndexDomain::ViewLength(_)
        | IndexDomain::ApplicationLength(_) | IndexDomain::Sum(..) | IndexDomain::Offset(..)
        | IndexDomain::Computed(_) => false,
    }
}
