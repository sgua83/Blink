//! Synchronous POD numerical calls; Proof, Value and ScalarRanges stay native.
use super::*;
#[repr(C)]
#[derive(Clone, Copy)]
struct RawRange { lower: Bits, upper: Bits }
#[repr(C)]
struct Finite { lower: f64, upper: f64, present: u32 }
#[repr(C)]
struct NumericalRange { lower: Bits, upper: Bits, state: u32 }
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
fn decoded(value: NumericalRange) -> ScalarRanges {
    match value.state {
        1 => ScalarRanges::interval(value.lower, value.upper),
        2 => ScalarRanges(Vec::new()),
        _ => ScalarRanges::default(),
    }
}
unsafe extern "C" {
    #[link_name="bootstrap.scalar_fold::float_bits"] fn source_bits(width:u32,value:Bits)->Bits;
    #[link_name="bootstrap.scalar_fold::float_order"] fn source_order(width:u32,value:Bits)->Bits;
    #[link_name="bootstrap.scalar_fold::float_finite"] fn source_finite(width:u32,range:RawRange)->Finite;
    #[link_name="bootstrap.scalar_fold::float_interval"] fn source_interval(width:u32,lower:f64,upper:f64)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_comparison_possible"] fn source_comparison(operation:u32,width:u32,left:RawRange,right:RawRange,truth:u32)->u32;
    #[link_name="bootstrap.scalar_fold::float_bitcast"] fn source_bitcast(width:u32,range:RawRange,source_float:u32)->Pieces;
    #[link_name="bootstrap.scalar_fold::float_cast_range"] fn source_cast(source:Encoding,target:Encoding,range:RawRange)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_range_arithmetic"] fn source_arithmetic(operation:u32,width:u32,left:RawRange,right:RawRange)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_math"] fn source_math(operation:u32,width:u32,left:RawRange,right:RawRange)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_sum_range"] fn source_sum(width:u32,range:RawRange,maximum:Bits)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_negate"] fn source_negate(width:u32,range:RawRange)->NumericalRange;
    #[link_name="bootstrap.scalar_fold::float_add_up"] fn source_add_up(left:f64,right:f64)->f64;
    #[link_name="bootstrap.scalar_fold::float_multiply_up"] fn source_multiply_up(left:f64,right:f64)->f64;
    #[link_name="bootstrap.scalar_fold::float_divide_up"] fn source_divide_up(left:f64,right:f64)->f64;
    #[link_name="bootstrap.scalar_fold::float_integer_bound"] fn source_integer_bound(value:Bits,lower:u32)->f64;
    #[link_name="bootstrap.scalar_fold::float_conversion_error"] fn source_conversion_error(upper:Bits)->f64;
}
pub(in crate::check::bounds) fn bits(scalar:Scalar,value:Bits)->Bits {unsafe{source_bits(scalar.bit_width(),value)}}
pub(in crate::check::bounds) fn order(scalar:Scalar,value:Bits)->Bits {unsafe{source_order(scalar.bit_width(),value)}}
pub(in crate::check::bounds) fn finite_ranges(scalar:Scalar,ranges:ScalarRanges)->Option<(f64,f64)> {
    let result=unsafe{source_finite(scalar.bit_width(),raw(ranges.hull()))};
    (result.present!=0).then_some((result.lower,result.upper))
}
pub(in crate::check::bounds) fn finite(value:&Value,component:usize)->Option<(f64,f64)> {
    finite_ranges(value.ty.components[component].0,super::ranges(value,component))
}
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
pub(in crate::check::bounds) fn cast(value:&Value,component:usize,target:Scalar)->ScalarRanges {
    let source=value.ty.components[component].0;
    let ranges=if source.is_float(){super::ranges(value,component)}else{value.component_ranges(component)};
    decoded(unsafe{source_cast(encoding(source),encoding(target),raw(ranges.hull()))})
}
pub(in crate::check::bounds) fn arithmetic(op:&str,a:&Value,b:&Value,component:usize)->ScalarRanges {
    let operation=match op{"+"=>0,"-"=>1,"*"=>2,"/"=>3,_=>u32::MAX};
    let left=super::ranges(a,component%a.regs.len());let right=super::ranges(b,component%b.regs.len());
    decoded(unsafe{source_arithmetic(operation,a.ty.components[0].0.bit_width(),raw(left.hull()),raw(right.hull()))})
}
pub(in crate::check::bounds) fn math(op:MathOp,args:&[Value],component:usize)->ScalarRanges {
    let a=&args[0];let left=super::ranges(a,component%a.regs.len());
    let right=if matches!(op,MathOp::Min|MathOp::Max){let b=&args[1];raw(super::ranges(b,component%b.regs.len()).hull())}else{RawRange{lower:0,upper:0}};
    let operation=match op{MathOp::Min=>1,MathOp::Max=>2,MathOp::Abs=>3,MathOp::Floor=>4,MathOp::Log=>5,MathOp::Log2=>6,_=>0};
    decoded(unsafe{source_math(operation,a.ty.components[0].0.bit_width(),raw(left.hull()),right)})
}
pub(in crate::check::bounds) fn sum(input:&Proof,component:usize,scalar:Scalar,maximum:Bits)->ScalarRanges {
    let ranges=input.element_range(component,scalar);decoded(unsafe{source_sum(scalar.bit_width(),raw(ranges.hull()),maximum)})
}
pub(in crate::check::bounds) fn negate(value:&Value,component:usize)->ScalarRanges {
    decoded(unsafe{source_negate(value.ty.components[component].0.bit_width(),raw(super::ranges(value,component).hull()))})
}
pub(in crate::check::bounds) fn add_up(left:f64,right:f64)->f64 {unsafe{source_add_up(left,right)}}
pub(in crate::check::bounds) fn multiply_up(left:f64,right:f64)->f64 {unsafe{source_multiply_up(left,right)}}
pub(in crate::check::bounds) fn divide_up(left:f64,right:f64)->f64 {unsafe{source_divide_up(left,right)}}
pub(in crate::check::bounds) fn integer_bound(value:Bits,lower:bool)->f64 {unsafe{source_integer_bound(value,u32::from(lower))}}
pub(in crate::check::bounds) fn conversion_error(upper:Bits)->f64 {unsafe{source_conversion_error(upper)}}
