//! Physical projections borrow original Fact fields until the synchronous consumer returns.
use super::{Clause, Comparison, Condition, Domains, Fact, IndexDomain, Operand, Proof, Scalar, ScalarPredicate, ScalarRanges, ValueIdentity, numeric_domain};
use crate::builtin_types::ScalarEncoding;
use std::{ffi::c_void, ptr};
type Borrowed = *const c_void;
type Slot = *mut c_void;

#[derive(Clone, Copy, Default)]
#[repr(u32)]
enum Kind {
    #[default]
    Equal = 1,
    Index,
    Minimum,
    Maximum,
    NonZero,
    Boolean,
    Below,
    Outside,
    Zero,
    NotEqual,
    Predicate,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub(super) struct Fields {
    present: u32,
    kind: Kind,
    first: Borrowed,
    second: Borrowed,
    payload: Borrowed,
    scalar: Borrowed,
    value: u64,
    mask: u64,
    flag: u32,
    fixed: u32,
}

impl From<&Fact> for Fields {
    fn from(fact: &Fact) -> Self {
        let mut view = Self {
            present: 1,
            ..Self::default()
        };
        match fact {
            Fact::Equal(a, b) => {
                view.kind = Kind::Equal;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
            }
            Fact::NotEqual(a, b) => {
                view.kind = Kind::NotEqual;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
            }
            Fact::Index(id, domain) => {
                view.kind = Kind::Index;
                view.first = ptr::from_ref(id).cast();
                view.payload = ptr::from_ref(domain).cast();
                if let IndexDomain::Fixed(length) = domain {
                    view.fixed = 1;
                    view.value = u64::from(*length);
                }
            }
            Fact::Outside(id, domain) => {
                view.kind = Kind::Outside;
                view.first = ptr::from_ref(id).cast();
                view.payload = ptr::from_ref(domain).cast();
            }
            Fact::Minimum(id, scalar, value) => {
                view.kind = Kind::Minimum;
                view.first = ptr::from_ref(id).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.value = *value;
                view.mask = scalar.bit_mask();
            }
            Fact::Maximum(id, scalar, value) => {
                view.kind = Kind::Maximum;
                view.first = ptr::from_ref(id).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.value = *value;
                view.mask = scalar.bit_mask();
            }
            Fact::NonZero(id) => {
                view.kind = Kind::NonZero;
                view.first = ptr::from_ref(id).cast();
            }
            Fact::Zero(id) => {
                view.kind = Kind::Zero;
                view.first = ptr::from_ref(id).cast();
            }
            Fact::Boolean(id, truth) => {
                view.kind = Kind::Boolean;
                view.first = ptr::from_ref(id).cast();
                view.flag = u32::from(*truth);
            }
            Fact::Below(a, b, strict, scalar) => {
                view.kind = Kind::Below;
                view.first = ptr::from_ref(a).cast();
                view.second = ptr::from_ref(b).cast();
                view.scalar = ptr::from_ref(scalar).cast();
                view.flag = u32::from(*strict);
            }
            Fact::Predicate(predicate, truth) => {
                view.kind = Kind::Predicate;
                view.payload = ptr::from_ref(predicate).cast();
                view.flag = u32::from(*truth);
            }
        }
        view
    }
}

// These keys only live during the native container operation that consumes them.
pub(super) unsafe fn key(view: Fields) -> Fact {
    unsafe {
        match view.kind {
            Kind::Equal => Fact::Equal(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
            ),
            Kind::NotEqual => Fact::NotEqual(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
            ),
            Kind::Index => Fact::Index(
                *view.first.cast::<ValueIdentity>(),
                *view.payload.cast::<IndexDomain>(),
            ),
            Kind::Outside => Fact::Outside(
                *view.first.cast::<ValueIdentity>(),
                *view.payload.cast::<IndexDomain>(),
            ),
            Kind::Minimum => Fact::Minimum(
                *view.first.cast::<ValueIdentity>(),
                *view.scalar.cast::<Scalar>(),
                view.value,
            ),
            Kind::Maximum => Fact::Maximum(
                *view.first.cast::<ValueIdentity>(),
                *view.scalar.cast::<Scalar>(),
                view.value,
            ),
            Kind::NonZero => Fact::NonZero(*view.first.cast::<ValueIdentity>()),
            Kind::Zero => Fact::Zero(*view.first.cast::<ValueIdentity>()),
            Kind::Boolean => Fact::Boolean(*view.first.cast::<ValueIdentity>(), view.flag != 0),
            Kind::Below => Fact::Below(
                *view.first.cast::<ValueIdentity>(),
                *view.second.cast::<ValueIdentity>(),
                view.flag != 0,
                *view.scalar.cast::<Scalar>(),
            ),
            Kind::Predicate => {
                Fact::Predicate(*view.payload.cast::<ScalarPredicate>(), view.flag != 0)
            }
        }
    }
}

pub(super) unsafe extern "C" fn compare(a: Borrowed, b: Borrowed) -> i32 {
    unsafe { (&*a.cast::<ValueIdentity>()).cmp(&*b.cast::<ValueIdentity>()) as i32 }
}

pub(super) unsafe extern "C" fn project(fact: Borrowed) -> Fields {
    Fields::from(unsafe { &*fact.cast::<Fact>() })
}

#[repr(C)]
pub(super) struct Host {
    pub(super) unsigned32: Borrowed,
    pub(super) compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
#[repr(C)]
pub(super) struct ScalarView {
    kind: u32,
    width: u32,
    mask: u64,
    bias: u64,
}
pub(super) unsafe extern "C" fn scalar(value: Borrowed) -> ScalarView {
    let value = unsafe { *value.cast::<Scalar>() };
    ScalarView {
        kind: if value.is_float() {
            2
        } else if value.is_signed() {
            1
        } else if value == Scalar::Bool {
            3
        } else {
            0
        },
        width: value.bit_width(),
        mask: value.bit_mask(),
        bias: value.order_bias(),
    }
}
pub(super) unsafe extern "C" fn same_scalar(a: Borrowed, b: Borrowed) -> u32 {
    u32::from(unsafe { *a.cast::<Scalar>() == *b.cast::<Scalar>() })
}

pub(super) unsafe extern "C" fn compare_scalars(a: Borrowed, b: Borrowed) -> i32 {
    unsafe { (&*a.cast::<Scalar>()).cmp(&*b.cast::<Scalar>()) as i32 }
}

#[repr(C)]
pub(super) struct UpperView {
    identity: Borrowed,
    distance: u32,
    present: u32,
}
pub(super) unsafe extern "C" fn upper(value: Borrowed) -> UpperView {
    match unsafe { &*value.cast::<IndexDomain>() } {
        IndexDomain::Upper(id, distance) => UpperView {
            identity: ptr::from_ref(id).cast(),
            distance: *distance,
            present: 1,
        },
        _ => UpperView {
            identity: ptr::null(),
            distance: 0,
            present: 0,
        },
    }
}

pub(super) fn bound(id: &ValueIdentity, scalar: &Scalar, value: u64, minimum: bool) -> Fields {
    Fields {
        present: 1,
        kind: if minimum {
            Kind::Minimum
        } else {
            Kind::Maximum
        },
        first: ptr::from_ref(id).cast(),
        scalar: ptr::from_ref(scalar).cast(),
        value,
        mask: scalar.bit_mask(),
        ..Fields::default()
    }
}

#[derive(Default)]
#[repr(C)]
pub(super) struct OperandView {
    identity: Borrowed,
    bits: u64,
    scalar: Borrowed,
    is_constant: u32,
}
pub(super) fn operand(value: &Operand) -> OperandView {
    match value {
        Operand::Value(id) => OperandView {
            identity: ptr::from_ref(id).cast(),
            bits: 0,
            scalar: ptr::null(),
            is_constant: 0,
        },
        Operand::Constant(scalar, bits) => OperandView {
            identity: ptr::null(),
            bits: *bits,
            scalar: ptr::from_ref(scalar).cast(),
            is_constant: 1,
        },
    }
}
#[repr(C)]
pub(super) struct PredicateView {
    scalar: Borrowed,
    operation: u32,
    left: OperandView,
    right: OperandView,
}
pub(super) unsafe extern "C" fn predicate(value: Borrowed) -> PredicateView {
    let value = unsafe { &*value.cast::<ScalarPredicate>() };
    let operation = match value.op {
        Comparison::Equal => 10,
        Comparison::NotEqual => 11,
        Comparison::Less => 12,
        Comparison::Greater => 13,
        Comparison::LessEqual => 14,
        Comparison::GreaterEqual => 15,
    };
    PredicateView {
        scalar: ptr::from_ref(&value.scalar).cast(),
        operation,
        left: operand(&value.a),
        right: operand(&value.b),
    }
}

#[repr(C)]
pub(super) struct DomainView {
    kind: u32,
    identity: Borrowed,
    count: u32,
    distance: u32,
}
pub(super) unsafe extern "C" fn domain(value: Borrowed) -> DomainView {
    let mut view = DomainView {
        kind: 0,
        identity: ptr::null(),
        count: 0,
        distance: 0,
    };
    match unsafe { &*value.cast::<IndexDomain>() } {
        IndexDomain::Fixed(count) => {
            view.kind = 1;
            view.count = *count;
        }
        IndexDomain::Upper(id, distance) => {
            view.kind = 2;
            view.identity = ptr::from_ref(id).cast();
            view.distance = *distance;
        }
        IndexDomain::ApplicationData(id) => {
            view.kind = 3;
            view.identity = ptr::from_ref(id).cast();
        }
        IndexDomain::ApplicationLength(id) => {
            view.kind = 4;
            view.identity = ptr::from_ref(id).cast();
        }
        _ => {}
    }
    view
}

pub(super) unsafe fn operand_key(view: OperandView) -> Operand {
    if view.is_constant != 0 {
        Operand::Constant(unsafe { *view.scalar.cast::<Scalar>() }, view.bits)
    } else {
        Operand::Value(unsafe { *view.identity.cast::<ValueIdentity>() })
    }
}
#[derive(Default)]
#[repr(C)]
pub(super) struct ExpressionView {
    present: u32,
    operation: u32,
    scalar: Borrowed,
    left: OperandView,
    right: OperandView,
    definition: Borrowed,
}
pub(super) fn expression(value: &super::IntegerExpression) -> ExpressionView {
    use super::{IntegerExpression, IntegerOperation};
    let IntegerExpression::Binary(op, scalar, left, right) = value else {
        return ExpressionView {
            present: 1,
            operation: 10,
            scalar: ptr::from_ref(&Scalar::U32).cast(),
            definition: ptr::from_ref(value).cast(),
            ..Default::default()
        };
    };
    let operation = match op {
        IntegerOperation::Add => 0,
        IntegerOperation::Subtract => 1,
        IntegerOperation::Multiply => 2,
        IntegerOperation::Divide => 3,
        IntegerOperation::Remainder => 4,
        IntegerOperation::And => 5,
        IntegerOperation::Or => 6,
        IntegerOperation::Xor => 7,
        IntegerOperation::ShiftLeft => 8,
        IntegerOperation::ShiftRight => 9,
    };
    ExpressionView {
        present: 1,
        operation,
        scalar: ptr::from_ref(scalar).cast(),
        left: operand(left),
        right: operand(right),
        definition: ptr::from_ref(value).cast(),
    }
}
pub(super) unsafe extern "C" fn computed(value: Borrowed) -> ExpressionView {
    match unsafe { &*value.cast::<IndexDomain>() } {
        IndexDomain::Computed(value) => expression(value),
        _ => ExpressionView::default(),
    }
}

// Both value and comparison conditions borrow this same original domain field.
#[derive(Default)]
#[repr(C)]
pub(super) struct ConditionDomain {present:u32,common:u32,original:Borrowed}
impl From<&IndexDomain> for ConditionDomain {
    fn from(domain:&IndexDomain)->Self {
        Self{present:1,common:u32::from(matches!(domain,IndexDomain::CommonRange(..))),original:ptr::from_ref(domain).cast()}
    }
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum ProofDomainKind {
    Other,
    FixedLength,
    ResourceLength,
    KnownBits,
    Numeric,
}

#[repr(C)]
struct ProofDomainView {
    kind: ProofDomainKind,
    value: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ProofDomainHost {
    describe: unsafe extern "C" fn(Borrowed) -> ProofDomainView,
    nonzero: unsafe extern "C" fn(Slot),
    bounded: unsafe extern "C" fn(Slot, u64),
    retain: unsafe extern "C" fn(Slot, Borrowed),
}

#[repr(C)]
struct ProofRefinementHost {
    domains: ProofDomainHost,
    scalar: unsafe extern "C" fn(Borrowed) -> ScalarEncoding,
    deny: unsafe extern "C" fn(Slot, u32),
    range: unsafe extern "C" fn(Slot, u64, u64),
}

struct Refinement<'a> {
    proof: &'a mut Proof,
    component: usize,
}

impl Refinement<'_> {
    fn range(&mut self) -> &mut ScalarRanges {
        let ranges = &mut self.proof.ranges;
        ranges.resize_with(ranges.len().max(self.component + 1), ScalarRanges::default);
        &mut ranges[self.component]
    }
}

unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::bind_proof_domain"]
    fn b_bind_proof_domain(context: Slot, domain: Borrowed, host: ProofDomainHost);
    #[link_name = "bootstrap.condition_graph::refine_proof_fact"]
    fn b_refine_proof_fact(
        context: Slot, scalar: ScalarEncoding, zero: u64, fact: Fields, host: ProofRefinementHost,
    );
}

unsafe extern "C" fn describe_domain(value: Borrowed) -> ProofDomainView {
    let value = unsafe { &*value.cast::<IndexDomain>() };
    let (kind, value) = match value {
        IndexDomain::Fixed(count) => (ProofDomainKind::FixedLength, u64::from(*count)),
        IndexDomain::Data(_) | IndexDomain::Tile | IndexDomain::View(_)
        | IndexDomain::ApplicationData(_) => (ProofDomainKind::ResourceLength, 0),
        IndexDomain::KnownBits(_, _, bits) => (ProofDomainKind::KnownBits, *bits),
        domain if numeric_domain(domain) => (ProofDomainKind::Numeric, 0),
        _ => (ProofDomainKind::Other, 0),
    };
    ProofDomainView { kind, value }
}

unsafe extern "C" fn mark_nonzero(context: Slot) {
    let state = unsafe { &mut *context.cast::<Refinement<'_>>() };
    state.proof.mark_nonzero(state.component);
}

unsafe extern "C" fn bounded_domain(context: Slot, limit: u64) {
    let state = unsafe { &mut *context.cast::<Refinement<'_>>() };
    let range = state.range();
    *range = range.restrict(0, limit).without(limit);
}

unsafe extern "C" fn retain_domain(context: Slot, domain: Borrowed) {
    let state = unsafe { &mut *context.cast::<Refinement<'_>>() };
    let domains = &mut state.proof.domains;
    domains.resize_with(domains.len().max(state.component + 1), Domains::new);
    domains[state.component].insert(unsafe { *domain.cast::<IndexDomain>() });
}

unsafe extern "C" fn refinement_scalar(scalar: Borrowed) -> ScalarEncoding {
    unsafe { (*scalar.cast::<Scalar>()).into() }
}

unsafe extern "C" fn deny_condition(context: Slot, branch: u32) {
    let state = unsafe { &mut *context.cast::<Refinement<'_>>() };
    let conditions = &mut state.proof.conditions;
    conditions.resize_with(conditions.len().max(state.component + 1), || None);
    let condition = conditions[state.component].get_or_insert_with(|| Box::new(Condition::default()));
    condition.cases[branch as usize] = Clause::impossible();
}

unsafe extern "C" fn restrict_range(context: Slot, lower: u64, upper: u64) {
    let state = unsafe { &mut *context.cast::<Refinement<'_>>() };
    let range = state.range();
    *range = range.restrict(lower, upper);
}

const PROOF_DOMAINS: ProofDomainHost = ProofDomainHost {
    describe: describe_domain,
    nonzero: mark_nonzero,
    bounded: bounded_domain,
    retain: retain_domain,
};

pub(super) fn bind_domain(proof: &mut Proof, component: usize, domain: &IndexDomain) {
    let mut state = Refinement { proof, component };
    unsafe { b_bind_proof_domain(ptr::from_mut(&mut state).cast(), ptr::from_ref(domain).cast(), PROOF_DOMAINS) };
}

pub(super) fn refine(proof: &mut Proof, component: usize, scalar: Scalar, fact: &Fact) {
    let mut state = Refinement { proof, component };
    unsafe {
        b_refine_proof_fact(
            ptr::from_mut(&mut state).cast(), scalar.into(), scalar.order_bias(), Fields::from(fact),
            ProofRefinementHost { domains: PROOF_DOMAINS, scalar: refinement_scalar,
                deny: deny_condition, range: restrict_range },
        );
    }
}

