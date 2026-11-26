//! Synchronous POD numerical calls; Proof, Value and ScalarRanges stay native.
use super::*;
use std::{collections::btree_set, ffi::c_void, ptr};
use crate::ast::TextureFormat;
use crate::builtin_types::ScalarBinaryOp;
#[repr(C)]
#[derive(Clone, Copy)]
struct RawRange { lower: Bits, upper: Bits }
#[repr(C)]
struct TextureSampleBounds { x: BitRange, y: BitRange, z: BitRange, lod: BitRange, has_z: u32 }
#[repr(C)]
pub(super) struct NumericalRange { lower: Bits, upper: Bits, state: u32 }
#[repr(C)]
struct Pieces { first: RawRange, second: RawRange, count: u32 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Encoding { kind: u32, width: u32 }
fn encoding(value: Scalar) -> Encoding {
    Encoding { kind: if value.is_float() { 2 } else if value == Scalar::Bool { 3 }
        else if value.is_signed() { 1 } else { 0 }, width: value.bit_width() }
}
fn raw(range: (Bits, Bits)) -> RawRange { RawRange { lower: range.0, upper: range.1 } }
pub(super) fn decoded(value: NumericalRange) -> ScalarRanges {
    match value.state {
        1 => ScalarRanges::interval(value.lower, value.upper),
        2 => ScalarRanges::empty(),
        _ => ScalarRanges::default(),
    }
}
unsafe extern "C" {
    #[link_name="bootstrap.scalar_fold::float_bits"] fn source_bits(width:u32,value:Bits)->Bits;
    #[link_name="bootstrap.scalar_fold::float_order"] fn source_order(width:u32,value:Bits)->Bits;
    #[link_name="bootstrap.scalar_fold::float_interval"] fn source_interval(width:u32,lower:f64,upper:f64)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_comparison_possible"] fn source_comparison(operation:u32,width:u32,left:RawRange,right:RawRange,truth:u32)->u32;
    #[link_name="bootstrap.scalar_fold::float_bitcast"] fn source_bitcast(width:u32,range:RawRange,source_float:u32)->Pieces;
    #[link_name = "bootstrap.scalar_fold::float_cast_range"]
    fn source_cast(
        source: Encoding,
        target: Encoding,
        range: RawRange,
        origin: *const ValueIdentity,
        context: *mut c_void,
        next: unsafe extern "C" fn(*mut c_void) -> FloatCastRelation,
        emit: unsafe extern "C" fn(*mut c_void, FloatCastFact),
    ) -> NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_math"] fn source_math(operation:u32,width:u32,left:RawRange,right:RawRange)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_sum_range"] fn source_sum(width:u32,range:RawRange,maximum:Bits)->NumericalRange;
    #[link_name = "bootstrap.scalar_fold::texture_sample_range"]
    fn source_texture_sample(format: TextureFormat, input: TextureSampleBounds) -> NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_negate"] fn source_negate(width:u32,range:RawRange)->NumericalRange;
}
pub(in crate::check::bounds) fn bits(scalar:Scalar,value:Bits)->Bits {unsafe{source_bits(scalar.bit_width(),value)}}
pub(in crate::check::bounds) fn order(scalar:Scalar,value:Bits)->Bits {unsafe{source_order(scalar.bit_width(),value)}}
pub(in crate::check::bounds) fn interval(scalar:Scalar,lower:f64,upper:f64)->ScalarRanges {decoded(unsafe{source_interval(scalar.bit_width(),lower,upper)})}
pub(in crate::check::bounds) fn comparison_possible(op:Comparison,scalar:Scalar,left:(Bits,Bits),right:(Bits,Bits),truth:bool)->bool {
    let operation=match op{Comparison::Equal=>10,Comparison::NotEqual=>11,Comparison::Less=>12,Comparison::Greater=>13,Comparison::LessEqual=>14,Comparison::GreaterEqual=>15};
    unsafe{source_comparison(operation,scalar.bit_width(),raw(left),raw(right),u32::from(truth))!=0}
}
pub(in crate::check::bounds) fn bitcast(value:&Value,component:usize,target:Scalar)->ScalarRanges {
    let source=value.ty.components[component].0;
    let (scalar,ranges)=if source.is_float(){(source,super::ranges(value,component))}else{(target,value.component_ranges(component))};
    ranges.map(|range|{
        let result=unsafe{source_bitcast(scalar.bit_width(),raw(range),u32::from(source.is_float()))};
        ScalarRanges::from_intervals([result.first,result.second].into_iter().take(result.count as usize).map(|value|(value.lower,value.upper)))
    })
}
#[repr(u32)]
enum FloatCastRelationKind { End, Upper, Remainder }

#[repr(C)]
struct FloatCastRelation {
    kind: FloatCastRelationKind,
    encoding: Encoding,
    origin: *const ValueIdentity,
    distance: u32,
}

#[repr(C)]
struct FloatCastFact {
    origin: *const ValueIdentity,
    range: RawRange,
    scale: f64,
    offset: f64,
    upper: u32,
}

