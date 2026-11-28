//! B controls integer relation inference. Original typed inputs, coefficients,
//! active equations and final facts remain owned by IntegerQuery and its caller.
use super::facts;
use super::*;
use std::{ffi::c_void, mem::MaybeUninit, ptr};
type Slot = *mut c_void;
type Borrowed = *const c_void;
fn borrow<T>(v: &T) -> Borrowed {
    ptr::from_ref(v).cast()
}
unsafe fn frame<'a, T>(s: Slot) -> &'a mut T {
    unsafe { &mut *s.cast() }
}
#[derive(Clone, Copy)]
#[repr(C)]
struct Wide {
    low: u64,
    high: u64,
}
impl From<i128> for Wide {
    fn from(v: i128) -> Self {
        let v = v as u128;
        Self {
            low: v as u64,
            high: (v >> 64) as u64,
        }
    }
}
impl From<Wide> for i128 {
    fn from(v: Wide) -> Self {
        (u128::from(v.low) | (u128::from(v.high) << 64)) as i128
    }
}
#[repr(C)]
struct Affine {
    present: u32,
    source: Borrowed,
    scale: Wide,
    offset: Wide,
}
impl From<&IntegerAffine> for Affine {
    fn from(v: &IntegerAffine) -> Self {
        Self {
            present: 1,
            source: v.source.as_ref().map(borrow).unwrap_or(ptr::null()),
            scale: v.scale.into(),
            offset: v.offset.into(),
        }
    }
}
impl Affine {
    fn absent() -> Self {
        Self {
            present: 0,
            source: ptr::null(),
            scale: 0.into(),
            offset: 0.into(),
        }
    }
}
unsafe fn owned(v: Affine) -> IntegerAffine {
    IntegerAffine {
        source: if v.source.is_null() {
            None
        } else {
            Some(unsafe { *v.source.cast::<ValueIdentity>() })
        },
        scale: v.scale.into(),
        offset: v.offset.into(),
    }
}
#[repr(C)]
struct Range {
    present: u32,
    lower: Wide,
    upper: Wide,
}
impl From<(i128, i128)> for Range {
    fn from((a, b): (i128, i128)) -> Self {
        Self {
            present: 1,
            lower: a.into(),
            upper: b.into(),
        }
    }
}
fn range(value: Option<(i128, i128)>) -> Range {
    value.map(Range::from).unwrap_or(Range {
        present: 0,
        lower: 0.into(),
        upper: 0.into(),
    })
}
fn decoded(v: Range) -> Option<(i128, i128)> {
    (v.present != 0).then(|| (v.lower.into(), v.upper.into()))
}
unsafe fn bounds(q: *const IntegerQuery<'_>, scalar: Scalar, id: Borrowed) -> Range {
    unsafe { (&*q).bounds(*id.cast::<ValueIdentity>(), scalar) }.into()
}

