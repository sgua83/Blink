//! 同一Proof区间集合中的有限浮点次序；不拥有运行资源或另一份事实表。
use super::*;
use crate::builtin_types::ScalarBinaryOp;
use crate::ir::scalar_ranges::RangeInput;
#[path="floating/scalar.rs"]
mod scalar;
pub(super) use scalar::{order,bits,comparison_possible,interval,bitcast,cast,arithmetic,math,sum,negate};

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
    below: unsafe extern "C" fn(ArithmeticSlot, ArithmeticBorrow, ArithmeticBorrow, ArithmeticBorrow) -> bool,
    emit: unsafe extern "C" fn(ArithmeticSlot, ArithmeticBound),
}

struct ArithmeticDomains<'a, 'program> {
    builder: &'a Builder<'program>,
    inputs: [&'a Domains; 2],
    iterators: [std::collections::btree_set::Iter<'a, IndexDomain>; 2],
    result: Domains,
}

unsafe extern "C" {
    #[link_name = "bootstrap.scalar_fold::float_arithmetic_domains"]
    fn produce_arithmetic_domains(
        op: ScalarBinaryOp, left_width: u32, left: BitRange, right_width: u32, right: BitRange,
        context: ArithmeticSlot, host: ArithmeticHost,
    );
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
) -> bool {
    let state = unsafe { &*context.cast::<ArithmeticDomains<'_, '_>>() };
    unsafe {
        state.builder.integer_below(*left.cast::<ValueIdentity>(), *right.cast::<ValueIdentity>(), *scalar.cast::<Scalar>())
    }
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

pub(super) fn arithmetic_domains(builder: &Builder<'_>, op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let Some(operation @ (ScalarBinaryOp::Add | ScalarBinaryOp::Multiply | ScalarBinaryOp::Divide)) =
        scalar::arithmetic_operation(op)
    else { return Domains::new(); };
    let (ai, bi) = (component % a.regs.len(), component % b.regs.len());
    let domains = [a.proof.domains(ai), b.proof.domains(bi)];
    let inputs: [&Domains; 2] = domains.each_ref().map(|domains| domains.as_ref());
    if inputs.iter().all(|domains| domains.is_empty()) { return Domains::new(); }
    let mut state = ArithmeticDomains { builder, inputs, iterators: inputs.map(Domains::iter), result: Domains::new() };
    unsafe {
        produce_arithmetic_domains(
            operation,
            a.ty.components[ai].0.bit_width(), ranges(a, ai).hull().into(),
            b.ty.components[bi].0.bit_width(), ranges(b, bi).hull().into(),
            std::ptr::from_mut(&mut state).cast(),
            ArithmeticHost {
                rewind: rewind_arithmetic_operand, next: next_arithmetic_bound,
                same_scalar: fact_fields::same_scalar, below: arithmetic_origin_below, emit: emit_arithmetic_bound,
            },
        );
    }
    state.result
}
pub(super) fn refine(proof: &mut Proof) {
    for (component,domains) in proof.domains.iter().enumerate() {
        for domain in domains {
            if let IndexDomain::Floating(bound) = domain && bound.form == FloatingForm::Upper {
                let range = upper_range(bound.maximum());
                proof.ranges[component] = proof.ranges[component].intersection(&range);
            }
        }
    }
}
