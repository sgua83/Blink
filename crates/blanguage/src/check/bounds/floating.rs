//! 同一Proof区间集合中的有限浮点次序；不拥有运行资源或另一份事实表。
use super::*;
use crate::ir::scalar_ranges::RangeInput;
#[path="floating/scalar.rs"]
mod scalar;
pub(super) use scalar::{order,bits,comparison_possible,interval,bitcast,cast,math,sum,negate};

unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::float_upper_maximum"]
    fn float_upper_maximum(inputs: BitRange, scale: f64, offset: f64, reciprocal: f64) -> f64;
    #[link_name = "bootstrap.scalar_fold::float_ranges_require_preserve"]
    fn ranges_require_preserve(width: u32, values: RangeInput, clip: BitRange) -> bool;
}

fn requires_float_preserve(
    scalar: Scalar, ranges: &[BitRange], clip: BitRange, domains: Option<&Domains>,
) -> bool {
    // An unsigned integer conversion to f32 can only produce +0 or a normal value.
    if scalar == Scalar::F32 && domains.into_iter().flatten().any(|domain|
        matches!(domain, IndexDomain::Floating(FloatingBound { form: FloatingForm::Cast, .. }))) {
        return false;
    }
    unsafe {
        ranges_require_preserve(scalar.bit_width(), RangeInput::records(ranges, range_sets::read_record), clip)
    }
}

impl Value {
    pub(in crate::check) fn requires_float_preserve(&self, component: usize) -> bool {
        let component = component % self.regs.len();
        let scalar = self.ty.components[component].0;
        let maximum = scalar.bit_mask();
        let clip = self.constant.as_ref().map_or(BitRange { lower: 0, upper: maximum }, |values| {
            let at = order(scalar, values[component]);
            BitRange { lower: at, upper: at.min(maximum) }
        });
        requires_float_preserve(scalar, self.proof.interval_source(component), clip, self.proof.domains.get(component))
    }
}

impl Proof {
    pub(in crate::check) fn binary_requires_float_preserve(
        &self, scalar: Scalar, component: usize, a: &Value, b: &Value, comparison: bool,
    ) -> bool {
        a.requires_float_preserve(component) || b.requires_float_preserve(component)
            || (!comparison && requires_float_preserve(scalar, self.interval_source(component),
                FULL_INTERVALS[0], self.domains.get(component)))
    }
}

pub(super) fn ranges(value: &Value, component: usize) -> ScalarRanges {
    let scalar = value.ty.components[component].0;
    let ranges = value.proof.range_set(component).restrict(0, scalar.bit_mask());
    match &value.constant {
        Some(values) => { let at = order(scalar, values[component]); ranges.restrict(at, at) }
        None => ranges,
    }
}

impl Proof {
    pub(in crate::check) fn texture_sample(
        format: Option<crate::ast::TextureFormat>, coordinate: &Value, lod: &Value,
    ) -> Self {
        let Some(range) = scalar::texture_sample(format, coordinate, lod) else {
            return Self::default();
        };
        Self { ranges: vec![range; 4], ..Self::default() }
    }
}

impl FloatingBound {
    fn coefficients(self) -> [f64; 3] { self.terms.map(f64::from_bits) }
    fn maximum(self) -> f64 {
        let inputs = BitRange { lower: self.lower, upper: self.upper };
        let [scale, offset, reciprocal] = self.coefficients();
        unsafe { float_upper_maximum(inputs, scale, offset, reciprocal) }
    }
    fn restricted(self, input: CallInput<'_>) -> Option<Self> {
        let (lower,upper) = range_sets::restricted_hull(input.intervals(), (0, self.scalar.bit_mask()), None);
        let bound = Self { lower: self.lower.max(lower), upper: self.upper.min(upper), ..self };
        (bound.lower <= bound.upper).then_some(bound)
    }
    pub(super) fn at_call(self, bindings: &impl CallBindings) -> Option<Self> {
        let input = bindings.input(self.source)?;
        Some(Self { source: input.identity()?, ..self.restricted(input)? })
    }
    pub(super) fn project_at_call(
        self, bindings: &impl CallBindings,
    ) -> Option<(Option<Self>, Option<ScalarRanges>)> {
        let input = bindings.input(self.source)?;
        let bound = self.restricted(input)?;
        let mapped = input.identity().map(|source| Self { source, ..bound });
        let range = (bound.form == FloatingForm::Upper).then(|| upper_range(bound.maximum()));
        Some((mapped, range))
    }
}
fn upper_range(upper: f64) -> ScalarRanges {
    interval(Scalar::F32,-(f32::MAX as f64),upper)
}
type ArithmeticSlot = *mut std::ffi::c_void;
type ArithmeticBorrow = *const std::ffi::c_void;

#[derive(Default)]
#[repr(C)]
struct ArithmeticBound {
    origin: ArithmeticBorrow,
    scalar: ArithmeticBorrow,
    range: BitRange,
    scale: f64,
    offset: f64,
    reciprocal: f64,
    form: u32,
}