struct Interval<'a> {
    query: *const IntegerQuery<'a>,
    scalar: Scalar,
}
unsafe extern "C" fn interval_bounds(s: Slot, id: Borrowed) -> Range {
    let c = unsafe { frame::<Interval<'_>>(s) };
    unsafe { bounds(c.query, c.scalar, id) }
}
#[repr(C)]
struct IntervalHost {
    bounds: unsafe extern "C" fn(Slot, Borrowed) -> Range,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_interval"]
    fn b_interval(context: Slot, value: Affine, host: IntervalHost) -> Range;
}
pub(super) fn interval(
    query: &IntegerQuery<'_>,
    value: IntegerAffine,
    scalar: Scalar,
) -> Option<(i128, i128)> {
    let mut c = Interval { query, scalar };
    decoded(unsafe {
        b_interval(
            (&mut c as *mut Interval<'_>).cast(),
            Affine::from(&value),
            IntervalHost {
                bounds: interval_bounds,
            },
        )
    })
}

struct OperandFrame<'a> {
    query: *mut IntegerQuery<'a>,
    scalar: Scalar,
    cached: *mut IntegerAffine,
    result: *mut IntegerAffine,
    root: *mut ValueIdentity,
    input: Option<facts::Iter<'a>>,
}
unsafe extern "C" fn operand_cache(s: Slot, id: Borrowed) -> Affine {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    let key = (unsafe { *id.cast::<ValueIdentity>() }, c.scalar);
    match unsafe { &*c.query }.coefficients.get(&key).copied() {
        Some(value) => {
            unsafe { c.cached.write(value) };
            Affine::from(unsafe { &*c.cached })
        }
        None => Affine::absent(),
    }
}
unsafe extern "C" fn operand_root(s: Slot, id: Borrowed) -> Borrowed {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    let value = unsafe { &*c.query }.root(unsafe { *id.cast::<ValueIdentity>() });
    unsafe { c.root.write(value) };
    c.root.cast()
}
unsafe extern "C" fn operand_enter(s: Slot, id: Borrowed) -> u32 {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    u32::from(
        unsafe { &mut *c.query }
            .expanding
            .insert((unsafe { *id.cast::<ValueIdentity>() }, c.scalar)),
    )
}
unsafe extern "C" fn operand_bounds(s: Slot, id: Borrowed) -> Range {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    unsafe { bounds(c.query, c.scalar, id) }
}
unsafe extern "C" fn operand_begin(s: Slot, id: Borrowed) {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    let facts = unsafe { &*c.query }.facts;
    c.input = Some(
        facts.range(Fact::Index(unsafe { *id.cast::<ValueIdentity>() }, IndexDomain::Data(0))..),
    );
}
unsafe extern "C" fn operand_next(s: Slot) -> fact_fields::Fields {
    unsafe { frame::<OperandFrame<'_>>(s) }
        .input
        .as_mut()
        .and_then(Iterator::next)
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn operand_expression(s: Slot, definition: Borrowed) -> Affine {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    let expression = unsafe { *definition.cast::<IntegerExpression>() };
    match unsafe { &mut *c.query }.expression(expression, c.scalar) {
        Some(value) => {
            unsafe { c.result.write(value) };
            Affine::from(unsafe { &*c.result })
        }
        None => Affine::absent(),
    }
}
unsafe extern "C" fn operand_end(s: Slot) {
    unsafe { frame::<OperandFrame<'_>>(s) }.input = None;
}
unsafe extern "C" fn operand_leave(s: Slot, id: Borrowed) {
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    unsafe { &mut *c.query }
        .expanding
        .remove(&(unsafe { *id.cast::<ValueIdentity>() }, c.scalar));
}
unsafe extern "C" fn operand_remember(s: Slot, id: Borrowed, value: Affine) {
    let value = unsafe { owned(value) };
    let c = unsafe { frame::<OperandFrame<'_>>(s) };
    unsafe { &mut *c.query }
        .coefficients
        .insert((unsafe { *id.cast::<ValueIdentity>() }, c.scalar), value);
}
#[repr(C)]
struct OperandHost {
    cache: unsafe extern "C" fn(Slot, Borrowed) -> Affine,
    root: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    enter: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    bounds: unsafe extern "C" fn(Slot, Borrowed) -> Range,
    begin: unsafe extern "C" fn(Slot, Borrowed),
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    computed: unsafe extern "C" fn(Borrowed) -> fact_fields::ExpressionView,
    expression: unsafe extern "C" fn(Slot, Borrowed) -> Affine,
    end: unsafe extern "C" fn(Slot),
    leave: unsafe extern "C" fn(Slot, Borrowed),
    remember: unsafe extern "C" fn(Slot, Borrowed, Affine),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_operand"]
    fn b_operand(
        context: Slot,
        operand: fact_fields::OperandView,
        info: fact_fields::ScalarView,
        host: OperandHost,
    ) -> Affine;
}
pub(super) fn operand(
    query: &mut IntegerQuery<'_>,
    operand: Operand,
    scalar: Scalar,
) -> IntegerAffine {
    let mut cached = MaybeUninit::<IntegerAffine>::uninit();
    let mut result = MaybeUninit::<IntegerAffine>::uninit();
    let mut root = MaybeUninit::<ValueIdentity>::uninit();
    let mut c = OperandFrame {
        query,
        scalar,
        cached: cached.as_mut_ptr(),
        result: result.as_mut_ptr(),
        root: root.as_mut_ptr(),
        input: None,
    };
    let value = unsafe {
        b_operand(
            (&mut c as *mut OperandFrame<'_>).cast(),
            fact_fields::operand(&operand),
            fact_fields::scalar(borrow(&scalar)),
            OperandHost {
                cache: operand_cache,
                root: operand_root,
                enter: operand_enter,
                bounds: operand_bounds,
                begin: operand_begin,
                next: operand_next,
                computed: fact_fields::computed,
                expression: operand_expression,
                end: operand_end,
                leave: operand_leave,
                remember: operand_remember,
                compare: fact_fields::compare,
            },
        )
    };
    unsafe { owned(value) }
}

struct ExpressionFrame<'a> {
    query: *mut IntegerQuery<'a>,
    scalar: Scalar,
    operands: [Operand; 2],
    values: [*mut IntegerAffine; 2],
}
unsafe extern "C" fn expression_operand(s: Slot, side: u32) -> Affine {
    let c = unsafe { frame::<ExpressionFrame<'_>>(s) };
    let value = unsafe { &mut *c.query }.operand(c.operands[side as usize], c.scalar);
    let out = c.values[side as usize];
    unsafe { out.write(value) };
    Affine::from(unsafe { &*out })
}
unsafe extern "C" fn expression_interval(s: Slot, value: Affine) -> Range {
    let value = unsafe { owned(value) };
    let c = unsafe { frame::<ExpressionFrame<'_>>(s) };
    range(unsafe { &*c.query }.interval(value, c.scalar))
}
#[repr(C)]
struct ExpressionHost {
    operand: unsafe extern "C" fn(Slot, u32) -> Affine,
    interval: unsafe extern "C" fn(Slot, Affine) -> Range,
    same_scalar: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_expression"]
    fn b_expression(
        context: Slot,
        expression: fact_fields::ExpressionView,
        scalar: Borrowed,
        info: fact_fields::ScalarView,
        host: ExpressionHost,
    ) -> Affine;
}
pub(super) fn expression(
    query: &mut IntegerQuery<'_>,
    expression: IntegerExpression,
    scalar: Scalar,
) -> Option<IntegerAffine> {
    let operands = match expression {
        IntegerExpression::Binary(_, _, a, b) => [a, b],
        IntegerExpression::Selection(index) => {
            if scalar != Scalar::U32 { return None; }
            let value = index.0.affine(query)?;
            let (lower, upper) = query.interval(value, scalar)?;
            return (lower >= 0 && upper <= i128::from(u32::MAX)).then_some(value);
        }
    };
    let mut a = MaybeUninit::<IntegerAffine>::uninit();
    let mut b = MaybeUninit::<IntegerAffine>::uninit();
    let mut c = ExpressionFrame {
        query,
        scalar,
        operands,
        values: [a.as_mut_ptr(), b.as_mut_ptr()],
    };
    let value = unsafe {
        b_expression(
            (&mut c as *mut ExpressionFrame<'_>).cast(),
            fact_fields::expression(&expression),
            borrow(&scalar),
            fact_fields::scalar(borrow(&scalar)),
            ExpressionHost {
                operand: expression_operand,
                interval: expression_interval,
                same_scalar: fact_fields::same_scalar,
                compare: fact_fields::compare,
            },
        )
    };
    if value.present != 0 {
        Some(unsafe { owned(value) })
    } else {
        None
    }
}

struct Retain<'a> {
    query: *const IntegerQuery<'a>,
    output: *mut Facts,
    id: ValueIdentity,
    scalar: Scalar,
}
unsafe extern "C" fn retain_bounds(s: Slot) -> Range {
    let c = unsafe { frame::<Retain<'_>>(s) };
    unsafe { &*c.query }.bounds(c.id, c.scalar).into()
}
unsafe extern "C" fn retain_emit(s: Slot, minimum: u32, value: u64) {
    let c = unsafe { frame::<Retain<'_>>(s) };
    let fact = if minimum != 0 {
        Fact::Minimum(c.id, c.scalar, value)
    } else {
        Fact::Maximum(c.id, c.scalar, value)
    };
    unsafe { &mut *c.output }.insert(fact);
}
#[repr(C)]
struct RetainHost {
    bounds: unsafe extern "C" fn(Slot) -> Range,
    emit: unsafe extern "C" fn(Slot, u32, u64),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_retain"]
    fn b_retain(
        context: Slot,
        lower: Wide,
        upper: Wide,
        info: fact_fields::ScalarView,
        host: RetainHost,
    ) -> u32;
}
pub(super) fn retain(
    query: &IntegerQuery<'_>,
    output: &mut Facts,
    id: ValueIdentity,
    scalar: Scalar,
    lower: i128,
    upper: i128,
) -> Option<()> {
    let mut c = Retain {
        query,
        output,
        id: query.root(id),
        scalar,
    };
    let result = unsafe {
        b_retain(
            (&mut c as *mut Retain<'_>).cast(),
            lower.into(),
            upper.into(),
            fact_fields::scalar(borrow(&scalar)),
            RetainHost {
                bounds: retain_bounds,
                emit: retain_emit,
            },
        )
    };
    (result != 0).then_some(())
}

struct Quotient<'a> {
    query: *const IntegerQuery<'a>,
    output: *mut Facts,
    scalar: Scalar,
}
unsafe extern "C" fn quotient_bounds(s: Slot, id: Borrowed) -> Range {
    let c = unsafe { frame::<Quotient<'_>>(s) };
    unsafe { bounds(c.query, c.scalar, id) }
}
unsafe extern "C" fn quotient_operand_bounds(s: Slot, operand: fact_fields::OperandView) -> Range {
    let operand = unsafe { fact_fields::operand_key(operand) };
    let c = unsafe { frame::<Quotient<'_>>(s) };
    unsafe { &*c.query }
        .operand_bounds(operand, c.scalar)
        .into()
}
unsafe extern "C" fn quotient_retain(s: Slot, id: Borrowed, lower: Wide, upper: Wide) -> u32 {
    let c = unsafe { frame::<Quotient<'_>>(s) };
    u32::from(
        unsafe { &*c.query }
            .retain(
                unsafe { &mut *c.output },
                unsafe { *id.cast::<ValueIdentity>() },
                c.scalar,
                lower.into(),
                upper.into(),
            )
            .is_some(),
    )
}
#[repr(C)]
struct QuotientHost {
    bounds: unsafe extern "C" fn(Slot, Borrowed) -> Range,
    operand_bounds: unsafe extern "C" fn(Slot, fact_fields::OperandView) -> Range,
    retain: unsafe extern "C" fn(Slot, Borrowed, Wide, Wide) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_quotient"]
    fn b_quotient(
        context: Slot,
        id: Borrowed,
        numerator: fact_fields::OperandView,
        divisor: fact_fields::OperandView,
        info: fact_fields::ScalarView,
        host: QuotientHost,
    ) -> u32;
}
pub(super) fn quotient(
    query: &IntegerQuery<'_>,
    output: &mut Facts,
    id: ValueIdentity,
    scalar: Scalar,
    numerator: Operand,
    divisor: Operand,
) -> Option<()> {
    let mut c = Quotient {
        query,
        output,
        scalar,
    };
    let result = unsafe {
        b_quotient(
            (&mut c as *mut Quotient<'_>).cast(),
            borrow(&id),
            fact_fields::operand(&numerator),
            fact_fields::operand(&divisor),
            fact_fields::scalar(borrow(&scalar)),
            QuotientHost {
                bounds: quotient_bounds,
                operand_bounds: quotient_operand_bounds,
                retain: quotient_retain,
            },
        )
    };
    (result != 0).then_some(())
}
#[repr(C)]
struct AtMostHost {
    retain: unsafe extern "C" fn(Slot, Borrowed, Wide, Wide) -> u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_at_most_zero"]
    fn b_at_most(
        context: Slot,
        value: Affine,
        info: fact_fields::ScalarView,
        host: AtMostHost,
    ) -> u32;
}
pub(super) fn at_most_zero(
    query: &IntegerQuery<'_>,
    output: &mut Facts,
    value: IntegerAffine,
    scalar: Scalar,
) -> Option<()> {
    let mut c = Quotient {
        query,
        output,
        scalar,
    };
    let result = unsafe {
        b_at_most(
            (&mut c as *mut Quotient<'_>).cast(),
            Affine::from(&value),
            fact_fields::scalar(borrow(&scalar)),
            AtMostHost {
                retain: quotient_retain,
            },
        )
    };
    (result != 0).then_some(())
}

struct Infer<'a> {
    query: *mut IntegerQuery<'a>,
    output: *mut Facts,
    input: facts::Iter<'a>,
    values: [*mut IntegerAffine; 3],
    root: *mut ValueIdentity,
}
unsafe extern "C" fn infer_next(s: Slot) -> fact_fields::Fields {
    unsafe { frame::<Infer<'_>>(s) }
        .input
        .next()
        .map(fact_fields::Fields::from)
        .unwrap_or_default()
}
unsafe extern "C" fn infer_operand(
    s: Slot,
    side: u32,
    operand: fact_fields::OperandView,
    scalar: Borrowed,
) -> Affine {
    let operand = unsafe { fact_fields::operand_key(operand) };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let c = unsafe { frame::<Infer<'_>>(s) };
    let value = unsafe { &mut *c.query }.operand(operand, scalar);
    let out = c.values[side as usize];
    unsafe { out.write(value) };
    Affine::from(unsafe { &*out })
}
unsafe extern "C" fn infer_expression(s: Slot, definition: Borrowed, scalar: Borrowed) -> Affine {
    let expression = unsafe { *definition.cast::<IntegerExpression>() };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let c = unsafe { frame::<Infer<'_>>(s) };
    match unsafe { &mut *c.query }.expression(expression, scalar) {
        Some(value) => {
            let out = c.values[2];
            unsafe { out.write(value) };
            Affine::from(unsafe { &*out })
        }
        None => Affine::absent(),
    }
}
unsafe extern "C" fn infer_at_most(s: Slot, scalar: Borrowed, value: Affine) -> u32 {
    let value = unsafe { owned(value) };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let c = unsafe { frame::<Infer<'_>>(s) };
    u32::from(
        unsafe { &*c.query }
            .at_most_zero(unsafe { &mut *c.output }, value, scalar)
            .is_some(),
    )
}
unsafe extern "C" fn infer_quotient(
    s: Slot,
    id: Borrowed,
    scalar: Borrowed,
    numerator: fact_fields::OperandView,
    divisor: fact_fields::OperandView,
) -> u32 {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let (numerator, divisor) = unsafe {
        (
            fact_fields::operand_key(numerator),
            fact_fields::operand_key(divisor),
        )
    };
    let c = unsafe { frame::<Infer<'_>>(s) };
    u32::from(
        unsafe { &*c.query }
            .quotient(unsafe { &mut *c.output }, id, scalar, numerator, divisor)
            .is_some(),
    )
}
unsafe extern "C" fn infer_root(s: Slot, id: Borrowed) -> Borrowed {
    let c = unsafe { frame::<Infer<'_>>(s) };
    let value = unsafe { &*c.query }.root(unsafe { *id.cast::<ValueIdentity>() });
    unsafe { c.root.write(value) };
    c.root.cast()
}
unsafe extern "C" fn infer_interval(s: Slot, value: Affine, scalar: Borrowed) -> Range {
    let value = unsafe { owned(value) };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let c = unsafe { frame::<Infer<'_>>(s) };
    range(unsafe { &*c.query }.interval(value, scalar))
}
unsafe extern "C" fn infer_retain(
    s: Slot,
    id: Borrowed,
    scalar: Borrowed,
    lower: Wide,
    upper: Wide,
) -> u32 {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let scalar = unsafe { *scalar.cast::<Scalar>() };
    let c = unsafe { frame::<Infer<'_>>(s) };
    u32::from(
        unsafe { &*c.query }
            .retain(
                unsafe { &mut *c.output },
                id,
                scalar,
                lower.into(),
                upper.into(),
            )
            .is_some(),
    )
}
unsafe extern "C" fn infer_finish(s: Slot) {
    let c = unsafe { frame::<Infer<'_>>(s) };
    let facts = unsafe { &*c.query }.facts;
    unsafe { &mut *c.output }.retain(|fact| !facts.contains(fact));
}
#[repr(C)]
struct InferHost {
    next: unsafe extern "C" fn(Slot) -> fact_fields::Fields,
    scalar: unsafe extern "C" fn(Borrowed) -> fact_fields::ScalarView,
    computed: unsafe extern "C" fn(Borrowed) -> fact_fields::ExpressionView,
    operand: unsafe extern "C" fn(Slot, u32, fact_fields::OperandView, Borrowed) -> Affine,
    expression: unsafe extern "C" fn(Slot, Borrowed, Borrowed) -> Affine,
    at_most: unsafe extern "C" fn(Slot, Borrowed, Affine) -> u32,
    quotient: unsafe extern "C" fn(
        Slot,
        Borrowed,
        Borrowed,
        fact_fields::OperandView,
        fact_fields::OperandView,
    ) -> u32,
    root: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    interval: unsafe extern "C" fn(Slot, Affine, Borrowed) -> Range,
    retain: unsafe extern "C" fn(Slot, Borrowed, Borrowed, Wide, Wide) -> u32,
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
    finish: unsafe extern "C" fn(Slot),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_facts"]
    fn b_facts(context: Slot, host: InferHost) -> u32;
}
pub(super) fn facts(
    facts: &Facts,
    roots: &EqualityRoots,
) -> Option<Facts> {
    let mut query = IntegerQuery::new(facts, roots);
    let mut output = Facts::new();
    let mut a = MaybeUninit::<IntegerAffine>::uninit();
    let mut b = MaybeUninit::<IntegerAffine>::uninit();
    let mut value = MaybeUninit::<IntegerAffine>::uninit();
    let mut root = MaybeUninit::<ValueIdentity>::uninit();
    let result = {
        let mut c = Infer {
            query: &mut query,
            output: &mut output,
            input: facts.iter(),
            values: [a.as_mut_ptr(), b.as_mut_ptr(), value.as_mut_ptr()],
            root: root.as_mut_ptr(),
        };
        unsafe {
            b_facts(
                (&mut c as *mut Infer<'_>).cast(),
                InferHost {
                    next: infer_next,
                    scalar: fact_fields::scalar,
                    computed: fact_fields::computed,
                    operand: infer_operand,
                    expression: infer_expression,
                    at_most: infer_at_most,
                    quotient: infer_quotient,
                    root: infer_root,
                    interval: infer_interval,
                    retain: infer_retain,
                    compare: fact_fields::compare,
                    finish: infer_finish,
                },
            )
        }
    };
    if result != 0 { Some(output) } else { None }
}

#[repr(C)]
struct SourceWork {
    present: u32,
    ready: u32,
    operand: fact_fields::OperandView,
}
struct SourcePlan<'a> {
    query: *mut IntegerQuery<'a>,
    scalar: Scalar,
    pending: Vec<(Operand, bool)>,
    order: Vec<ValueIdentity>,
    current: *mut Operand,
    root: *mut ValueIdentity,
    source: *mut ValueIdentity,
    expression: *mut IntegerExpression,
}
unsafe extern "C" fn source_pop(s: Slot) -> SourceWork {
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    match c.pending.pop() {
        Some((value, ready)) => {
            unsafe { c.current.write(value) };
            SourceWork {
                present: 1,
                ready: u32::from(ready),
                operand: fact_fields::operand(unsafe { &*c.current }),
            }
        }
        None => SourceWork {
            present: 0,
            ready: 0,
            operand: fact_fields::OperandView::default(),
        },
    }
}
unsafe extern "C" fn source_push(s: Slot, value: fact_fields::OperandView, ready: u32) {
    let value = unsafe { fact_fields::operand_key(value) };
    unsafe { frame::<SourcePlan<'_>>(s) }
        .pending
        .push((value, ready != 0));
}
unsafe extern "C" fn source_bounds(s: Slot, id: Borrowed) -> Range {
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    unsafe { bounds(c.query, c.scalar, id) }
}
unsafe extern "C" fn source_enter(s: Slot, id: Borrowed) -> u32 {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    let query = unsafe { &mut *c.query };
    u32::from(query.expanding.insert((query.root(id), c.scalar)))
}
unsafe fn selected_expression(
    query: *const IntegerQuery<'_>,
    scalar: Scalar,
    id: Borrowed,
    target: *mut IntegerExpression,
) -> fact_fields::ExpressionView {
    match unsafe { &*query }.computed(unsafe { *id.cast::<ValueIdentity>() }, scalar) {
        Some(expression) => {
            unsafe { target.write(expression) };
            fact_fields::expression(unsafe { &*target })
        }
        None => fact_fields::ExpressionView::default(),
    }
}
unsafe extern "C" fn source_computed(s: Slot, id: Borrowed) -> fact_fields::ExpressionView {
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    unsafe { selected_expression(c.query, c.scalar, id, c.expression) }
}
unsafe extern "C" fn source_root(s: Slot, id: Borrowed) -> Borrowed {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    unsafe { c.root.write((&*c.query).root(id)) };
    c.root.cast()
}
unsafe extern "C" fn source_select(s: Slot, id: Borrowed) -> Borrowed {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    unsafe { c.source.write(id) };
    c.source.cast()
}
unsafe extern "C" fn source_emit(s: Slot, id: Borrowed) {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    unsafe { frame::<SourcePlan<'_>>(s) }.order.push(id);
}
unsafe extern "C" fn source_clear(s: Slot) {
    let c = unsafe { frame::<SourcePlan<'_>>(s) };
    unsafe { &mut *c.query }.expanding.clear();
}
#[repr(C)]
struct SourcePlanHost {
    pop: unsafe extern "C" fn(Slot) -> SourceWork,
    push: unsafe extern "C" fn(Slot, fact_fields::OperandView, u32),
    bounds: unsafe extern "C" fn(Slot, Borrowed) -> Range,
    enter: unsafe extern "C" fn(Slot, Borrowed) -> u32,
    computed: unsafe extern "C" fn(Slot, Borrowed) -> fact_fields::ExpressionView,
    root: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    select: unsafe extern "C" fn(Slot, Borrowed) -> Borrowed,
    emit: unsafe extern "C" fn(Slot, Borrowed),
    clear: unsafe extern "C" fn(Slot),
    compare: unsafe extern "C" fn(Borrowed, Borrowed) -> i32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_source_plan"]
    fn b_source_plan(context: Slot, host: SourcePlanHost) -> u32;
}
pub(super) fn source_plan(
    query: &mut IntegerQuery<'_>,
    left: ValueIdentity,
    right: ValueIdentity,
    scalar: Scalar,
) -> Option<(ValueIdentity, Vec<ValueIdentity>)> {
    let mut current = MaybeUninit::<Operand>::uninit();
    let mut root = MaybeUninit::<ValueIdentity>::uninit();
    let mut source = MaybeUninit::<ValueIdentity>::uninit();
    let mut expression = MaybeUninit::<IntegerExpression>::uninit();
    let mut c = SourcePlan {
        query,
        scalar,
        pending: vec![
            (Operand::Value(left), false),
            (Operand::Value(right), false),
        ],
        order: Vec::new(),
        current: current.as_mut_ptr(),
        root: root.as_mut_ptr(),
        source: source.as_mut_ptr(),
        expression: expression.as_mut_ptr(),
    };
    let result = unsafe {
        b_source_plan(
            (&mut c as *mut SourcePlan<'_>).cast(),
            SourcePlanHost {
                pop: source_pop,
                push: source_push,
                bounds: source_bounds,
                enter: source_enter,
                computed: source_computed,
                root: source_root,
                select: source_select,
                emit: source_emit,
                clear: source_clear,
                compare: fact_fields::compare,
            },
        )
    };
    if result != 0 {
        Some((unsafe { source.assume_init() }, c.order))
    } else {
        None
    }
}

struct SourceBelow<'a, 'b> {
    query: &'b IntegerQuery<'a>,
    scalar: Scalar,
    order: &'b [ValueIdentity],
    piece: BTreeMap<ValueIdentity, BitRange>,
    expression: *mut IntegerExpression,
}
unsafe extern "C" fn source_seed(s: Slot, id: Borrowed, interval: BitRange) {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<SourceBelow<'_, '_>>(s) };
    let base = c.query.ordered_bounds(id, c.scalar);
    c.piece.clear();
    c.piece.insert(id, BitRange {
        lower: base.lower.max(interval.lower),
        upper: base.upper.min(interval.upper),
    });
}
unsafe extern "C" fn source_next(s: Slot, index: usize) -> Borrowed {
    unsafe { frame::<SourceBelow<'_, '_>>(s) }.order.get(index).map(borrow).unwrap_or(ptr::null())
}
unsafe extern "C" fn below_bounds(s: Slot, id: Borrowed) -> Range {
    let c = unsafe { frame::<SourceBelow<'_, '_>>(s) };
    let id = c.query.root(unsafe { *id.cast::<ValueIdentity>() });
    let bits = c.piece.get(&id).copied().unwrap_or_else(|| c.query.ordered_bounds(id, c.scalar));
    let bias = c.scalar.order_bias();
    (
        IntegerQuery::number(c.scalar, bits.lower ^ bias),
        IntegerQuery::number(c.scalar, bits.upper ^ bias),
    ).into()
}
unsafe extern "C" fn below_computed(s: Slot, id: Borrowed) -> fact_fields::ExpressionView {
    let c = unsafe { frame::<SourceBelow<'_, '_>>(s) };
    unsafe { selected_expression(c.query, c.scalar, id, c.expression) }
}
unsafe extern "C" fn source_store(s: Slot, id: Borrowed, lower: u64, upper: u64) {
    let id = unsafe { *id.cast::<ValueIdentity>() };
    let c = unsafe { frame::<SourceBelow<'_, '_>>(s) };
    c.piece.insert(c.query.root(id), BitRange { lower, upper });
}
#[repr(C)]
struct SourceBelowHost {
    seed: unsafe extern "C" fn(Slot, Borrowed, BitRange),
    next: unsafe extern "C" fn(Slot, usize) -> Borrowed,
    bounds: unsafe extern "C" fn(Slot, Borrowed) -> Range,
    computed: unsafe extern "C" fn(Slot, Borrowed) -> fact_fields::ExpressionView,
    same_scalar: unsafe extern "C" fn(Borrowed, Borrowed) -> u32,
    store: unsafe extern "C" fn(Slot, Borrowed, u64, u64),
}
unsafe extern "C" {
    #[link_name = "bootstrap.condition_graph::relation_source_below"]
    fn b_source_below(
        context: Slot,
        left: Borrowed,
        right: Borrowed,
        source: Borrowed,
        intervals: crate::ir::scalar_ranges::RangeInput,
        scalar: Borrowed,
        info: fact_fields::ScalarView,
        host: SourceBelowHost,
    ) -> u32;
}
pub(super) fn below_in(
    query: &IntegerQuery<'_>,
    left: ValueIdentity,
    right: ValueIdentity,
    scalar: Scalar,
    source: ValueIdentity,
    ranges: &[BitRange],
    order: &[ValueIdentity],
) -> bool {
    let mut expression = MaybeUninit::<IntegerExpression>::uninit();
    let mut c = SourceBelow {
        query,
        scalar,
        order,
        piece: BTreeMap::new(),
        expression: expression.as_mut_ptr(),
    };
    unsafe {
        b_source_below(
            ptr::from_mut(&mut c).cast(),
            borrow(&left),
            borrow(&right),
            borrow(&source),
            crate::ir::scalar_ranges::RangeInput::records(ranges, range_sets::read_record),
            borrow(&scalar),
            fact_fields::scalar(borrow(&scalar)),
            SourceBelowHost {
                seed: source_seed,
                next: source_next,
                bounds: below_bounds,
                computed: below_computed,
                same_scalar: fact_fields::same_scalar,
                store: source_store,
            },
        ) != 0
    }
}
