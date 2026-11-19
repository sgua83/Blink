//! 范围属于实际参与比较的值；布尔值保存命题，分支只精化仍持有该值的绑定。
mod condition_graph;
mod cross_facts;
mod fact_fields;
mod negation;
mod internal_conflicts;
mod common_facts;
mod task_sources;
mod combination;
mod canonical;
mod equality;
mod integer_relations;
mod index_domains;
mod comparison_conditions;
mod value_conditions;
mod index_conditions;
mod numeric_bounds;
mod execution_conditions;
mod proof_facts;
mod range_sets;
use super::*;
mod floating;
mod bitwise;
mod known_bits;
mod integer;
mod contract;
mod codec;
pub(super) use contract::Normalizer;
use std::{borrow::Cow, collections::{HashMap, HashSet}, mem::ManuallyDrop, sync::Arc};

pub(super) fn seeks(body: &Block) -> bool {
    body.iter().any(|statement| match statement.kind() {
        StmtKind::Seek(..) => true,
        StmtKind::If(_, yes, no) => seeks(&yes) || seeks(&no),
        StmtKind::Match(_, arms) => arms.iter().any(|arm|seeks(&arm.body())),
        // Nested loops own their reentry; their seek cannot replace this cursor.
        _ => false,
    })
}
pub(super) fn for_cursor(begin: &Value, limit: &Value, seeks: bool) -> Proof {
    let minimum = if seeks { 0 } else { begin.minimum() };
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
    fn at_call(self, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> Option<Self> {
        let operand = |operand| Some(match operand {
            Operand::Value(id) => {
                let proof = Proof::bound_input(bindings, &id)?;
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
    fn at_call(self, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> Option<Self> {
        let Self::Binary(op, scalar, a, b) = self;
        let operand = |value| Some(match value {
            Operand::Value(id) => {
                let proof = Proof::bound_input(bindings, &id)?;
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
    fn ranges_at_call(self, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> Option<ScalarRanges> {
        let Self::Binary(op, scalar, a, b) = self;
        let range = |operand| Some(match operand {
            Operand::Value(id) => Proof::bound_input(bindings, &id)?.range_set(0).restrict(0, scalar.bit_mask()),
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
        Some(arithmetic_ranges(operation, scalar, &left, &right))
    }
}



// The existing numeric fact owns one source snapshot and its valid integer domain.
// Upper means value <= a*d+b+c/d (a >= 0); Cast names the actual F32 conversion.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FloatingForm { Cast, Upper }
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FloatingBound {
    source: ValueIdentity,
    lower: Bits,
    upper: Bits,
    terms: [Bits; 3],
    scalar: Scalar,
    form: FloatingForm,
}


type Facts = BTreeSet<Fact>;

struct RequiredFacts {
    facts: Facts,
    roots: BTreeMap<ValueIdentity, ValueIdentity>,
}
// 固定点环境只由本轮持有；节点仅交接环境之外的事实，保持到最后消费。
#[derive(Clone)]
enum QueryFacts<'a> {
    Borrowed(&'a Facts),
    Owned(Arc<Facts>),
}
impl<'a> std::ops::Deref for QueryFacts<'a> {
    type Target = Facts;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(facts) => facts,
            Self::Owned(facts) => facts,
        }
    }
}
impl QueryFacts<'_> {
    fn extend(&mut self, incoming: impl IntoIterator<Item = Fact>) {
        match self {
            Self::Borrowed(facts) => {
                *self = Self::Owned(Arc::new(facts.iter().copied().chain(incoming).collect()));
            }
            Self::Owned(facts) => Arc::make_mut(facts).extend(incoming),
        }
    }
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
        IndexDomain::Floating(bound) => IndexDomain::Floating(FloatingBound { source: identity(bound.source), ..bound }),
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
    matches!(domain, IndexDomain::Upper(..) | IndexDomain::Lower(..) | IndexDomain::Room(..) | IndexDomain::Sum(..) | IndexDomain::Shifted(..) | IndexDomain::Offset(..) | IndexDomain::CommonRange(..) | IndexDomain::Computed(..) | IndexDomain::KnownBits(..) | IndexDomain::Floating(..))
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
pub(super) fn range_scalar(ty: &Ty, source: &Source, span: Span) -> Result<Scalar> {
    if !ty.is_numeric() || ty.components.is_empty() || ty.components[0].0 == Scalar::Bool
        || ty.components.iter().any(|(scalar, _)| *scalar != ty.components[0].0)
    {
        return source.fail(span, "B225", "range选择同类型数值标量或向量分量");
    }
    Ok(ty.components[0].0)
}
pub(super) fn numeric_intervals(scalar: Scalar, intervals: impl IntoIterator<Item = (Bits, Bits)>, source: &Source, span: Span) -> Result<Vec<(Bits, Bits)>> {
    let intervals = intervals.into_iter().map(|(lower, upper)| {
        let (lower, upper) = if scalar.is_float() {
            let number = |bits| if scalar == Scalar::F32 { f32::from_bits(bits as u32) as f64 } else { f64::from_bits(bits) };
            if number(lower).is_nan() || number(upper).is_nan() {
                return source.fail(span, "B225", "range浮点端点不能是NaN");
            }
            let lower = if number(lower) == 0.0 { 1 << (scalar.bit_width() - 1) } else { lower };
            let upper = if number(upper) == 0.0 { 0 } else { upper };
            (floating::order(scalar, lower), floating::order(scalar, upper))
        } else { (lower ^ scalar.order_bias(), upper ^ scalar.order_bias()) };
        if lower > upper { return source.fail(span, "B225", "range闭区间下端点不能大于上端点"); }
        Ok((lower, upper))
    }).collect::<Result<Vec<_>>>()?;
    Ok(ScalarRanges::from_intervals(intervals).0)
}

#[derive(Clone)]
pub(super) struct ScalarRanges(pub(super) Vec<(Bits,Bits)>);
impl Default for ScalarRanges {
    fn default()->Self {range_sets::full()}
}
impl ScalarRanges {
    fn cast(&self,source:Scalar,target:Scalar,nonzero:bool)->Self {range_sets::cast(&self.0,source,target,nonzero)}
    fn from_intervals(values:impl IntoIterator<Item=(Bits,Bits)>)->Self {range_sets::from_intervals(values)}
    fn from_sorted_intervals(values:impl IntoIterator<Item=(Bits,Bits)>)->Self {range_sets::from_sorted_intervals(values)}
    fn interval(lower:Bits,upper:Bits)->Self {range_sets::interval(lower,upper)}
    fn hull(&self)->(Bits,Bits) {range_sets::hull(&self.0)}
    fn contains(&self,value:Bits)->bool {range_sets::contains(&self.0,value)}
    fn union(&self,other:&Self)->Self {range_sets::union(&self.0,&other.0)}
    fn restrict(&self,lower:Bits,upper:Bits)->Self {range_sets::restrict(&self.0,lower,upper)}
    fn without(&self,value:Bits)->Self {range_sets::without(&self.0,value)}
    fn map(&self,operation:impl FnMut((Bits,Bits))->Self)->Self {range_sets::map(&self.0,operation)}
    fn shift_counts(&self,scalar:Scalar)->Self {range_sets::shift_counts(&self.0,scalar)}
    fn combine(&self,other:&Self,operation:impl FnMut((Bits,Bits),(Bits,Bits))->Self)->Self {range_sets::combine(&self.0,&other.0,operation)}
}
#[derive(Clone,Copy,PartialEq,Eq)]
struct KnownBits { scalar:Scalar, mask:Bits, value:Bits }
impl KnownBits {
    fn domain(self)->IndexDomain {IndexDomain::KnownBits(self.scalar,self.mask,self.value)}
    fn cast(self,scalar:Scalar)->Self {known_bits::cast(self,scalar)}
    fn combined(self,other:Self)->Self {let scalar=if self.scalar.size()>=other.scalar.size(){self.scalar}else{other.scalar};known_bits::combined(self,other,scalar)}
    fn bitwise(operation:IntegerOperation,scalar:Scalar,a:Option<Self>,b:Option<Self>)->Option<Self> {
        let operation=match operation {IntegerOperation::And=>bitwise::Operation::And,IntegerOperation::Or=>bitwise::Operation::Or,IntegerOperation::Xor=>bitwise::Operation::Xor,_=>return None};
        known_bits::bitwise(scalar,operation,a,b)
    }
    fn common(self,other:Self)->Option<Self> {known_bits::common(self,other)}
    fn facts(scalar:Scalar,id:ValueIdentity,sets:&[&Facts])->Option<Self> {
        let (mut lower,mut upper)=(0,scalar.bit_mask());
        for facts in sets {
            if let Some(Fact::Minimum(_,_,bound))=facts.range(Fact::Minimum(id,scalar,0)..=Fact::Minimum(id,scalar,scalar.bit_mask())).next_back(){lower=lower.max(*bound);}
            if let Some(Fact::Maximum(_,_,bound))=facts.range(Fact::Maximum(id,scalar,0)..=Fact::Maximum(id,scalar,scalar.bit_mask())).next(){upper=upper.min(*bound);}
            if facts.contains(&Fact::Zero(id)){lower=lower.max(scalar.order_bias());upper=upper.min(scalar.order_bias());}
        }
        let mut bits=Self::ranges(scalar,&[(lower,upper)]);
        for facts in sets {for fact in facts.range(Fact::Index(id,IndexDomain::KnownBits(scalar,0,0))..=Fact::Index(id,IndexDomain::KnownBits(scalar,Bits::MAX,Bits::MAX))) {
            if let Fact::Index(_,IndexDomain::KnownBits(from,mask,value))=*fact {
                let incoming=Self{scalar:from,mask,value}.cast(scalar);
                bits=Some(match bits{Some(old)=>old.combined(incoming),None=>incoming});
            }
        }}
        bits
    }
    fn shifted(self,left:bool,counts:ScalarRanges)->Option<Self> {known_bits::shifted(self,left,&counts.0)}
    fn ranges(scalar:Scalar,ranges:&[(Bits,Bits)])->Option<Self> {known_bits::ranges(scalar,ranges)}
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
        // The existing Condition equality owns canonical node identity.
        self.origins==other.origins && self.conditions==other.conditions
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
                    let mut bound = target(*binding);
                    if let Some(domains) = bound.domains.first_mut() { facts.domains[component].append(domains); }
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
            let known_bits=scalar.is_integer().then(||self.known_bits(component,scalar)).flatten().map_or((0,0),|bits|(bits.mask,bits.value));
            (intervals != [(0, scalar.bit_mask())]||known_bits.0!=0).then_some(ElementRange { binding, offset, scalar, intervals, known_bits })
        }).collect()
    }
    pub(super) fn element_guarantees(&self, binding: usize, ty: &Ty, selected: impl Fn(u32, u32) -> bool) -> Vec<ElementFact> {
        let mut facts = Vec::new();
        for (component, &(scalar, offset)) in ty.components.iter().enumerate() {
            if !selected(offset, scalar.size()) { continue; }
            let intervals = self.element_range(component, scalar).0;
            let known_bits=scalar.is_integer().then(||self.known_bits(component,scalar)).flatten().map_or((0,0),|bits|(bits.mask,bits.value));
            if intervals != [(0, scalar.bit_mask())]||known_bits.0!=0 {
                facts.push(ElementFact::Range(ElementRange { binding, offset, scalar, intervals, known_bits }));
            }
            if scalar == Scalar::U32 {
                facts.extend(self.domains(component).iter().copied().filter_map(|domain| match domain {
                    IndexDomain::Data(target) => Some(ElementFact::Index { binding, offset, target }),
                    _ => None,
                }));
            }
        }
        facts
    }
    pub(super) fn constrain_range(&mut self, component: usize, intervals: &[(Bits, Bits)]) {
        self.ranges.resize_with(self.ranges.len().max(component + 1), ScalarRanges::default);
        self.ranges[component] = self.range_set(component).combine(&ScalarRanges::from_intervals(intervals.iter().copied()),
            |(a,b),(c,d)| ScalarRanges::interval(a.max(c), b.min(d)));
    }
    pub(super) fn holds_range(&self, component: usize, scalar: Scalar, intervals: &[(Bits, Bits)], required_bits:(Bits,Bits)) -> bool {
        let known_bits=scalar.is_integer().then(||self.known_bits(component,scalar)).flatten().map_or((0,0),|bits|(bits.mask,bits.value));
        self.element_range(component, scalar).0.iter().all(|&range|scalar.covers_range(range,known_bits,intervals,required_bits))
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
            self.ranges[component] = fresh.range_set(source).into_owned();
            self.domains[component] = fresh.domains(source).into_owned();
            self.conditions[component] = fresh.conditions.get(source).cloned().flatten();
        }
    }
    pub(super) fn constrain_common_ranges(&mut self, other: &Self, width: usize) {
        self.nonzero.resize(width, false);
        self.domains.resize_with(width, Domains::new);
        self.ranges.resize_with(width, ScalarRanges::default);
        self.conditions.resize_with(width,||None);
        for component in 0..width {
            self.nonzero[component] |= other.nonzero.get(component) == Some(&true);
            for domain in other.domains(component).iter().copied() {self.mark_domain(component,domain);}
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
        self.domains[0].extend(other.domains(0).iter().copied());
    }
    pub(super) fn borrow_state(&self)->Self {Self{views:self.views.clone(),..Self::default()}}
    pub(super) fn numeric_facts(&self)->Self {
        Self{origins:self.origins.clone(),conditions:self.conditions.clone(),ranges:self.ranges.clone(),nonzero:self.nonzero.clone(),domains:self.domains.clone(),..Self::default()}
    }
    pub(super) fn typed_view_borrow(&self,ty:&Ty)->view::Borrow {view_contracts::components(ty).into_iter().filter_map(|component|self.views.get(component)).fold(view::Borrow::default(),|mut all,value|{all.union(value);all})}
    pub(super) fn select_view_borrows(&mut self,values:&[(Condition,Self)]){
        let width=values.iter().map(|(_,proof)|proof.views.len()).max().unwrap_or(0);
        self.views=(0..width).map(|component|{let mut value=view::Borrow::default();for(when,proof)in values{if let Some(borrow)=proof.views.get(component).filter(|borrow|!borrow.empty()){value.union(&borrow.restricted(&when.assume(&Clause::default(),true)));}}value}).collect();
    }
    pub(super) fn join_view_borrows(&mut self,a:&Self,b:&Self,choice:&Condition){
        self.views=(0..a.views.len().max(b.views.len())).map(|component|{
            let empty=view::Borrow::default();
            let a=a.views.get(component).unwrap_or(&empty);let b=b.views.get(component).unwrap_or(&empty);
            a.clone().selected(choice,b)
        }).collect();
    }
    pub(super) fn retain_view_borrows(&mut self,ty:&Ty) {let selected=view_contracts::components(ty);for(component,borrow)in self.views.iter_mut().enumerate(){if !selected.contains(&component){*borrow=view::Borrow::default();}}}
    pub(super) fn view_borrow(&self,component:usize)->view::Borrow {self.views.get(component).cloned().unwrap_or_default()}
    pub(super) fn borrowed_view(&self)->view::Borrow {self.views.iter().fold(view::Borrow::default(),|mut all,value|{all.union(value);all})}
    pub(super) fn borrowed_views(&self)->BTreeSet<view::Source> {self.borrowed_view().sources.entries().filter_map(|(source,when)|when.possible().then_some(source.clone())).collect()}
    pub(super) fn with_external_borrows(mut self,ty:&Ty)->Self {
        for (offset,field) in ty.borrow_fields() {
            if field.is_address() {
                let component=ty.components.iter().position(|(_,at)|*at==offset).unwrap();
                self.mark_external_borrow(component);
            }
        }
        self
    }
    pub(super) fn mark_external_borrow(&mut self,component:usize) {
        self.views.resize_with(self.views.len().max(component+1),view::Borrow::default);
        self.views[component].sources.union_when(view::Source::External,&Clause::default());
    }
    pub(super) fn set_view_borrow(&mut self,component:usize,borrow:view::Borrow) {
        self.views.resize_with(self.views.len().max(component+1),view::Borrow::default);self.views[component]=borrow;
    }
    pub(super) fn view_parameters(mut self, ty: &Ty, indices:&[Option<Operand>], identity: impl Fn(usize) -> ValueIdentity) -> Self {
        for at in view_contracts::components(ty) {
            self.set_view_borrow(at,view::Borrow::source(view::Source::Parameter(identity(at),indices.to_vec())));
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
                if matches!(identity,ValueIdentity::Register(source) if *source>=entry && source!=reg)
                    || matches!(identity,ValueIdentity::TaskCompletion(..)) {

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
        let mut values=Facts::new();
        for (component,reg) in registers.iter().enumerate(){proof_facts::append_component(&mut values,&outputs,component,types[*reg]);}
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
        let mut proof = Self {
            origins: self.origins.clone(),
            conditions: self.conditions.clone(),
            ranges: self.ranges.clone(),
            nonzero: self.nonzero.clone(),
            domains: self.domains.clone(),
            views: self.views.clone(),
            addresses: self.addresses.iter().map(|value| value.as_ref().map(|value| value.output_bound(bindings))).collect(),
        };
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
        Clause::any_scalar_identity(self.conditions.iter().flatten().flat_map(|condition|condition.cases.iter()),&|id|{identity(*id);false});
        for address in self.addresses.iter().flatten(){if let address::Identity::Value(values)=address{for id in values{identity(*id);}}}
        ids.into_inner()
    }
    pub(super) fn any_scalar_identity(&self, test:&impl Fn(&ValueIdentity)->bool)->bool {
        if self.origins.iter().flatten().any(test) { return true; }
        for domain in self.domains.iter().flatten() {
            let found=std::cell::Cell::new(false);
            map_domain(*domain,&|id|{found.set(found.get()||test(&id));id});
            if found.get() { return true; }
        }
        if Clause::any_scalar_identity(self.conditions.iter().flatten().flat_map(|condition|condition.cases.iter()),test) {
            return true;
        }
        self.addresses.iter().flatten().any(|address|match address {
            address::Identity::Value(values)=>values.iter().any(test),
            _=>false,
        })
    }
    fn bound_input<'a>(
        bindings: &'a BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
        id: &ValueIdentity,
    ) -> Option<&'a Proof> {
        bindings.get(id).map(std::borrow::Borrow::borrow)
    }
    fn value_bindings<'a>(
        identities: impl IntoIterator<Item = ValueIdentity>,
        mut values: impl FnMut(ValueIdentity) -> Option<&'a Proof>,
        unchanged: impl Fn(ValueIdentity) -> Proof,
    ) -> BTreeMap<ValueIdentity, Cow<'a, Proof>> {
        identities.into_iter().map(|id| {
            let proof = match values(id) { Some(proof) => Cow::Borrowed(proof), None => Cow::Owned(unchanged(id)) };
            (id, proof)
        }).collect()
    }

    pub(super) fn substitute_values(&self, values: &BTreeMap<ValueIdentity, Proof>) -> Self {
        let bindings = Self::value_bindings(self.scalar_identities(), |id| values.get(&id), |id| {
            Proof::default().with_identity(id)
        });
        let mut proof = self.project_call(&bindings, None);
        proof.views = self.views.iter().map(|borrow| borrow.substitute_values(values)).collect();
        proof.addresses = self.call_addresses(&bindings, |component, incoming| {
            self.addresses.get(component).and_then(Option::as_ref)
                .map(|address| address.substitute_values(values)).or_else(|| incoming.cloned())
        });
        proof
    }
    fn call_identity(
        id: ValueIdentity,
        bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
    ) -> Option<ValueIdentity> {
        Proof::bound_input(bindings, &id)?.known_identity(0)
    }
    fn call_domain(
        value: IndexDomain,
        bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
    ) -> Option<IndexDomain> {
        Some(match value {
            IndexDomain::Fixed(n) => IndexDomain::Fixed(n),
            IndexDomain::KnownBits(..)=>value,
            IndexDomain::ApplicationData(id) => {
                IndexDomain::ApplicationData(Self::call_identity(id, bindings)?)
            }
            IndexDomain::ApplicationLength(id) => {
                IndexDomain::ApplicationLength(Self::call_identity(id, bindings)?)
            }
            IndexDomain::View(ids) => IndexDomain::View([
                Self::call_identity(ids[0], bindings)?,
                Self::call_identity(ids[1], bindings)?,
                Self::call_identity(ids[2], bindings)?,
            ]),
            IndexDomain::ViewLength(ids) => IndexDomain::ViewLength([
                Self::call_identity(ids[0], bindings)?,
                Self::call_identity(ids[1], bindings)?,
                Self::call_identity(ids[2], bindings)?,
            ]),
            IndexDomain::Upper(limit, distance) => IndexDomain::Upper(Self::call_identity(limit, bindings)?, distance),
            IndexDomain::Lower(base, distance) => IndexDomain::Lower(Self::call_identity(base, bindings)?, distance),
            IndexDomain::Room(base) => IndexDomain::Room(Self::call_identity(base, bindings)?),
            IndexDomain::Sum(left, right) => IndexDomain::Sum(Self::call_identity(left, bindings)?, Self::call_identity(right, bindings)?),
            IndexDomain::Offset(base, amount) => IndexDomain::Offset(Self::call_identity(base, bindings)?, amount),
            IndexDomain::Shifted(base, limit, distance) => IndexDomain::Shifted(Self::call_identity(base, bindings)?, Self::call_identity(limit, bindings)?, distance),
            IndexDomain::Computed(expression) => IndexDomain::Computed(expression.at_call(bindings)?),
            IndexDomain::Floating(bound) => IndexDomain::Floating(bound.at_call(bindings)?),
            _ => return None,
        })
    }
    fn call_common_bits(input:&Proof,from:Scalar,low:Scalar,extended:Scalar,to:Scalar)->Option<KnownBits> {
        if ![from,low,extended,to].into_iter().all(Scalar::is_integer){return None;}
        let bits=input.known_bits(0,from)?.cast(low).cast(extended).cast(to);
        (bits.mask!=0).then_some(bits)
    }
    fn call_fact(value: Fact, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> Option<Clause> {
        let projected = |facts: Facts| (!facts.is_empty()).then(|| Clause::facts(facts));
        let translated = match value {
            Fact::Boolean(id, _) | Fact::Zero(id) | Fact::NonZero(id) => {
                let truth=match value {Fact::Boolean(_,truth)=>truth,Fact::NonZero(_)=>true,_=>false};
                let Some(input) = Proof::bound_input(bindings, &id) else {
                    return None;
                };
                if !truth && input.nonzero.first()==Some(&true) {
                    return Some(Clause::impossible());
                }
                let mut clause = input.component_condition(0).and_then(|condition| {
                    let case = &condition.cases[usize::from(truth)];
                    (!matches!(&**case.0, ClauseNode::Facts(facts) if facts.is_empty()))
                        .then(|| case.clone())
                });
                if let Some(id) = input.known_identity(0) {
                    let fact = Clause::facts([value.at(id)].into());
                    clause = Some(match clause { Some(clause) => Clause::both(&clause,&fact), None => fact });
                }
                return clause;
            }
            Fact::Below(lower, upper, strict, scalar) => {
                let Some((lower_id, upper_id)) =
                    Self::call_identity(lower, bindings).zip(Self::call_identity(upper, bindings))
                else {
                    return None;
                };
                let (lower, upper) = (Proof::bound_input(bindings, &lower).unwrap(), Proof::bound_input(bindings, &upper).unwrap());
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
                    return Some(Clause::impossible());
                }
                let mut facts = Facts::new();
                constrain_proofs(
                    &mut facts,
                    (lower_id, lower),
                    (upper_id, upper),
                    strict,
                    scalar,
                );
                return projected(facts);
            }
            Fact::Equal(a, b) => {
                let Some((left,right))=Self::call_identity(a,bindings).zip(Self::call_identity(b,bindings)) else{return None;};
                let mut facts=Facts::from([Fact::equal(left,right)]);
                for(source,target)in [(a,right),(b,left)] {
                    facts.extend(Proof::bound_input(bindings, &source).unwrap().domains(0).iter().copied().map(|domain|Fact::Index(target,domain)));
                }
                let mut clause=Clause::facts(facts);
                for source in [a,b] {
                    if let Some(condition)=Proof::bound_input(bindings, &source).unwrap().component_condition(0){
                        clause=Clause::both(&clause,&Clause::either(&condition.cases[0],&condition.cases[1]));
                    }
                }
                return Some(clause);
            },
            Fact::NotEqual(a, b) => {
                if let Some((left, right)) = Proof::bound_input(bindings, &a).zip(Proof::bound_input(bindings, &b)) {
                    let same = left.known_identity(0).zip(right.known_identity(0))
                        .is_some_and(|(a, b)| a == b);
                    let conditions = left.component_condition(0).and_then(Condition::boolean_constant)
                        .zip(right.component_condition(0).and_then(Condition::boolean_constant));
                    let (al, ah) = left.range(0);
                    let (bl, bh) = right.range(0);
                    if same || conditions == Some((false,false))
                        || (al == ah && bl == bh && al == bl) {
                        return Some(Clause::impossible());
                    }
                    if conditions.is_some_and(|(a,b)| a != b) || ah < bl || bh < al {
                        return None;
                    }
                }
                Self::call_identity(a, bindings).zip(Self::call_identity(b, bindings))
                    .map(|(a,b)| Fact::NotEqual(a.min(b), a.max(b)))
            },
            Fact::Index(id, IndexDomain::CommonRange(source,from,low,extended,to)) => {
                let mut facts=Facts::new();
                if let Some(target)=Self::call_identity(id,bindings) && let Some(input)=Proof::bound_input(bindings, &source){
                    let range=input.range_set(0).cast(from,low,input.nonzero.first()==Some(&true)).cast(low,extended,false).cast(extended,to,false);
                    if to==Scalar::Bool && let Some(truth)=input.component_condition(0).and_then(Condition::boolean_constant){facts.insert(Fact::Boolean(target,truth));}
                    let nonzero=if to.is_integer(){!range.contains(to.order_bias())||Self::call_common_bits(input,from,low,extended,to).is_some_and(|bits|bits.value!=0)}else{input.nonzero.first()==Some(&true)};
                    if nonzero{facts.insert(Fact::NonZero(target));}
                    if to.is_integer()||to.is_float(){let(minimum,maximum)=range.hull();if minimum!=0{facts.insert(Fact::Minimum(target,to,minimum));}if maximum!=to.bit_mask(){facts.insert(Fact::Maximum(target,to,maximum));}}
                    facts.extend(input.domains(0).iter().copied().filter_map(|domain|cast_domain(domain,from,low).and_then(|domain|cast_domain(domain,low,extended)).and_then(|domain|cast_domain(domain,extended,to))).map(|domain|Fact::Index(target,domain)));
                    facts.extend(input.origins.first().into_iter().flatten().filter(|source|!matches!(source,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)|ValueIdentity::ElementIndex(_))).map(|source|Fact::Index(target,IndexDomain::CommonRange(*source,from,low,extended,to))));
                }
                return projected(facts);
            },
            Fact::Index(id, d) => Self::call_identity(id, bindings)
                .zip(Self::call_domain(d, bindings))
                .map(|(id, d)| Fact::Index(id, d)),

            Fact::Minimum(id, scalar, n)=>{
                if let Some(value)=Proof::bound_input(bindings, &id){let range=value.range_set(0).restrict(0,scalar.bit_mask()).hull();if range.0>=n{return None;}if range.1<n{return Some(Clause::impossible());}}
                Self::call_identity(id,bindings).map(|id|Fact::Minimum(id, scalar, n))
            },
            Fact::Maximum(id, scalar, n)=>{
                if let Some(value)=Proof::bound_input(bindings, &id){let range=value.range_set(0).restrict(0,scalar.bit_mask()).hull();if range.1<=n{return None;}if range.0>n{return Some(Clause::impossible());}}
                Self::call_identity(id,bindings).map(|id|Fact::Maximum(id, scalar, n))
            },
            Fact::Outside(id,d)=>Self::call_identity(id,bindings).zip(Self::call_domain(d,bindings)).map(|(id,d)|Fact::Outside(id,d)),
            Fact::Predicate(predicate,truth)=>{
                let Some(mapped)=predicate.at_call(bindings)else{return None};
                let mut facts=Facts::from([Fact::Predicate(mapped,truth)]);
                for operand in [predicate.a,predicate.b]{
                    if let Operand::Value(source)=operand && let Some(target)=Proof::bound_input(bindings, &source).unwrap().known_identity(0){facts.extend(Proof::bound_input(bindings, &source).unwrap().domains(0).iter().copied().filter(|domain|matches!(domain,IndexDomain::CommonRange(..))).map(|domain|Fact::Index(target,domain)));}
                }
                return Some(Clause::facts(facts));
            },
        };
        translated.map(|fact| Clause::facts([fact].into()))
    }
    pub(super) fn at_call(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        let mut proof = self.numeric_at_call(bindings);
        proof.views = self.views.iter().map(|borrow| borrow.at_call(bindings)).collect();
        proof.addresses = self.call_addresses(bindings, |component, incoming| {
            incoming.cloned().or_else(|| self.addresses.get(component).and_then(Option::as_ref)
                .map(|address| address.at_call(bindings)))
        });
        proof
    }

    pub(super) fn numeric_at_call(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        self.project_call(bindings, None)
    }

    pub(super) fn project_numeric_at_call(
        &self, components: &[usize], width: usize, bindings: &BTreeMap<ValueIdentity, Proof>,
    ) -> Self {
        let selected = (!components.iter().copied().eq(0..width)).then_some(components);
        self.project_call(bindings, selected)
    }

    fn call_addresses(
        &self,
        bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
        mut address: impl FnMut(usize, Option<&address::Identity>) -> Option<address::Identity>,
    ) -> Vec<Option<address::Identity>> {
        let mut addresses = Vec::with_capacity(self.addresses.len());
        for component in 0..self.addresses.len().max(self.origins.len()) {
            let incoming = self.origins.get(component).into_iter().flatten()
                .filter_map(|id| Proof::bound_input(bindings, id)
                    .and_then(|proof| proof.addresses.first()).and_then(Option::as_ref)).last();
            let value = address(component, incoming);
            if component < self.addresses.len() || value.is_some() {
                addresses.resize_with(component + 1, || None);
                addresses[component] = value;
            }
        }
        addresses
    }

    fn project_call(
        &self,
        bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
        components: Option<&[usize]>,
    ) -> Self {
        let count = |width| components.map_or(width, |selected| selected.len());
        let source = |component: usize| components.map_or(component, |selected| selected[component]);
        let mut translated = Self {
            views: Vec::new(), addresses: Vec::new(),
            origins: Vec::with_capacity(count(self.origins.len())),
            domains: Vec::with_capacity(count(self.origins.len().max(self.domains.len()))),
            conditions: Vec::new(),
            ranges: components.map_or_else(|| self.ranges.clone(), |selected| selected.iter().map(|&i| self.range_set(i).into_owned()).collect()),
            nonzero: components.map_or_else(|| self.nonzero.clone(), |selected| selected.iter().map(|&i| self.nonzero.get(i) == Some(&true)).collect()),
        };
        for i in 0..count(self.origins.len()) {
            let origins = self.origins.get(source(i)).into_iter().flatten();
            translated.origins.push(Vec::new());
            translated.ranges.resize_with(translated.ranges.len().max(i + 1), ScalarRanges::default);
            translated.nonzero.resize(translated.nonzero.len().max(i + 1), false);
            translated.domains.push(self.domains.get(source(i)).into_iter().flatten().copied()
                .filter_map(|domain| Self::call_domain(domain, bindings)).collect());
            for value in origins.filter_map(|id| Proof::bound_input(bindings, id)) {
                translated.origins[i].extend(value.origins.first().into_iter().flatten().copied());
                let incoming = value.ranges.first().map_or(&[(0, Bits::MAX)][..], |range| range.0.as_slice());
                translated.ranges[i] = ScalarRanges::from_sorted_intervals(
                    translated.ranges[i].0.iter().flat_map(|(a, b)| {
                        incoming.iter().map(move |(c, d)| ((*a).max(*c), (*b).min(*d)))
                    }).filter(|(lower, upper)| lower <= upper),
                );
                translated.nonzero[i] |= value.nonzero.first() == Some(&true);
                translated.domains[i].extend(value.domains.first().into_iter().flatten().copied());
            }
        }
        // A proven domain can exist without a captured origin.
        for i in count(self.origins.len())..count(self.domains.len()) {
            let domains = &self.domains[source(i)];
            translated.domains.push(domains.iter().copied()
                .filter_map(|domain| Self::call_domain(domain, bindings)).collect());
        }
        for component in 0..count(self.domains.len()) {
            let domains = self.domains.get(source(component)).into_iter().flatten();
            for domain in domains {
                if let IndexDomain::CommonRange(source,from,low,extended,to) = domain {
                    if let Some(incoming) = Proof::bound_input(bindings, source) {
                        let range=incoming.range_set(0).cast(*from,*low,incoming.nonzero.first()==Some(&true)).cast(*low,*extended,false).cast(*extended,*to,false);
                        if let Some(bits)=Self::call_common_bits(incoming,*from,*low,*extended,*to){translated.mark_domain(component,bits.domain());}
                        translated.ranges.resize_with(translated.ranges.len().max(component + 1), ScalarRanges::default);
                        translated.ranges[component] = translated.ranges[component].combine(&range,
                            |(al, ah), (bl, bh)| ScalarRanges::interval(al.max(bl), ah.min(bh)));
                        translated.nonzero.resize(translated.nonzero.len().max(component + 1), false);
                        translated.nonzero[component] |= if to.is_integer(){!range.contains(to.order_bias())}else{incoming.nonzero.first()==Some(&true)};
                        translated.domains.resize_with(translated.domains.len().max(component + 1), Domains::new);
                        translated.domains[component].extend(incoming.domains.first().into_iter().flatten().copied().filter_map(|domain|cast_domain(domain,*from,*low).and_then(|domain|cast_domain(domain,*low,*extended)).and_then(|domain|cast_domain(domain,*extended,*to))));
                        translated.domains[component].extend(incoming.origins.first().into_iter().flatten()
                            .filter(|id|!matches!(id,ValueIdentity::ElementComponent(_)|ValueIdentity::ElementParent(..)|ValueIdentity::ElementIndex(_)))
                            .map(|id|IndexDomain::CommonRange(*id,*from,*low,*extended,*to)));
                    }
                }
                if let IndexDomain::Floating(bound) = domain
                    && let Some(range) = bound.range_at_call(bindings)
                {
                    translated.ranges.resize_with(translated.ranges.len().max(component + 1), ScalarRanges::default);
                    translated.ranges[component] = translated.ranges[component].combine(&range,
                        |(al, ah), (bl, bh)| ScalarRanges::interval(al.max(bl), ah.min(bh)));
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
        let roots: Vec<_> = (0..count(self.conditions.len())).filter_map(|i| self.conditions.get(source(i)).and_then(Option::as_ref))
            .flat_map(|condition| condition.cases.iter()).collect();
        let mut projected = if roots.is_empty() { Vec::new() } else {
            Clause::at_calls(&roots, &Clause::ordered(roots.iter().copied()), bindings, |_| false, Clause::required)
        }.into_iter();
        translated.conditions = (0..count(self.conditions.len())).map(|i| self.conditions.get(source(i)).and_then(Option::as_ref).map(|_| {
            Box::new(Condition { cases: [projected.next().unwrap(), projected.next().unwrap()] })
        })).collect();
        // 输出仍是入口快照时，继续交接调用者已经持有的分量条件。
        // 实参图属于调用者命名空间，不能再次按被调函数的身份替换。
        for i in 0..count(self.origins.len()) {
            let origins = self.origins.get(source(i)).into_iter().flatten();
            for incoming in origins.filter_map(|id| Proof::bound_input(bindings, id)) {
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
        for component in 0..translated.conditions.len() {
            if translated.nonzero.get(component) != Some(&true)
                && translated.component_condition(component)
                    .is_some_and(|condition| !condition.cases[0].possible())
            {
                translated.mark_nonzero(component);
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
            if let Some(id)=initial.known_identity(0){self.mark_domain(component,IndexDomain::Lower(id,0));}
            for domain in initial.domains(0).iter().copied() {
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
        let mut bits=KnownBits::ranges(scalar,self.ranges.get(component).map_or(&[(0,Bits::MAX)][..],|ranges|ranges.0.as_slice()));
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
        let range=arithmetic_ranges("&",scalar,&self.range_set(component).restrict(0,scalar.bit_mask()),&ScalarRanges::interval(ordered,ordered));
        let changed=ScalarRanges::interval(0,mask).cast(Scalar::U64,scalar,false);
        let mut proof=Self{ranges:vec![arithmetic_ranges("|",scalar,&range,&changed)],..Self::default()};
        if let Some(mut bits)=self.known_bits(component,scalar){bits.mask&=kept;bits.value&=bits.mask;if bits.mask!=0{proof.mark_domain(0,bits.domain());}}
        proof
    }
    pub(super) fn forget_value(&mut self, components: &[usize], width: usize) {
        if components.iter().copied().eq(0..width) {
            let mut views = std::mem::take(&mut self.views);
            views.resize_with(width, view::Borrow::default);
            *self = Self { views, ..Self::default() };
        } else {
            self.views.resize_with(width, view::Borrow::default);
            self.origins.resize_with(width, Vec::new);
            self.conditions.resize_with(width, || None);
            self.nonzero.resize(width, false);
            self.ranges.resize_with(width, ScalarRanges::default);
            self.domains.resize_with(width, Domains::new);
            if !self.addresses.is_empty() { self.addresses.resize_with(width, || None); }
            for &component in components {
                self.origins[component].clear();
                self.conditions[component] = None;
                self.nonzero[component] = false;
                self.ranges[component] = ScalarRanges::default();
                self.domains[component].clear();
                if !self.addresses.is_empty() { self.addresses[component] = None; }
            }
        }
    }
    pub(super) fn assign_view_borrows(&mut self,positions:&[Position],owner:&[Position],fresh:&Self){
        self.views.resize_with(owner.len(),view::Borrow::default);
        for(component,position)in positions.iter().enumerate(){
            let empty=view::Borrow::default();let sources=fresh.views.get(component).unwrap_or(&empty);
            for(index,leaf)in owner.iter().enumerate(){
                let selected=owned_presence::Presence::selected(position,leaf);
                if selected!=owned_presence::Presence::None{self.views[index]=sources.clone().selected(&selected.condition(),&self.views[index]);}
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
            let mut addresses = fresh.addresses.into_iter();
            let mut views = fresh.views.into_iter();
            let mut origins = fresh.origins.into_iter();
            let mut conditions = fresh.conditions.into_iter();
            let mut nonzero = fresh.nonzero.into_iter();
            let mut ranges = fresh.ranges.into_iter();
            let mut domains = fresh.domains.into_iter();
            for &i in components {
                if !self.addresses.is_empty(){self.addresses[i]=addresses.next().flatten();}
                self.views[i] = views.next().unwrap_or_default();
                self.origins[i] = origins.next().unwrap_or_default();
                self.conditions[i] = conditions.next().flatten();
                self.nonzero[i] = nonzero.next().unwrap_or(false);
                self.ranges[i] = ranges.next().unwrap_or_default();
                self.domains[i] = domains.next().unwrap_or_default();
            }
        }
    }
    pub(super) fn join(&mut self,other:&Self,width:usize,branch:Option<&Value>){let choice=branch.map(Condition::of);self.join_condition(other,width,choice.as_ref());}
    pub(super) fn join_condition(&mut self, other: &Self, width: usize, branch: Option<&Condition>) {
        let conditions = (0..width)
            .map(|i| {
                if self.component_condition(i).is_none() && other.component_condition(i).is_none() {
                    return None;
                }
                let a = self.condition_for_join(i);
                let b = other.condition_for_join(i);
                Some(Box::new(if let Some(c) = branch {
                    Condition::branch(c.clone(), a, b)
                } else {
                    Condition::common(a, b)
                }))
            })
            .collect();
        self.join_fields(other, width, branch);
        self.conditions = conditions;
    }

    pub(super) fn condition_for_join(&self, component: usize) -> Condition {
        match self.component_condition(component) {
            Some(condition) => condition.clone(),
            None if self.nonzero.get(component) == Some(&true) => Condition::constant(true),
            None => Condition::default(),
        }
    }

    pub(super) fn carried_loop_condition(&self) -> Option<view::LoopInput> {
        if self.component_condition(0).is_some() {
            return None;
        }
        match self.origins.first()?.as_slice() {
            [ValueIdentity::LoopValue(level, index)] => Some(view::LoopInput { level: *level, index: *index }),
            _ => None,
        }
    }

    pub(super) fn set_loop_condition(&mut self, condition: Option<Condition>) {
        self.conditions = vec![condition.map(Box::new)];
    }

    pub(super) fn join_fields(&mut self, other: &Self, width: usize, branch: Option<&Condition>) {
        if !self.addresses.is_empty()||!other.addresses.is_empty(){self.addresses=(0..width).map(|i|address::Identity::selected(branch,self.addresses.get(i).and_then(Option::as_ref),other.addresses.get(i).and_then(Option::as_ref))).collect();}
        self.views.resize_with(width,view::Borrow::default);
        for(i,value)in self.views.iter_mut().enumerate(){
            let empty=view::Borrow::default();let other=other.views.get(i).unwrap_or(&empty);
            if let Some(choice)=branch{*value=std::mem::take(value).selected(choice,other);}else{value.union(other);}
        }
        self.domains = (0..width).map(|i| {
            let a=self.domains(i);let b=other.domains(i);
            let mut common=a.intersection(&b).copied().collect::<Domains>();
            let mut previous_limit=None;
            for domain in a.union(&b) {
                let IndexDomain::Upper(limit,_) = domain else {continue;};
                if previous_limit==Some(*limit){continue;}previous_limit=Some(*limit);
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
        self.origins.resize_with(width,Vec::new);
        for(i,origins)in self.origins.iter_mut().enumerate(){
            let other=other.origins.get(i).map(Vec::as_slice).unwrap_or_default();
            origins.retain(|identity|other.contains(identity));
        }
        self.nonzero = (0..width)
            .map(|i| self.nonzero.get(i) == Some(&true) && other.nonzero.get(i) == Some(&true) || self.domains(i).iter().any(|domain|matches!(domain,IndexDomain::KnownBits(_,_,value)if *value!=0)))
            .collect();
        self.ranges = (0..width)
            .map(|i| self.range_set(i).union(&other.range_set(i)))
            .collect();
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
            ranges: components.iter().map(|&i| self.range_set(i).into_owned()).collect(),
            domains: components.iter().map(|&i| self.domains(i).into_owned()).collect(),
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
                &self.range_set(0).cast(scalar,scalar,self.nonzero.get(0)==Some(&true)),
                &integer_result_range((low,high),0,scalar.bit_width()))],
            ..Self::default()
        }
    }
    pub(super) fn unsigned_reflected(&self, amount: u32) -> Self {
        let amount = Bits::from(amount);
        Self {
            ranges: vec![arithmetic_ranges("-", Scalar::U32,
                &ScalarRanges::interval(amount, amount), &self.range_set(0))],
            ..Self::default()
        }
    }
    pub(super) fn with_domains(mut self, mut domains: Domains) -> Self {
        // 存储版本更换资源域，数值仍是实际同一份长度快照。
        domains.extend(self.domains.first_mut().map(std::mem::take).unwrap_or_default().into_iter().filter(numeric_domain));
        self.domains = vec![domains];
        self
    }
    pub(super) fn below_length(index: (&[ValueIdentity], &Domains), length: (&[ValueIdentity], &Domains), strict: bool) -> bool {
        let (identities, bounds) = index;
        let (origins, domains) = length;
        if bounds.iter().any(|bound|match *bound{
            IndexDomain::Data(binding)=>origins.contains(&ValueIdentity::DataLength(binding)),
            IndexDomain::Tile=>origins.contains(&ValueIdentity::TileLength),
            IndexDomain::View(value)=>origins.contains(&value[2]),
            _=>false,
        }){return true;}
        if domains.iter().any(|domain| {
            let IndexDomain::Offset(base, amount) = *domain else { return false; };
            amount >= u32::from(strict) && identities.contains(&base) || bounds.iter().any(|bound| match *bound {
                IndexDomain::Offset(source, offset) => source == base && if strict { offset < amount } else { offset <= amount },
                IndexDomain::Upper(limit, distance) => limit == base && (!strict || amount != 0 || distance != 0),
                _ => false,
            })
        }) { return true; }
        bounds.iter().any(|bound| match *bound {
            IndexDomain::Upper(limit, distance) => distance >= u32::from(strict) && origins.contains(&limit),
            IndexDomain::Shifted(base, limit, distance) if distance >= u32::from(strict) => domains.iter().any(|sum| {
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
        let mut origins = self.origins.first_mut().map(std::mem::take).unwrap_or_default();
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
    pub(super) fn domains(&self, component: usize) -> Cow<'_, Domains> {
        self.domains.get(component).map_or_else(|| Cow::Owned(Domains::new()), Cow::Borrowed)
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
    fn range_set(&self, component: usize) -> Cow<'_, ScalarRanges> {
        self.ranges.get(component).map_or_else(|| Cow::Owned(ScalarRanges::default()), Cow::Borrowed)
    }
    pub(super) fn bitcast(value: &Value, target: Scalar) -> Self {
        Self {
            ranges: (0..value.regs.len()).map(|component| floating::bitcast(value, component, target)).collect(),
            ..Self::default()
        }
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
            let domains=proof.domains.get_mut(i).map(std::mem::take).unwrap_or_default().into_iter().filter_map(|domain|cast_domain(domain,source,target))
                .chain(proof.origins[i].iter().copied().filter(|_|integers&&!same_value).filter_map(|id|cast_domain(IndexDomain::CommonRange(id,source,source,source,source),source,target))).collect::<Domains>();
            for domain in domains{proof.mark_domain(i,domain);}
            if target == Scalar::F32 {
                for domain in floating::cast_domains(value, i) { proof.mark_domain(i, domain); }
            }
            if !same_value{proof.origins[i].clear();}
            if !integers || target.size() < source.size() { proof.conditions[i] = None; }
        }
        proof
    }
    pub(super) fn capture(value: &Value) -> Self {
        Self {
            domains: value.proof.domains.clone(),
            views: value.proof.views.clone(),
            addresses: value.proof.addresses.clone(),
            ranges: (0..value.regs.len()).map(|i| value.component_ranges(i)).collect(),
            nonzero: (0..value.regs.len()).map(|i| value.component_nonzero(i)).collect(),
            origins: (0..value.regs.len()).map(|i| value.component_origins(i)).collect(),
            conditions: (0..value.regs.len()).map(|i| Self::value_condition(value, i)).collect(),
        }
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
    fn facts(&self,ty:&Ty)->Facts {proof_facts::collect(self,ty)}
    pub(super) fn returning(
        value: &Value,
        environment: &BTreeMap<String, Binding>,
        path: &Clause,
    ) -> Self {
        let mut proof = Self {
            origins: value.proof.origins.clone(),
            ranges: value.proof.ranges.clone(),
            nonzero: value.proof.nonzero.clone(),
            domains: value.proof.domains.clone(),
            views: value.proof.views.clone(),
            addresses: value.proof.addresses.clone(),
            conditions: (0..value.regs.len()).map(|i| Self::value_condition(value, i)).collect(),
        };
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
            .extend((0..value.regs.len()).map(|i| value.proof.domains(i).into_owned()));
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
fn environment_facts(environment:&BTreeMap<String,Binding>)->Facts {
    let mut facts=Facts::new();
    for binding in environment.values(){
        match binding {
            Binding::ApplicationData(data)=>proof_facts::append_proof(&mut facts,&data.length.proof,&data.length.ty),
            Binding::Value(value)=>proof_facts::append_value(&mut facts,value,false),
            Binding::Owned(owner)|Binding::ViewElement{element:owner,..}=>proof_facts::append_proof(&mut facts,&owner.proof,owner.place.ty()),
            Binding::OwnedValue(owner)=>proof_facts::append_proof(&mut facts,&owner.value.proof,&owner.value.ty),
            Binding::Data(view)=>proof_facts::append_identity(&mut facts,&view.length,Scalar::U32,ValueIdentity::DataLength(view.binding)),
            Binding::Tile(view)=>proof_facts::append_identity(&mut facts,&view.length,Scalar::U32,ValueIdentity::TileLength),
            _=>{},
        }
    }
    facts
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
where I: IntoIterator<Item = &'a Fact>, I::IntoIter: Clone { equality::roots(facts) }
fn extend_computed_roots<'a>(roots: &mut BTreeMap<ValueIdentity, ValueIdentity>, facts: impl IntoIterator<Item = &'a Fact>) {
    equality::computed(roots, facts)
}
fn extend_equality_roots<'a>(roots: &mut BTreeMap<ValueIdentity, ValueIdentity>, facts: impl IntoIterator<Item = &'a Fact>) -> bool {
    equality::extend(roots, facts)
}
fn additional_equality_roots<'a>(environment: &Facts, environment_roots: &'a BTreeMap<ValueIdentity, ValueIdentity>, additions: &Facts) -> Cow<'a, BTreeMap<ValueIdentity, ValueIdentity>> {
    equality::additional(environment, environment_roots, additions)
}
type EqualityMembers<'a> = Vec<(&'a ValueIdentity, &'a ValueIdentity)>;

fn equality_members(roots: &BTreeMap<ValueIdentity, ValueIdentity>) -> EqualityMembers<'_> {
    let mut members: Vec<_> = roots.iter().map(|(id, group)| (group, id)).collect();
    members.sort_unstable_by_key(|(group, _)| *group);
    members
}

fn equivalent_facts<'a>(environment: &Facts, facts: QueryFacts<'a>, roots: &BTreeMap<ValueIdentity, ValueIdentity>, members: &[(&ValueIdentity, &ValueIdentity)], inherited_roots: Option<&BTreeMap<ValueIdentity, ValueIdentity>>) -> QueryFacts<'a> {
    equality::equivalent(environment, facts, roots, members, inherited_roots)
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
    pub(super) fn component_ranges(&self, component: usize) -> ScalarRanges {
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
        let first_fallback;
        let first = match self.proof.origins.get(index).filter(|origins| !origins.is_empty()) {
            Some(origins) => origins.as_slice(),
            None => { first_fallback = [ValueIdentity::Register(self.regs[index])]; &first_fallback[..] },
        };
        let second_fallback;
        let second = match other.proof.origins.get(other_index).filter(|origins| !origins.is_empty()) {
            Some(origins) => origins.as_slice(),
            None => { second_fallback = [ValueIdentity::Register(other.regs[other_index])]; &second_fallback[..] },
        };
        first.iter().any(|origin| second.contains(origin))
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
            domains: indices.iter().map(|i| self.proof.domains(*i).into_owned()).collect(),
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
// One stored-output pass has one fixed public binding and owns its guard results.
pub(super) struct PublicGuards<'a> {
    pub(super) bindings: &'a BTreeMap<ValueIdentity, Proof>,
    results: Option<std::cell::RefCell<std::collections::HashMap<usize, PublicGuard>>>,
}

struct PublicGuard {
    // Keep the source address unique for this pass without retaining its condition graph.
    _source: std::sync::Weak<ClauseNode>,
    possible: Option<bool>,
    facts: Option<Facts>,
    projected: Option<Clause>,
}

impl PublicGuard {
    fn new(source: &Clause) -> Self {
        Self { _source: Arc::downgrade(&source.0), possible: None, facts: None, projected: None }
    }
}

impl<'a> PublicGuards<'a> {
    pub(super) fn new(bindings: &'a BTreeMap<ValueIdentity, Proof>) -> Self {
        Self { bindings, results: None }
    }

    pub(super) fn stored(bindings: &'a BTreeMap<ValueIdentity, Proof>) -> Self {
        Self { bindings, results: Some(std::cell::RefCell::new(std::collections::HashMap::new())) }
    }

    pub(super) fn possible(&self, source: &Clause) -> bool {
        let Some(results) = &self.results else { return source.possible(); };
        if !matches!(&**source.0, ClauseNode::Both(..) | ClauseNode::Either(..) | ClauseNode::Branch(..)) {
            return source.possible();
        }
        let mut results = results.borrow_mut();
        let result = results.entry(source.key()).or_insert_with(|| PublicGuard::new(source));
        if let Some(possible) = result.possible {
            return possible;
        }
        result.facts = source.required();
        let possible = result.facts.is_some();
        result.possible = Some(possible);
        possible
    }

    pub(super) fn project(&self, source: &Clause) -> Clause {
        let Some(results) = &self.results else { return source.project_guard(self.bindings); };
        let mut results = results.borrow_mut();
        let result = results.entry(source.key()).or_insert_with(|| PublicGuard::new(source));
        if let Some(projected) = &result.projected {
            return projected.clone();
        }
        let projected = source.project_guard_in(&Clause::ordered([source]), self.bindings, |root| {
            if result.possible == Some(false) {
                None
            } else {
                result.facts.take().or_else(|| root.required())
            }
        });
        result.facts = None;
        result.projected = Some(projected.clone());
        projected
    }
}

pub(super) struct GuardProjection<'a> {
    source: &'a Clause,
    bindings: &'a BTreeMap<ValueIdentity, Proof>,
    projected: std::cell::OnceCell<Clause>,
}
impl<'a> GuardProjection<'a> {
    pub(super) fn new(source: &'a Clause, bindings: &'a BTreeMap<ValueIdentity, Proof>) -> Self {
        Self { source, bindings, projected: std::cell::OnceCell::new() }
    }
    pub(super) fn project(&self, source: &Clause, bindings: &BTreeMap<ValueIdentity, Proof>) -> Clause {
        if Arc::ptr_eq(&source.0, &self.source.0) && std::ptr::eq(bindings, self.bindings) {
            self.projected.get_or_init(|| self.source.project_guard(self.bindings)).clone()
        } else {
            source.project_guard(bindings)
        }
    }
}
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
    fn at_calls(
        roots: &[&Self],
        ordered: &[&Self],
        bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
        numeric_context: impl FnOnce(&[&Self]) -> bool,
        mut required: impl FnMut(&Self) -> Option<Facts>,
    ) -> Vec<Self> {
        let mut clauses = BTreeMap::new();
        let mut private_numeric = false;
        for &clause in ordered {
            let mapped = match &**clause.0 {
                ClauseNode::Impossible => Clause::impossible(),
                ClauseNode::Facts(facts) => {
                    private_numeric = private_numeric || facts.iter().any(|fact| Self::private_numeric(fact, bindings));
                    Clause::call_facts(facts.iter().copied(), bindings)
                },
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
        // An impossible result has no consumer for supplementary bounds.
        let mut outputs = roots.iter().copied()
            .filter(|root| !matches!(&**clauses[&root.key()].0, ClauseNode::Impossible))
            .peekable();
        // The other case can trigger numeric projection without itself being projected.
        if outputs.peek().is_some() && (private_numeric || numeric_context(ordered)) {
            let mut roots: Vec<_> = outputs.collect();
            // Consume the original numeric condition before private snapshots leave scope.
            roots.sort_by_key(|root| root.key());
            roots.dedup_by_key(|root| root.key());
            for root in &roots {
                let numeric = match required(root) {
                    Some(facts) => Clause::call_facts(
                        facts.into_iter().filter(|value| matches!(value, Fact::Minimum(..) | Fact::Maximum(..))),
                        bindings,
                    ),
                    None => Clause::impossible(),
                };
                let mapped = clauses.get_mut(&root.key()).unwrap();
                *mapped = Clause::both(mapped, &numeric);
            }
        }
        roots.iter().map(|root|clauses[&root.key()].clone()).collect()
    }
    fn private_numeric(fact: &Fact, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> bool {
        match fact {
            Fact::Below(a, b, _, _) => Proof::call_identity(*a, bindings)
                .zip(Proof::call_identity(*b, bindings)).is_none(),
            Fact::Index(id, IndexDomain::Computed(expression)) =>
                Proof::call_identity(*id, bindings).and_then(|_| expression.at_call(bindings)).is_none(),
            _ => false,
        }
    }

    fn has_private_numeric(&self, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> bool {
        Self::any_fact([self],&|fact|Self::private_numeric(fact,bindings))
    }

    pub(super) fn project_guard(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        self.project_guard_in(&Self::ordered([self]), bindings, Self::required)
    }

    fn project_guard_in(
        &self, ordered: &[&Self], bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>,
        required: impl FnMut(&Self) -> Option<Facts>,
    ) -> Self {
        Self::at_calls(&[self], ordered, bindings, |ordered| {
            // A reversed Below uses the same identities. Only a negated computed
            // exclusion can introduce a numeric trigger absent from the original path.
            ordered.iter().any(|clause| {
                matches!(&**clause.0, ClauseNode::Facts(facts) if facts.iter().any(|fact| {
                    matches!(*fact, Fact::Outside(id, domain @ IndexDomain::Computed(_))
                        if Self::private_numeric(&Fact::Index(id, domain), bindings))
                }))
            }) && self.negate().has_private_numeric(bindings)
        }, required).pop().unwrap()
    }

    fn call_facts(facts: impl IntoIterator<Item = Fact>, bindings: &BTreeMap<ValueIdentity, impl std::borrow::Borrow<Proof>>) -> Self {
        let mut facts = facts.into_iter().filter_map(|fact| Proof::call_fact(fact, bindings));
        let mut projected = Facts::new();
        for clause in facts.by_ref() {
            match &**clause.0 {
                ClauseNode::Impossible => return clause,
                ClauseNode::Facts(value) => projected.extend(value.iter().copied()),
                _ => {
                    let mut result = Self::both(&Self::facts(projected), &clause);
                    while !matches!(&**result.0, ClauseNode::Impossible) {
                        let Some(clause) = facts.next() else { break; };
                        result = Self::both(&result, &clause);
                    }
                    return result;
                }
            }
        }
        Self::facts(projected)
    }
    pub(super) fn values<'a>(values:impl IntoIterator<Item=(Scalar,ValueIdentity,&'a Proof)>)->Self {
        let mut facts=Facts::new();
        for (scalar,id,proof) in values{proof_facts::append_identity(&mut facts,proof,scalar,id);}
        Self::facts(facts)
    }
    pub(super) fn substitute_values(&self, values: &BTreeMap<ValueIdentity, Proof>) -> Self {
        // A synthesized complement uses only identities already in this path.
        let ordered = Self::ordered([self]);
        let identities = std::cell::RefCell::new(BTreeSet::new());
        Self::visit_identities(&ordered, &|id| {
            identities.borrow_mut().insert(id);
            id
        });
        let bindings = Proof::value_bindings(identities.into_inner(), |id| values.get(&id), |id| Proof::default().with_identity(id));
        self.project_guard_in(&ordered, &bindings, Self::required)
    }

    fn visit_identities(
        ordered: &[&Self],
        identity: &impl Fn(ValueIdentity) -> ValueIdentity,
    ) {
        for &clause in ordered {
            if let ClauseNode::Facts(facts) = &**clause.0 {
                for fact in facts {
                    fact.map(identity);
                }
            }
        }
    }

    fn node(value: ClauseNode) -> Self {
        Self(ManuallyDrop::new(Arc::new(value)))
    }
    fn into_node(self) -> Arc<ClauseNode> {
        let mut owned = ManuallyDrop::new(self);
        // 已消费Clause不再运行Drop，直接转交它原来持有的Arc。
        unsafe { ManuallyDrop::take(&mut owned.0) }
    }

    fn negate(&self)->Self { negation::negate(self) }
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
    fn facts(facts: Facts) -> Self { combination::facts(facts) }
    pub(super) fn both(a: &Self, b: &Self) -> Self { combination::both(a, b) }
    fn observed(parent:&Self,facts:Facts)->Self {
        let known=Arc::new(Condition{cases:[Self::impossible(),Self::facts(facts)]});
        Self::branch(parent,&known,true)
    }
    fn branch(parent: &Self, condition: &Arc<Condition>, truth: bool) -> Self {
        combination::branch(parent, condition, truth)
    }
    pub(super) fn either(a: &Self, b: &Self) -> Self { combination::either(a, b) }

    fn query_inputs(&self) -> Option<(&Self, &Self, bool)> {
        match &**self.0 {
            ClauseNode::Both(a, b) => Some((a, b, false)),
            ClauseNode::Branch(parent, condition, truth) =>
                Some((parent, &condition.cases[usize::from(*truth)], false)),
            ClauseNode::Either(a, b) => Some((a, b, true)),
            _ => None,
        }
    }
    fn key(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }
    pub(super) fn scalar_identities<'a>(roots:impl IntoIterator<Item=&'a Self>)->BTreeSet<ValueIdentity> {
        let identities=std::cell::RefCell::new(BTreeSet::new());
        Self::any_scalar_identity(roots,&|id|{identities.borrow_mut().insert(*id);false});
        identities.into_inner()
    }
    pub(super) fn any_scalar_identity<'a>(
        roots:impl IntoIterator<Item=&'a Self>, test:&impl Fn(&ValueIdentity)->bool,
    )->bool {
        Self::any_fact(roots,&|fact|{
            let found=std::cell::Cell::new(false);
            fact.map(&|id|{found.set(found.get()||test(&id));id});
            found.get()
        })
    }
    fn any_fact<'a>(roots:impl IntoIterator<Item=&'a Self>,test:&impl Fn(&Fact)->bool)->bool {
        let mut pending=Vec::new();let mut visited=HashSet::new();
        for root in roots {
            pending.push(root);
            while let Some(clause)=pending.pop() {
                if !visited.insert(clause.key()) { continue; }
                match &**clause.0 {
                    ClauseNode::Facts(facts)=>if facts.iter().any(test){return true;},
                    ClauseNode::Both(a,b)|ClauseNode::Either(a,b)=>pending.extend([b,a]),
                    ClauseNode::Branch(parent,condition,truth)=>{
                        pending.extend([&condition.cases[usize::from(*truth)],parent]);
                    },
                    ClauseNode::Impossible=>{},
                }
            }
        }
        false
    }
    // 按子节点在先的顺序只访问每个共享节点一次，不展开路径的笛卡尔积。
    fn ordered<'a>(roots: impl IntoIterator<Item = &'a Self>) -> Vec<&'a Self> {
        let mut pending: Vec<_> = roots.into_iter().map(|root| (root, false)).collect();
        pending.reverse();
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
    pub(super) fn canonical(&self) -> Self { canonical::canonical(self) }
    pub(super) fn task_return_source(component:usize,source:u32) -> Self {
        let id=ValueIdentity::TaskReturnSource(component);
        let source=Bits::from(source);
        Self::facts([Fact::Minimum(id,Scalar::U32,source),Fact::Maximum(id,Scalar::U32,source)].into())
    }
    pub(super) fn task_return_sources(&self,component:usize) -> Option<BTreeSet<u32>> {
        task_sources::query(self, component)
    }
    pub(super) fn zero_results(registers:impl IntoIterator<Item=Reg>)->Self {
        Self::facts(registers.into_iter().map(|reg|Fact::Zero(ValueIdentity::Register(reg))).collect())
    }
    pub(super) fn task_control(reg:Reg,truth:bool) -> Self {
        Self::facts([Fact::Boolean(ValueIdentity::TaskControl(reg),truth)].into())
    }
    pub(super) fn task_controls(&self) -> Option<BTreeMap<Reg,bool>> {
        Some(self.required()?.into_iter().filter_map(|fact|match fact {
            Fact::Boolean(ValueIdentity::TaskControl(reg),truth)=>Some((reg,truth)),
            _=>None,
        }).collect())
    }
    fn required(&self) -> Option<Facts> {
        self.required_in(&Facts::new()).map(|required| required.facts)
    }
    fn required_in<'a>(&'a self, environment: &'a Facts) -> Option<RequiredFacts> {
        let mut environment_roots = equality_roots(environment);
        if contradictory_with_roots(environment, &environment_roots) {
            return None;
        }
        let mut query = condition_graph::RequiredQuery::new(self);
        let mut environment: Cow<'a, Facts> = Cow::Borrowed(environment);
        let unexpanded = BTreeMap::new();
        loop {
            // 共同环境的别名事实只在原环境所有者处生产一次。
            let environment_members = equality_members(&environment_roots);
            {
                let empty = Facts::new();
                let inherited = equivalent_facts(
                    &environment,
                    QueryFacts::Borrowed(&empty),
                    &environment_roots,
                    &environment_members,
                    Some(&unexpanded),
                );
                if !inherited.is_empty() {
                    environment.to_mut().extend(inherited.iter().copied());
                }
            }
            let required = query.evaluate(&environment, &environment_roots, &environment_members)?;
            let unchanged = required.is_empty();
            // The prior roots already close the original environment. Inherited
            // facts only copy a computed result to an existing member of its group.
            let computed_roots_changed = extend_equality_roots(&mut environment_roots, required.iter())
                || required.iter().any(|fact| matches!(fact, Fact::Index(_, IndexDomain::Computed(_))));
            if !unchanged {
                if environment.is_empty() {
                    environment = match required {
                        QueryFacts::Borrowed(facts) => Cow::Borrowed(facts),
                        QueryFacts::Owned(facts) => Cow::Owned(Arc::unwrap_or_clone(facts)),
                    };
                } else {
                    environment.to_mut().extend(required.iter().copied());
                }
            }
            if computed_roots_changed {
                extend_computed_roots(&mut environment_roots, environment.iter());
            }
            let numeric = integer_relation_facts(&environment, &environment_roots)?;
            if unchanged && numeric.is_empty() {
                return Some(RequiredFacts { facts: environment.into_owned(), roots: environment_roots });
            }
            // The integer solver emits bounds, which leave the equality roots intact.
            environment.to_mut().extend(numeric);
            if contradictory_with_roots(&environment, &environment_roots) {
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
    pub(super) fn overlaps(&self, other: &Self) -> bool {
        self.overlap_query(other, true)
    }

    pub(super) fn equivalent(&self, other: &Self) -> bool {
        condition_graph::equivalent(self, other)
    }

    fn overlap_query(&self, other: &Self, other_truth: bool) -> bool {
        condition_graph::overlap(self, other, other_truth)
    }

}
fn contradictory(facts: &Facts) -> bool {
    contradictory_with_roots(facts, &equality_roots(facts))
}
fn contradictory_with_roots(facts: &Facts, roots: &BTreeMap<ValueIdentity, ValueIdentity>) -> bool {
    internal_conflicts::conflict(facts, roots)
}

// 环境已由本轮固定点检查；这里只处理新增事实内部及其与环境的关系。
fn additions_conflict(environment: &Facts, additions: &Facts) -> bool {
    contradictory(additions) || cross_conflict(environment, additions, true)
}
// 两侧已各自完成内部检查；共有事实不会与另一侧冲突。
fn cross_conflict(a: &Facts, b: &Facts, may_share: bool) -> bool {
    cross_facts::conflict(a, b, may_share)
}


fn combine_facts<'a>(mut a: QueryFacts<'a>, mut b: QueryFacts<'a>) -> Option<QueryFacts<'a>> {
    if a.is_subset(&b) {
        return Some(b);
    }
    if b.is_subset(&a) {
        return Some(a);
    }
    if cross_conflict(&a, &b, true) {
        return None;
    }
    if a.len() < b.len() { std::mem::swap(&mut a, &mut b); }
    a.extend(b.iter().copied());
    Some(a)
}
fn either_facts<'a>(
    environment: &Facts,
    environment_roots: &BTreeMap<ValueIdentity, ValueIdentity>,
    environment_members: &[(&ValueIdentity, &ValueIdentity)],
    a: Option<QueryFacts<'a>>,
    b: Option<QueryFacts<'a>>,
) -> Option<QueryFacts<'a>> {
    common_facts::either(environment, environment_roots, environment_members, a, b)
}
impl PartialEq for Clause {
    fn eq(&self, other: &Self) -> bool { same_condition_roots([self], [other]) }
}
impl Eq for Clause {}
impl PartialOrd for Clause {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}
impl Ord for Clause {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if Arc::ptr_eq(&self.0, &other.0) {
            return std::cmp::Ordering::Equal;
        }
        Condition::paths(self.clone(), Self::impossible())
            .cmp(&Condition::paths(other.clone(), Self::impossible()))
    }
}

impl PartialEq for Condition {
    fn eq(&self, other: &Self) -> bool {
        same_condition_roots(self.cases.each_ref(), other.cases.each_ref())
    }
}
impl Eq for Condition {}
impl PartialOrd for Condition {
    fn partial_cmp(&self,other:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(other))}
}
impl Ord for Condition {
    fn cmp(&self,other:&Self)->std::cmp::Ordering {
        if self.cases.iter().zip(&other.cases).all(|(a,b)| Arc::ptr_eq(&a.0,&b.0)) {
            return std::cmp::Ordering::Equal;
        }
        codec::conditions::order(self, other)
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
        let names=self.scalar_identities().into_iter().filter_map(|id|match id{
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

    pub(super) fn scalar_identities(&self)->BTreeSet<ValueIdentity> {
        Clause::scalar_identities(&self.cases)
    }
    pub(super) fn value_bindings<'a>(&self, values: impl FnMut(ValueIdentity) -> Option<&'a Proof>) -> BTreeMap<ValueIdentity, Cow<'a, Proof>> {
        Proof::value_bindings(self.scalar_identities(), values, |id| Proof::default().with_identity(id))
    }

    pub(super) fn substitute_values(&self, values: &BTreeMap<ValueIdentity, Proof>) -> Self {
        self.at_call(&self.value_bindings(|id| values.get(&id)))
    }

    pub(super) fn element_index(depth:usize,index:Operand)->Self {
        Self::index_equal(Operand::Value(ValueIdentity::ElementIndex(depth)),index)
    }
    pub(super) fn index_equal(a:Operand,b:Operand)->Self {index_conditions::equal(a,b)}
    pub(super) fn index_below((a,a_proof):(impl Into<Option<Operand>>,&Proof),(b,b_proof):(impl Into<Option<Operand>>,&Proof),strict:bool)->Self {
        index_conditions::below(a.into(),a_proof,b.into(),b_proof,strict)
    }
    pub(super) fn guard(path:Clause)->Self{Self{cases:[path.negate(),path]}}
    pub(super) fn execution(path:&Clause)->Self {execution_conditions::execution(path)}
    pub(super) fn projected_case(&self, truth: bool, bindings: &BTreeMap<ValueIdentity, Proof>) -> Clause {
        let index = usize::from(truth);
        Clause::at_calls(
            &[&self.cases[index]],
            &Clause::ordered([&self.cases[index]]),
            bindings,
            |_| self.cases[1 - index].has_private_numeric(bindings),
            Clause::required,
        ).pop().unwrap()
    }

    pub(super) fn guaranteed_at_call(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        Self::guard(self.projected_case(false, bindings)).negate()
    }

    pub(super) fn guard_at_call(&self, bindings: &BTreeMap<ValueIdentity, Proof>) -> Self {
        // Project actual input relations before forming their complement.
        Self::guard(self.projected_case(true, bindings))
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
    pub(super) fn at_call(&self,bindings:&BTreeMap<ValueIdentity,impl std::borrow::Borrow<Proof>>)->Self{
        let mut cases=Clause::at_calls(&self.cases.each_ref(), &Clause::ordered(&self.cases), bindings, |_| false, Clause::required).into_iter();
        Self{cases:[cases.next().unwrap(),cases.next().unwrap()]}
    }
    pub(super) fn output_bound(&self,bindings:&OutputBindings)->Self{
        let cases=Clause::output_bounds(&self.cases,bindings);
        Self{cases:std::array::from_fn(|i|cases[i].clone())}
    }
    pub(super) fn assume(&self,path:&Clause,truth:bool)->Clause{Clause::both(path,&self.cases[usize::from(truth)])}
    pub(super) fn truth_in(&self,path:&Clause)->Option<bool>{
        if !self.assume(path,false).possible(){Some(true)}else if !self.assume(path,true).possible(){Some(false)}else{None}
    }
    pub(super) fn mentions(&self,identity:ValueIdentity)->bool{Clause::any_fact(&self.cases,&|fact|match fact{Fact::Equal(a,b)|Fact::NotEqual(a,b)|Fact::Below(a,b,..)=>*a==identity||*b==identity,Fact::Predicate(predicate,_)=>predicate.mentions(identity),Fact::Index(a,_)|Fact::Outside(a,_)|Fact::Zero(a)|Fact::Minimum(a,..)|Fact::Maximum(a,..)|Fact::NonZero(a)|Fact::Boolean(a,_)=>*a==identity})}
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
    pub(super) fn common(a: Self, b: Self) -> Self {
        let [af, at] = a.cases;
        let [bf, bt] = b.cases;
        Self {
            cases: [Clause::either(&af, &bf), Clause::either(&at, &bt)],
        }
    }
    pub fn of(value: &Value) -> Self {
        Self::component(value, 0)
    }
    pub(super) fn range_value<I: IntoIterator<Item = (Bits, Bits)>>(value: &Value, scalar: Scalar, intervals: impl Fn(usize) -> I) -> Self {
        let mut condition = Clause::default();
        for component in 0..value.regs.len() {
            let identity = value.component_identity(component);
            let mut selected = Clause::impossible();
            for (lower, upper) in intervals(component) {
                selected = Clause::either(&selected, &Clause::facts([
                    Fact::Minimum(identity, scalar, lower), Fact::Maximum(identity, scalar, upper),
                ].into()));
            }
            condition = Clause::both(&condition, &selected);
        }
        Self { cases: [condition.negate(), condition] }
    }
    fn component(value: &Value, component: usize) -> Self {
        value_conditions::component(value,component)
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
        comparison_conditions::compare(op,a,b)
    }
}
fn constrain_proofs(facts: &mut Facts,index: (ValueIdentity,&Proof),limit: (ValueIdentity,&Proof),strict: bool,scalar: Scalar) {
    comparison_conditions::constrain(facts,index,limit,strict,scalar)
}
// These coefficients belong to one numeric query. The original snapshot
// equations and scalar bounds remain the facts' only source.
#[derive(Clone, Copy, PartialEq, Eq)]
struct IntegerAffine {
    source: Option<ValueIdentity>,
    scale: i128,
    offset: i128,
}
struct IntegerQuery<'a> {
    facts: &'a Facts,
    roots: &'a BTreeMap<ValueIdentity, ValueIdentity>,
    lower: BTreeMap<(ValueIdentity, Scalar), Bits>,
    upper: BTreeMap<(ValueIdentity, Scalar), Bits>,
    coefficients: BTreeMap<(ValueIdentity, Scalar), IntegerAffine>,
    expanding: BTreeSet<(ValueIdentity, Scalar)>,
}
impl<'a> IntegerQuery<'a> {
    fn new(facts: &'a Facts, roots: &'a BTreeMap<ValueIdentity, ValueIdentity>) -> Self {
        let (lower,upper)=numeric_bounds::collect(facts,Some(roots));
        Self { facts, roots, lower, upper, coefficients: BTreeMap::new(), expanding: BTreeSet::new() }
    }
    fn root(&self, id: ValueIdentity) -> ValueIdentity { self.roots.get(&id).copied().unwrap_or(id) }
    fn number(scalar: Scalar, bits: Bits) -> i128 {
        if scalar.is_signed() { i128::from(scalar.signed(bits)) }
        else { i128::from(scalar.truncate(bits)) }
    }
    fn computed(&self, id: ValueIdentity, scalar: Scalar) -> Option<IntegerExpression> {
        let root = self.root(id);
        self.facts.iter().find_map(|fact| match fact {
            Fact::Index(value, IndexDomain::Computed(expression @ IntegerExpression::Binary(_, actual, _, _)))
                if self.root(*value) == root && *actual == scalar => Some(*expression),
            _ => None,
        })
    }
    fn source_plan(&mut self,left:ValueIdentity,right:ValueIdentity,scalar:Scalar)->Option<(ValueIdentity,Vec<ValueIdentity>)> {integer_relations::source_plan(self,left,right,scalar)}
    fn below_in(&mut self,left:ValueIdentity,right:ValueIdentity,scalar:Scalar,source:ValueIdentity,bounds:(Bits,Bits),order:&[ValueIdentity])->Option<bool> {integer_relations::below_in(self,left,right,scalar,source,bounds,order)}
    fn bounds(&self, id: ValueIdentity, scalar: Scalar) -> (i128, i128) {
        let key = (self.root(id), scalar);
        let lower = self.lower.get(&key).copied().unwrap_or(0);
        let upper = self.upper.get(&key).copied().unwrap_or(scalar.bit_mask());
        (Self::number(scalar, lower ^ scalar.order_bias()), Self::number(scalar, upper ^ scalar.order_bias()))
    }
    fn interval(&self,value:IntegerAffine,scalar:Scalar)->Option<(i128,i128)> {integer_relations::interval(self,value,scalar)}
    fn operand(&mut self,operand:Operand,scalar:Scalar)->IntegerAffine {integer_relations::operand(self,operand,scalar)}
    fn expression(&mut self,expression:IntegerExpression,scalar:Scalar)->Option<IntegerAffine> {integer_relations::expression(self,expression,scalar)}
    fn retain(&self,output:&mut Facts,id:ValueIdentity,scalar:Scalar,lower:i128,upper:i128)->Option<()> {integer_relations::retain(self,output,id,scalar,lower,upper)}
    fn operand_bounds(&self, operand: Operand, scalar: Scalar) -> (i128, i128) {
        match operand {
            Operand::Constant(_, bits) => { let value = Self::number(scalar, bits); (value, value) }
            Operand::Value(id) => self.bounds(id, scalar),
        }
    }
    fn quotient(&self,output:&mut Facts,id:ValueIdentity,scalar:Scalar,numerator:Operand,divisor:Operand)->Option<()> {integer_relations::quotient(self,output,id,scalar,numerator,divisor)}
    fn at_most_zero(&self,output:&mut Facts,value:IntegerAffine,scalar:Scalar)->Option<()> {integer_relations::at_most_zero(self,output,value,scalar)}
}

fn integer_relation_facts(facts:&Facts,roots:&BTreeMap<ValueIdentity,ValueIdentity>)->Option<Facts> {integer_relations::facts(facts,roots)}
/// 数值区间逐分量计算；资源索引域只沿已证明的无符号关系传递。
pub(super) fn math(op: MathOp, args: &[Value]) -> Proof {
    let width = args.iter().map(|value| value.regs.len()).max().unwrap();
    Proof {
        ranges: (0..width).map(|i| floating::math(op, args, i)).collect(),
        ..Proof::default()
    }
}
pub(super) fn arithmetic(builder: &Builder<'_>, op: &str, a: &Value, b: &Value) -> Proof {
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
                    let range = arithmetic_ranges(op, scalar, &left, &right);
                    if !signed && op == "-" {
                        let left_origins = a.component_origins(ai);
                        let right_origins = b.component_origins(bi);
                        if let Some(minimum) = a.proof.domains(ai).iter().copied().filter_map(|domain| match domain {
                            IndexDomain::Lower(base, distance) | IndexDomain::Offset(base, distance) if right_origins.contains(&base) => Some(distance),
                            _ => None,
                        }).chain(b.proof.domains(bi).iter().copied().filter_map(|domain| match domain {
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
        domains: if scalar == Scalar::F32 {
            (0..count).map(|i| floating::arithmetic_domains(builder, op, a, b, i)).collect()
        } else if scalar == Scalar::U32 {
            (0..count)
                .map(|i| arithmetic_domains(op, a, b, i))
                .collect()
        } else {
            Vec::new()
        },
        ..Proof::default()
    };
    if scalar == Scalar::F32 {
        floating::refine(&mut proof);
    }
    if scalar.is_integer() {
        let shift_input = matches!(op, "<<" | ">>").then(|| Proof::capture(a));
        for component in 0..count {
            if let Some(input) = &shift_input {
                let source = input.known_bits(component % a.regs.len(), scalar)
                    .unwrap_or(KnownBits { scalar, mask: 0, value: 0 });
                let counts = b.component_ranges(component % b.regs.len()).shift_counts(scalar);
                if let Some(bits) = source.shifted(op == "<<", counts) {
                    proof.mark_domain(component, bits.domain());
                }
            }
            if let Some((value, source)) = arithmetic_source(op, a, b, component) {
                let mut exact = value.projected_proof(&[source]);
                // 继承操作数身份，但仍与运算的可达片段相交，保留空路径。
                exact.ranges[0] = exact
                    .range_set(0)
                    .combine(&proof.range_set(component), |(al, ah), (bl, bh)| {
                        ScalarRanges::interval(al.max(bl), ah.min(bh))
                    });
                exact.domains[0].extend(proof.domains(component).iter().copied());
                proof.assign(&[component], count, exact);
            }
        }
    }
    proof
}
fn arithmetic_ranges(op: &str, scalar: Scalar, left: &ScalarRanges, right: &ScalarRanges) -> ScalarRanges {
    let shifted;
    let right = if matches!(op, "<<" | ">>") {
        shifted = right.shift_counts(scalar);
        &shifted
    } else { right };
    left.combine(right, |left, right| {
        let bitwise = |operation| {
            let (lower, upper) = bitwise::range(left, right, scalar, operation);
            ScalarRanges::interval(lower, upper)
        };
        match op {
            "&" => bitwise(bitwise::Operation::And),
            "|" => bitwise(bitwise::Operation::Or),
            "^" => bitwise(bitwise::Operation::Xor),
            _ => integer::arithmetic(op,scalar,left,right),
        }
    })
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
fn arithmetic_domains(op:&str,a:&Value,b:&Value,component:usize)->Domains {index_domains::arithmetic(op,a,b,component)}
pub(super) fn unary(op: UnaryOp, value: &Value) -> Proof {
    if op == UnaryOp::LogicalNot {
        return Proof::condition(Condition::of(value).negate());
    }
    let mut proof = Proof::default();
    let scalar = value.ty.components[0].0;
    if op == UnaryOp::FloatNegate {
        proof.ranges = (0..value.regs.len()).map(|i| floating::negate(value, i)).collect();
    } else if scalar.is_integer() {
        let operation=match op {
            UnaryOp::BitNot=>Some(integer::Unary::BitNot),
            UnaryOp::UnsignedNegate|UnaryOp::SignedNegate=>Some(integer::Unary::Negate),
            _=>None,
        };
        proof.ranges=(0..value.regs.len()).map(|i|value.component_ranges(i).map(|range|
            operation.map_or_else(ScalarRanges::default,|operation|integer::unary(operation,scalar,range)))).collect();
    }
    proof
}
fn integer_result_range(range:(i128,i128),bias:Bits,width:u32)->ScalarRanges {
    integer::project(range,bias,width)
}
pub(super) fn indices_below(domains:&Domains,strict:bool)->Domains {index_domains::below(domains,strict)}
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
        let Some(RequiredFacts { facts, roots }) = self.path.required_in(&environment) else { return false; };
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
        if matches!(operation,IntegerOperation::And|IntegerOperation::Or|IntegerOperation::Xor) {
            let inputs=[Proof::capture(a),Proof::capture(b)];
            for component in 0..result.regs.len() {
                if let Some(bits)=KnownBits::bitwise(operation,scalar,
                    inputs[0].known_bits(component%a.regs.len(),scalar),
                    inputs[1].known_bits(component%b.regs.len(),scalar)) {
                    result.proof.mark_domain(component,bits.domain());
                }
            }
        }
        let mut facts: Facts = (0..result.regs.len()).map(|component| {
            let left = Operand::of(a, component % a.regs.len());
            let right = Operand::of(b, component % b.regs.len());
            let domain = IndexDomain::Computed(IntegerExpression::Binary(operation, scalar, left, right));
            result.proof.mark_domain(component, domain);
            Fact::Index(result.component_identity(component), domain)
        }).collect();
        proof_facts::append_value(&mut facts,result,true);
        self.path = Clause::observed(&self.path,facts);
    }
    fn integer_below(&self, left: ValueIdentity, right: ValueIdentity, scalar: Scalar) -> bool {
        let environment = environment_facts(&self.env);
        let Some(RequiredFacts { facts, roots }) = self.path.required_in(&environment) else { return false; };
        let mut query = IntegerQuery::new(&facts, &roots);
        if query.bounds(left, scalar).1 < query.bounds(right, scalar).0 { return true; }
        let Some((source, order)) = query.source_plan(left, right, scalar) else { return false; };
        let mut ranges: Option<ScalarRanges> = None;
        let mut collect = |proof: &Proof, ty: &Ty| {
            for (component, &(actual, _)) in ty.components.iter().enumerate() {
                if actual == scalar && proof.origins.get(component).is_some_and(|origins|
                    origins.iter().any(|id| query.root(*id) == source))
                {
                    let current = proof.element_range(component, scalar);
                    ranges = Some(match ranges.take() {
                        Some(previous) => previous.combine(&current, |(a,b),(c,d)| ScalarRanges::interval(a.max(c),b.min(d))),
                        None => current,
                    });
                }
            }
        };
        for binding in self.env.values() {
            match binding {
                Binding::Value(value) => collect(&Proof::capture(value), &value.ty),
                Binding::OwnedValue(owner) => collect(&Proof::capture(&owner.value), &owner.value.ty),
                Binding::Owned(owner) | Binding::ViewElement { element: owner, .. } => collect(&owner.proof, owner.place.ty()),
                _ => {}
            }
        }
        let Some(ranges) = ranges else { return false; };
        ranges.0.iter().all(|&range| IntegerQuery::new(&facts, &roots)
            .below_in(left, right, scalar, source, range, &order) == Some(true))
    }
    pub(super) fn observe_bounds(&mut self, value: &Value) {
        self.path = Clause::observed(&self.path,proof_facts::value(value));
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
            let Some(RequiredFacts { facts: context, roots }) = candidate_path.required_in(&environment) else {
                continue;
            };
            next.extend_equalities(&roots);
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
        element: &Ty,
        count: u32,
        index: ownership::ApplicationIndex,
        registers: &[Reg],
    ) -> Proof {
        let width = element.components.len();
        if width == 0 {
            return Proof::default();
        }
        let choices: Vec<_> = (0..count as usize)
            .map(|i| ((i * width..(i + 1) * width).collect(), Clause::selection(index, i)))
            .collect();
        let (result, path) = Self::selected_proof(
            &self.path, environment_facts(&self.env), proof, array, element, &choices, Some(registers),
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
            || lengths.iter().any(|(proof, component)| Proof::below_length((&value.component_origins(0), &value.proof.domains(0)), (proof.origins.get(*component).map(Vec::as_slice).unwrap_or_default(), &proof.domains(*component)), true))
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
            let mut facts=Facts::new();proof_facts::append_component(&mut facts,proof,component,Scalar::U32);
            known=Clause::both(&known,&Clause::facts(facts));
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
            let Some(RequiredFacts { facts, roots }) = self.path.required_in(&environment) else {
                return;
            };
            // Keep branch-derived integer bounds on the path that established them.
            self.path = Clause::both(&self.path, &Clause::facts(facts.difference(&environment)
                .filter(|fact| match fact {
                    Fact::Minimum(_, scalar, _) | Fact::Maximum(_, scalar, _) => scalar.is_integer(),
                    Fact::Zero(_) | Fact::NonZero(_) => true,
                    _ => false,
                }).copied().collect()));
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
    pub(super) fn common_element_proof(&self, source: (usize, u32), words: &[u32],
        writes: Option<&BTreeMap<(u32, u32), Option<Box<Proof>>>>,
    ) -> Option<Proof> {
        let (binding, base) = source;
        let common = self.element_facts?.get(&binding)?;
        let ty = &self.program.buffers[binding].ty;
        let components = words.iter().map(|word| ty.components.iter().position(|(_, at)| *at == base + *word).unwrap()).collect::<Vec<_>>();
        let mut common = common.project(&components, ty.components.len());
        let changed = components.iter().enumerate().filter_map(|(component, &source)| {
            let (scalar, offset) = ty.components[source];
            writes.is_some_and(|writes| writes.keys().any(|&(start, end)|
                u64::from(offset) < u64::from(end) && u64::from(start) < u64::from(offset)+u64::from(scalar.size()))).then_some(component)
        }).collect::<Vec<_>>();
        common.forget_value(&changed, words.len());
        Some(common.element_facts(|target| self.data_index_proof(target)
            .with_domains(Domains::from([IndexDomain::Data(target)]))))
    }
    // A reduction sums distinct elements, not repeated copies of one identity.
    pub(super) fn tile_sum(&self, tile: usize, words: &[u32], input: &Proof, ty: &Ty, length: &Proof) -> Proof {
        let (minimum, maximum) = length.range_set(0).restrict(1, Bits::from(u32::MAX)).hull();
        Proof {
            ranges: ty.components.iter().enumerate().map(|(component, (scalar, _))| {
                if minimum > maximum { return ScalarRanges(Vec::new()); }
                if scalar.is_float() { return floating::sum(input, component, *scalar, maximum); }
                if !scalar.is_integer() { return ScalarRanges::default(); }
                let (lower, upper) = input.element_range(component, *scalar).hull();
                if lower > upper { return ScalarRanges(Vec::new()); }
                let number = |bits| if scalar.is_signed() {
                    i128::from(scalar.signed(bits ^ scalar.order_bias()))
                } else { i128::from(bits) };
                let (lower, upper) = (number(lower), number(upper));
                let ends = [lower*i128::from(minimum), lower*i128::from(maximum),
                    upper*i128::from(minimum), upper*i128::from(maximum)];
                let mut range = (*ends.iter().min().unwrap(), *ends.iter().max().unwrap());
                if let Some(count) = self.tile_nonzero_count(tile, words[component], maximum as u32) {
                    let count = i128::from(count);
                    range.0 = range.0.max(lower.min(0) * count);
                    range.1 = range.1.min(upper.max(0) * count);
                }
                if range.0 > range.1 { return ScalarRanges(Vec::new()); }
                integer_result_range(range, scalar.order_bias(), scalar.bit_width())
            }).collect(),
            ..Proof::default()
        }
    }
    fn tile_nonzero_count(&self, tile: usize, word: u32, maximum: u32) -> Option<u32> {
        let TileOrigin::Published(value) = &self.tiles[tile].source else { return None; };
        let TileWidth::Fixed(width) = self.tiles[tile].width else { return None; };
        let component = value.ty.components.iter().position(|(_, at)| *at == word).unwrap();
        let condition = Condition::component(value, component);
        let Some(RequiredFacts { facts, roots }) = condition.cases[1].required_in(&Facts::new()) else {
            return Some(0);
        };
        let query = IntegerQuery::new(&facts, &roots);
        let global = query.root(ValueIdentity::Register(self.global_index));
        let mut count: Option<u32> = None;
        for fact in &facts {
            let Fact::Index(lane, IndexDomain::Computed(IntegerExpression::Binary(
                IntegerOperation::Remainder, Scalar::U32, Operand::Value(index), Operand::Constant(Scalar::U32, span)
            ))) = *fact else { continue; };
            if span != Bits::from(width) || query.root(index) != global { continue; }
            let (lower, upper) = query.bounds(lane, Scalar::U32);
            let upper = upper.min(i128::from(maximum.min(width)-1));
            let mut population = if lower > upper { 0 } else { (upper-lower+1) as u32 };
            if population != 0 {
                for fact in &facts {
                    let Fact::Index(remainder, IndexDomain::Computed(IntegerExpression::Binary(
                        IntegerOperation::Remainder, Scalar::U32, Operand::Value(source), Operand::Constant(Scalar::U32, period)
                    ))) = *fact else { continue; };
                    if query.root(source) != query.root(lane) { continue; }
                    let (first, last) = query.bounds(remainder, Scalar::U32);
                    let last = last.min(i128::from(period)-1);
                    let selected = if first > last { 0 } else {
                        // Count an integer interval of residues without enumerating lanes.
                        let (first, last) = (first as u64, last as u64);
                        let prefix = |end: u64| {
                            let tail = (end % period).min(last+1);
                            end / period * (last-first+1) + tail-first.min(tail)
                        };
                        (prefix(upper as u64+1)-prefix(lower as u64)) as u32
                    };
                    population = population.min(selected);
                }
            }
            count = Some(count.map_or(population, |old| old.min(population)));
        }
        count
    }
    pub(super) fn common_tile_proof(&self, tile: usize, words: &[u32]) -> Option<Proof> {
        match &self.tiles[tile].source {
            TileOrigin::Input { binding, word, .. } =>
                self.common_element_proof((*binding, *word), words, None),
            TileOrigin::Published(value) if self.tiles[tile].access == Access::Value => {
                let components = words.iter().map(|word| value.ty.components.iter()
                    .position(|(_, byte)| byte == word).unwrap()).collect::<Vec<_>>();
                Some(Proof::capture(value).project(&components, value.regs.len()).element_facts(
                    |target| self.data_index_proof(target).with_domains(Domains::from([IndexDomain::Data(target)]))))
            }
            _ => None,
        }
    }
    pub(super) async fn read_owned_proof(&mut self, place: &Place, regs: &[Reg], reads: &[(usize,usize)]) -> Result<(Proof, bool, bool)> {
        let (snapshots,common)=match place {
            Place::ApplicationElement{..}=>{
                let snapshots=regs.iter().map(|reg|self.application_read_snapshot(*reg)).collect::<Vec<_>>();
                let common=unsafe { self.world.call(self.application_element_proof(place,&snapshots,reads)).await }?;(snapshots,common)
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
                    if let Some(common)=&common{proof.set_view_borrow(*component,common.view_borrow(target));}
                }
            }
            stored = Some((proof.project(&components, width), *uniform));
            break;
        }
        let mut known=stored.is_some();
        let mut projected=|this:&mut Self|{
            let proof=this.projected_owned_proof(place,Some(regs));
            known|=proof.is_some();
            proof.unwrap_or_default()
        };
        let (mut proof, uniform) = match place {
            Place::ApplicationElement { .. } => {
                let mut proof = match (stored, common) {
                    (Some((mut proof, _)), Some(common)) => {
                        proof.constrain_common_ranges(&common, regs.len());
                        proof
                    }
                    (Some((proof, _)), None) | (None, Some(proof)) => proof,
                    (None, None) => projected(self),
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
                    None => (projected(self), false),
                };
                if let Some(common) = self.common_element_proof((*binding, 0), words, self.memory_regions.get(binding).map(|(_, writes)| writes)) {
                    proof.constrain_common_ranges(&common, regs.len());
                }
                proof.origins.resize_with(regs.len(), Vec::new);
                for (origins, reg) in proof.origins.iter_mut().zip(regs) {
                    if origins.is_empty() {
                        origins.push(ValueIdentity::Register(self.application_read_snapshot(*reg)));
                    }
                }
                (self.path_proof(place.ty(), proof), uniform)
            }
            _ => match stored {
                Some(stored) => stored,
                None => (projected(self), false),
            },
        };
        if let Place::TileElement { tile, words, .. } = place
            && let Some(common) = self.common_tile_proof(*tile, words)
        {
            proof.constrain_common_ranges(&common, regs.len());
        }
        Ok((proof, uniform, known))
    }
    pub(super) async fn update_application_contents(&mut self, data: &application::DataReference, changed: &[u32], update: application::ElementUpdate<'_>)->Result<()> {
        unsafe { self.world.call(self.update_application_elements(&data.sources,&view::resource_borrow(&data.data),changed,update)).await }?;
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
    pub(super) async fn assign_owned_proof(&mut self, place: &Place, fresh: Proof, uniform: bool)->Result<()> {
        if let Place::ApplicationElement{sources,resource,words,index_value,index_range,..}=place {unsafe { self.world.call(self.update_application_elements(sources,resource,words,application::ElementUpdate::Element{proof:&fresh,index:Some((*index_value,*index_range)),when:None})).await }?;}
        if let Place::Indexed{locations,..}=place && let Place::ApplicationElement{sources,resource,..}=place.storage() {
            for(component,position)in locations.iter().enumerate(){
                let mut selected=BTreeSet::new();owned_borrows::positions(position,&mut selected);
                let proof=fresh.project(&[component],place.ty().components.len());
                for position in selected{let Position::Borrowed(_,_,word)=position else{unreachable!()};unsafe { self.world.call(self.update_application_elements(sources,resource,&[word],application::ElementUpdate::Element{proof:&proof,index:None,when:None})).await }?;}
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

// 所有逻辑分量及其两个结果共享节点表；编码不受 Arc 的物理共享方式影响。
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EncodedClause<F> {
    Impossible,
    Facts(F),
    Both(usize, usize),
    Either(usize, usize),
}

fn condition_node_ids<'a>(
    roots: impl IntoIterator<Item = &'a Clause>,
    record: impl FnMut(EncodedClause<&'a Facts>),
) -> BTreeMap<usize, usize> {
    codec::conditions::node_ids(roots, record)
}

fn same_condition_roots<const N: usize>(left: [&Clause; N], right: [&Clause; N]) -> bool {
    if left.iter().zip(&right).all(|(a, b)| Arc::ptr_eq(&a.0, &b.0)) {
        return true;
    }
    let indices = condition_node_ids(left.into_iter().chain(right), |_| {});
    left.map(|root| indices[&root.key()]) == right.map(|root| indices[&root.key()])
}




pub(super) fn scoped_proof(ty: &Ty, proof: Proof, path: &Clause, env: &BTreeMap<String, Binding>) -> Proof {
    proof_scope(path, env)(ty, proof)
}

pub(super) fn proof_scope(path: &Clause, env: &BTreeMap<String, Binding>) -> impl Fn(&Ty, Proof) -> Proof + use<> {
    let environment = environment_facts(env);
    let scope = path.required_in(&environment);

    move |ty, mut proof| {
        if let Some(RequiredFacts { facts, roots }) = &scope {
            proof.extend_equalities(roots);
            for component in 0..ty.components.len() {
                for domain in proof.domains(component).into_owned() {
                    if let IndexDomain::CommonRange(id, from, low, extended, to) = domain
                        && [from, low, extended, to].into_iter().all(Scalar::is_integer)
                        && let Some(bits) = KnownBits::facts(from, id, &[facts])
                    {
                        let bits = bits.cast(low).cast(extended).cast(to);
                        if bits.mask != 0 {
                            proof.mark_domain(component, bits.domain());
                        }
                    }
                }
                for fact in facts {
                    if fact.identity().is_some_and(|identity| {
                        proof.origins.get(component).is_some_and(|origins| origins.contains(&identity))
                    }) {
                        proof.refine(ty, component, *fact);
                    }
                }
            }
        }
        proof
    }
}

fn common_range_domain(domain: IndexDomain) -> bool {
    match domain {
        IndexDomain::Data(_) | IndexDomain::Fixed(_) | IndexDomain::Tile
        | IndexDomain::View(_) | IndexDomain::ApplicationData(_)
        | IndexDomain::Upper(..) | IndexDomain::Lower(..) | IndexDomain::Room(_)
        | IndexDomain::Shifted(..) | IndexDomain::CommonRange(..) | IndexDomain::KnownBits(..) | IndexDomain::Floating(..) => true,
        IndexDomain::DataLength(_) | IndexDomain::TileLength | IndexDomain::ViewLength(_)
        | IndexDomain::ApplicationLength(_) | IndexDomain::Sum(..) | IndexDomain::Offset(..)
        | IndexDomain::Computed(_) => false,
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Proof);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Address, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        origins: Slot,
        conditions: Slot,
        ranges: Slot,
        nonzero: Slot,
        domains: Slot,
        views: Slot,
        addresses: Slot,
    }
    #[repr(C)]
    struct Writer {
        origins: WriteCallback,
        conditions: WriteCallback,
        ranges: WriteCallback,
        nonzero: WriteCallback,
        domains: WriteCallback,
        views: WriteCallback,
        addresses: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        origins: ReadCallback,
        conditions: ReadCallback,
        ranges: ReadCallback,
        nonzero: ReadCallback,
        domains: ReadCallback,
        views: ReadCallback,
        addresses: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_proof"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_proof"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    unsafe extern "C" fn write_conditions(source: Address, output: Slot) {
        unsafe {
            codec::conditions::write_borrowed(
                codec::conditions::Conditions::Components(
                    &*source.cast::<Vec<Option<Box<Condition>>>>(),
                ),
                &mut *output.cast::<crate::codec::Writer>(),
            )
        }
    }
    unsafe extern "C" fn read_conditions(context: Slot, output: Slot) -> ReadStatus {
        let result = unsafe { codec::conditions::read_borrowed(context) };
        unsafe {
            contracts::finish(
                context,
                result,
                &mut *output.cast::<Vec<Option<Box<Condition>>>>(),
            )
        }
    }
    unsafe extern "C" fn empty_view(source: Address) -> u32 {
        u32::from(
            unsafe { &*source.cast::<view::Borrow>() }
                .sources
                .is_empty(),
        )
    }
    unsafe extern "C" fn empty_address(source: Address) -> u32 {
        u32::from(unsafe { &*source.cast::<Option<address::Identity>>() }.is_none())
    }
    unsafe extern "C" fn write_views(source: Address, output: Slot) {
        unsafe {
            contracts::write_trimmed(&*source.cast::<Vec<view::Borrow>>(), output, empty_view)
        }
    }
    unsafe extern "C" fn write_addresses(source: Address, output: Slot) {
        unsafe {
            contracts::write_trimmed(
                &*source.cast::<Vec<Option<address::Identity>>>(),
                output,
                empty_address,
            )
        }
    }
    unsafe extern "C" fn write_one(source: Address, output: Slot) {
        unsafe {
            codec::conditions::write_borrowed(
                codec::conditions::Conditions::One(&*source.cast::<Condition>()),
                &mut *output.cast::<crate::codec::Writer>(),
            )
        }
    }
    impl Native for Condition {
        fn empty() -> Self {
            Self::constant(false)
        }
        unsafe fn write(&self, output: Slot) {
            let blank = Proof::default();
            unsafe {
                write(
                    output,
                    Fields {
                        origins: contracts::address(&blank.origins).cast_mut(),
                        conditions: contracts::address(self).cast_mut(),
                        ranges: contracts::address(&blank.ranges).cast_mut(),
                        nonzero: contracts::address(&blank.nonzero).cast_mut(),
                        domains: contracts::address(&blank.domains).cast_mut(),
                        views: contracts::address(&blank.views).cast_mut(),
                        addresses: contracts::address(&blank.addresses).cast_mut(),
                    },
                    Writer {
                        origins: contracts::write_value::<Vec<Vec<ValueIdentity>>>,
                        conditions: write_one,
                        ranges: contracts::write_value::<Vec<ScalarRanges>>,
                        nonzero: contracts::write_value::<Vec<bool>>,
                        domains: contracts::write_value::<Vec<Domains>>,
                        views: write_views,
                        addresses: write_addresses,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut proof = Proof::default();
            let status = unsafe { Proof::read(context, &mut proof) };
            if status != ReadStatus::Success {
                return status;
            }
            match proof.conditions.into_iter().next().flatten() {
                Some(value) => {
                    *output = *value;
                    ReadStatus::Success
                }
                None => unsafe { contracts::fail(context, "模块资源位置缺少条件合同") },
            }
        }
    }
    impl Native for Proof {
        fn empty() -> Self {
            Self::default()
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        origins: contracts::address(&self.origins).cast_mut(),
                        conditions: contracts::address(&self.conditions).cast_mut(),
                        ranges: contracts::address(&self.ranges).cast_mut(),
                        nonzero: contracts::address(&self.nonzero).cast_mut(),
                        domains: contracts::address(&self.domains).cast_mut(),
                        views: contracts::address(&self.views).cast_mut(),
                        addresses: contracts::address(&self.addresses).cast_mut(),
                    },
                    Writer {
                        origins: contracts::write_value::<Vec<Vec<ValueIdentity>>>,
                        conditions: write_conditions,
                        ranges: contracts::write_value::<Vec<ScalarRanges>>,
                        nonzero: contracts::write_value::<Vec<bool>>,
                        domains: contracts::write_value::<Vec<Domains>>,
                        views: write_views,
                        addresses: write_addresses,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let mut origins = <Vec<Vec<ValueIdentity>> as Native>::empty();
            let mut conditions = <Vec<Option<Box<Condition>>> as Native>::empty();
            let mut ranges = <Vec<ScalarRanges> as Native>::empty();
            let mut nonzero = <Vec<bool> as Native>::empty();
            let mut domains = <Vec<Domains> as Native>::empty();
            let mut views = <Vec<view::Borrow> as Native>::empty();
            let mut addresses = <Vec<Option<address::Identity>> as Native>::empty();
            let status = unsafe {
                read(
                    context,
                    Fields {
                        origins: contracts::slot(&mut origins),
                        conditions: contracts::slot(&mut conditions),
                        ranges: contracts::slot(&mut ranges),
                        nonzero: contracts::slot(&mut nonzero),
                        domains: contracts::slot(&mut domains),
                        views: contracts::slot(&mut views),
                        addresses: contracts::slot(&mut addresses),
                    },
                    Reader {
                        origins: contracts::read_value::<Vec<Vec<ValueIdentity>>>,
                        conditions: read_conditions,
                        ranges: contracts::read_value::<Vec<ScalarRanges>>,
                        nonzero: contracts::read_value::<Vec<bool>>,
                        domains: contracts::read_value::<Vec<Domains>>,
                        views: contracts::read_value::<Vec<view::Borrow>>,
                        addresses: contracts::read_value::<Vec<Option<address::Identity>>>,
                    },
                )
            };
            if status == ReadStatus::Success {
                *output = Self {
                    origins,
                    conditions,
                    ranges,
                    nonzero,
                    domains,
                    views,
                    addresses,
                };
            }
            status
        }
    }
};

crate::codec::contracts::codec!(Condition);