#[repr(C)]
struct ArithmeticHost {
    rewind: unsafe extern "C" fn(ArithmeticSlot, u32),
    next: unsafe extern "C" fn(ArithmeticSlot, u32) -> ArithmeticBound,
    same_scalar: unsafe extern "C" fn(ArithmeticBorrow, ArithmeticBorrow) -> u32,
    below: unsafe extern "C" fn(ArithmeticSlot, ArithmeticBorrow, ArithmeticBorrow, ArithmeticBorrow) -> u32,
    emit: unsafe extern "C" fn(ArithmeticSlot, ArithmeticBound),
}

struct ArithmeticDomains<'a, 'program> {
    builder: &'a Builder<'program>,
    inputs: [&'a Domains; 2],
    iterators: [std::collections::btree_set::Iter<'a, IndexDomain>; 2],
    result: Domains,
}

unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::float_arithmetic_proof"]
    fn produce_arithmetic(
        op: u32, width: u32, left: BitRange, right: BitRange,
        with_domains: u32, context: ArithmeticSlot, host: ArithmeticHost,
    ) -> scalar::NumericalRange;
}

unsafe extern "C" fn rewind_arithmetic_operand(context: ArithmeticSlot, side: u32) {
    let state = unsafe { &mut *context.cast::<ArithmeticDomains<'_, '_>>() };
    state.iterators[side as usize] = state.inputs[side as usize].iter();
}

unsafe extern "C" fn next_arithmetic_bound(context: ArithmeticSlot, side: u32) -> ArithmeticBound {
    let state = unsafe { &mut *context.cast::<ArithmeticDomains<'_, '_>>() };
    state.iterators[side as usize].find_map(|domain| {
        let IndexDomain::Floating(bound) = domain else { return None; };
        let [scale, offset, reciprocal] = bound.coefficients();
        Some(ArithmeticBound {
            origin: std::ptr::from_ref(&bound.source).cast(),
            scalar: std::ptr::from_ref(&bound.scalar).cast(),
            range: BitRange { lower: bound.lower, upper: bound.upper },
            scale, offset, reciprocal,
            form: u32::from(bound.form == FloatingForm::Upper) + 1,
        })
    }).unwrap_or_default()
}

unsafe extern "C" fn arithmetic_origin_below(
    context: ArithmeticSlot, left: ArithmeticBorrow, right: ArithmeticBorrow, scalar: ArithmeticBorrow,
) -> u32 {
    let state = unsafe { &*context.cast::<ArithmeticDomains<'_, '_>>() };
    u32::from(unsafe {
        state.builder.integer_below(*left.cast::<ValueIdentity>(), *right.cast::<ValueIdentity>(), *scalar.cast::<Scalar>())
    })
}

unsafe extern "C" fn emit_arithmetic_bound(context: ArithmeticSlot, value: ArithmeticBound) {
    let state = unsafe { &mut *context.cast::<ArithmeticDomains<'_, '_>>() };
    let bound = FloatingBound {
        source: unsafe { *value.origin.cast::<ValueIdentity>() },
        scalar: unsafe { *value.scalar.cast::<Scalar>() },
        lower: value.range.lower,
        upper: value.range.upper,
        terms: [value.scale, value.offset, value.reciprocal].map(f64::to_bits),
        form: FloatingForm::Upper,
    };
    state.result.insert(IndexDomain::Floating(bound));
}

pub(super) fn arithmetic(
    builder: &Builder<'_>, op: &str, a: &Value, b: &Value, count: usize, scalar: Scalar,
) -> Proof {
    let tracks_domains = scalar == Scalar::F32;
    let Some(operation) = scalar::arithmetic_operation(op) else {
        let mut proof = Proof::default();
        proof.ranges.resize_with(count, ScalarRanges::default);
        if tracks_domains { proof.domains.resize_with(count, Domains::new); }
        return proof;
    };
    let operation = operation as u32;
    let width = scalar.bit_width();
    let mut proof = Proof {
        ranges: Vec::with_capacity(count),
        domains: Vec::with_capacity(if tracks_domains { count } else { 0 }),
        ..Proof::default()
    };
    let empty = Domains::new();
    for component in 0..count {
        let (ai, bi) = (component % a.regs.len(), component % b.regs.len());
        let inputs = [a.proof.domains.get(ai).unwrap_or(&empty), b.proof.domains.get(bi).unwrap_or(&empty)];
        let with_domains = tracks_domains && inputs.iter().any(|domains| !domains.is_empty());
        let mut state = ArithmeticDomains { builder, inputs, iterators: inputs.map(Domains::iter), result: Domains::new() };
        let range = unsafe {
            produce_arithmetic(
                operation, width, ranges(a, ai).hull().into(), ranges(b, bi).hull().into(),
                u32::from(with_domains), std::ptr::from_mut(&mut state).cast(),
                ArithmeticHost {
                    rewind: rewind_arithmetic_operand, next: next_arithmetic_bound,
                    same_scalar: fact_fields::same_scalar, below: arithmetic_origin_below, emit: emit_arithmetic_bound,
                },
            )
        };
        proof.ranges.push(scalar::decoded(range));
        if tracks_domains { proof.domains.push(state.result); }
    }
    proof
}