struct FloatCast<'a> {
    source: Scalar,
    component: usize,
    relations: btree_set::Iter<'a, IndexDomain>,
    proof: &'a mut Proof,
}

unsafe extern "C" fn cast_relation(context: *mut c_void) -> FloatCastRelation {
    let input = unsafe { &mut *context.cast::<FloatCast<'_>>() };
    input.relations.find_map(|domain| match domain {
        IndexDomain::Upper(origin, distance) => Some(FloatCastRelation {
            kind: FloatCastRelationKind::Upper,
            encoding: encoding(input.source),
            origin,
            distance: *distance,
        }),
        IndexDomain::Computed(IntegerExpression::Binary(
            IntegerOperation::Remainder, scalar, _, Operand::Value(origin),
        )) => Some(FloatCastRelation {
            kind: FloatCastRelationKind::Remainder,
            encoding: encoding(*scalar),
            origin,
            distance: 1,
        }),
        _ => None,
    }).unwrap_or(FloatCastRelation {
        kind: FloatCastRelationKind::End,
        encoding: encoding(input.source),
        origin: ptr::null(),
        distance: 0,
    })
}

unsafe extern "C" fn cast_fact(context: *mut c_void, fact: FloatCastFact) {
    let input = unsafe { &mut *context.cast::<FloatCast<'_>>() };
    let domain = FloatingBound {
        source: unsafe { *fact.origin },
        lower: fact.range.lower,
        upper: fact.range.upper,
        terms: [fact.scale.to_bits(), fact.offset.to_bits(), 0],
        scalar: input.source,
        form: if fact.upper == 0 { FloatingForm::Cast } else { FloatingForm::Upper },
    };
    input.proof.mark_domain(input.component, IndexDomain::Floating(domain));
}

pub(in crate::check::bounds) fn cast(
    value: &Value, component: usize, target: Scalar, proof: &mut Proof,
) -> ScalarRanges {
    let source = value.ty.components[component].0;
    let range = raw(proof.ranges[component].hull());
    let origin = value.component_identity(component);
    let mut input = FloatCast {
        source,
        component,
        relations: value.proof.domains.get(component).map(Domains::iter).unwrap_or_default(),
        proof,
    };
    decoded(unsafe {
        source_cast(encoding(source), encoding(target), range, &origin,
            ptr::from_mut(&mut input).cast(), cast_relation, cast_fact)
    })
}
pub(super) fn arithmetic_operation(op: &str) -> Option<ScalarBinaryOp> {
    match op {
        "+" => Some(ScalarBinaryOp::Add),
        "-" => Some(ScalarBinaryOp::Subtract),
        "*" => Some(ScalarBinaryOp::Multiply),
        "/" => Some(ScalarBinaryOp::Divide),
        _ => None,
    }
}
pub(in crate::check::bounds) fn math(op:MathOp,args:&[Value],component:usize)->ScalarRanges {
    let a=&args[0];let left=super::ranges(a,component%a.regs.len());
    let right=if matches!(op,MathOp::Min|MathOp::Max){let b=&args[1];raw(super::ranges(b,component%b.regs.len()).hull())}else{RawRange{lower:0,upper:0}};
    let operation=match op{MathOp::Min=>1,MathOp::Max=>2,MathOp::Abs=>3,MathOp::Floor=>4,MathOp::Log=>5,MathOp::Log2=>6,MathOp::Ceil=>7,_=>0};
    decoded(unsafe{source_math(operation,a.ty.components[0].0.bit_width(),raw(left.hull()),right)})
}
pub(in crate::check::bounds) fn sum(input:&Proof,component:usize,scalar:Scalar,maximum:Bits)->ScalarRanges {
    let hull = input.element_hull(component, scalar);
    decoded(unsafe { source_sum(scalar.bit_width(), raw(hull), maximum) })
}
pub(super) fn texture_sample(format: Option<TextureFormat>, coordinate: &Value, lod: &Value) -> Option<ScalarRanges> {
    let format = format?;
    let hull = |value: &Value, component: usize| {
        let clip = value.constant.as_ref().map_or((0, u64::from(u32::MAX)), |values| {
            let point = order(Scalar::F32, values[component]);
            (point, point)
        });
        range_sets::restricted_hull(value.proof.interval_source(component), clip, None).into()
    };
    let z = (coordinate.regs.len() == 3).then(|| hull(coordinate, 2));
    let input = TextureSampleBounds {
        x: hull(coordinate, 0),
        y: hull(coordinate, 1),
        z: z.unwrap_or_default(),
        lod: hull(lod, 0),
        has_z: u32::from(z.is_some()),
    };
    let range = unsafe { source_texture_sample(format, input) };
    (range.state == 1).then(|| decoded(range))
}

pub(in crate::check::bounds) fn negate(value:&Value,component:usize)->ScalarRanges {
    decoded(unsafe{source_negate(value.ty.components[component].0.bit_width(),raw(super::ranges(value,component).hull()))})
}
