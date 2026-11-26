//! B owns complete scalar-set rules; native containers retain their ownership.
use super::{Bits,Scalar,ScalarRanges};
use std::{ffi::c_void,ptr};
use crate::ir::scalar_ranges::{self, BitPattern as Pattern, BitRange as Range, Pair, push};
type Slot=*mut c_void;
type Borrowed=*const c_void;
#[repr(C)]
struct Next {value:Range,present:u32}
#[repr(C)]
struct CastSummary { hull: Range, contains: u32 }
struct Sources<'a>{left:&'a [Pair],right:&'a [Pair]}
type Read=unsafe extern "C" fn(Borrowed,u32,usize)->Range;
type Push=unsafe extern "C" fn(Slot,Range);
type Pull=unsafe extern "C" fn(Slot)->Next;
type Invoke=unsafe extern "C" fn(Slot,Range,Range,Slot)->usize;
#[repr(C)]
struct Output {
    get:unsafe extern "C" fn(Slot,usize)->Range,
    set:unsafe extern "C" fn(Slot,usize,Range),
    push:Push,
}
#[repr(C)]
pub(super) struct Projection {
    lower: u64,
    upper: u64,
    excluded: u64,
    exclude: u32,
}

impl Projection {
    pub(super) fn new(lower: u64, upper: u64, excluded: Option<u64>) -> Self {
        Self { lower, upper, excluded: excluded.unwrap_or_default(), exclude: u32::from(excluded.is_some()) }
    }
}

#[repr(C)]
struct Requirement {
    order_bias: Bits,
    known: Pattern,
    required: Pattern,
    intervals: Borrowed,
    count: usize,
    interval: unsafe extern "C" fn(Borrowed, usize) -> Range,
}

#[repr(C)]
struct Cast {
    source_width:u32,source_signed:u32,target_width:u32,
    source_integer:u32,same:u32,nonzero:u32,
    source_mask:u64,source_bias:u64,target_bias:u64,
}
impl Cast {
    fn new(source: Scalar, target: Scalar, nonzero: bool) -> Self {
        Self {
            source_width: source.bit_width(),
            source_signed: u32::from(source.is_signed()),
            target_width: target.bit_width(),
            source_integer: u32::from(source.is_integer()),
            same: u32::from(source == target),
            nonzero: u32::from(nonzero),
            source_mask: source.bit_mask(),
            source_bias: source.order_bias(),
            target_bias: target.order_bias(),
        }
    }
}
#[repr(C)]
struct CastStep {
    source_width: u32,
    source_signed: u32,
    target_width: u32,
    same: u32,
    target_bias: u64,
}
impl CastStep {
    fn new(source: Scalar, target: Scalar) -> Self {
        Self {
            source_width: source.bit_width(),
            source_signed: u32::from(source.is_signed()),
            target_width: target.bit_width(),
            same: u32::from(source == target),
            target_bias: target.order_bias(),
        }
    }
}
unsafe extern "C" fn read(slot:Borrowed,side:u32,index:usize)->Range {
    let source=unsafe{&*slot.cast::<Sources<'_>>()};
    (if side==0{source.left}else{source.right})[index].into()
}
unsafe extern "C" fn read_requirement(slot: Borrowed, index: usize) -> Range {
    unsafe { read(slot, 1, index) }
}
unsafe extern "C" fn get(slot:Slot,index:usize)->Range {{let values=unsafe{&*slot.cast::<Vec<Pair>>()};values[index].into()}}
unsafe extern "C" fn set(slot:Slot,index:usize,value:Range) {let values=unsafe{&mut *slot.cast::<Vec<Pair>>()};values[index]=value.into();}
unsafe extern "C" fn pull<I:Iterator<Item=Pair>>(slot:Slot)->Next {
    unsafe{&mut *slot.cast::<I>()}.next().map(|v|Next{value:v.into(),present:1}).unwrap_or(Next{value:Range::default(),present:0})
}
unsafe extern "C" fn invoke<F:FnMut(Pair,Pair)->ScalarRanges>(slot:Slot,left:Range,right:Range,output:Slot)->usize {
    let value=unsafe{&mut *slot.cast::<F>()}(left.into(),right.into());
    let output=unsafe{&mut *output.cast::<Vec<Pair>>()};output.extend(value.0);output.len()
}
fn host()->Output {Output{get,set,push}}
fn inputs<R>(left:&[Pair],right:&[Pair],run:impl FnOnce(Borrowed,usize,usize)->R)->R {
    let source=Sources{left,right};run(ptr::from_ref(&source).cast(),left.len(),right.len())
}
fn output(run:impl FnOnce(Slot))->ScalarRanges {
    let mut values=Vec::<Pair>::new();run(ptr::from_mut(&mut values).cast());ScalarRanges(values)
}
fn normalized_output(run:impl FnOnce(Slot,Output)->usize)->ScalarRanges {
    let mut values=Vec::<Pair>::new();let count=run(ptr::from_mut(&mut values).cast(),host());values.truncate(count);ScalarRanges(values)
}
unsafe extern "C" {
    #[link_name="bootstrap.scalar_fold::range_set_normalize"]
    fn normalize(output:Slot,count:usize,host:Output)->usize;
    #[link_name="bootstrap.scalar_fold::range_set_sorted"]
    fn sorted(input:Slot,pull:Pull,output:Slot,push:Push);
    #[link_name="bootstrap.scalar_fold::range_set_full"]
    fn all(output:Slot,push:Push);
    #[link_name="bootstrap.scalar_fold::range_set_interval"]
    fn single(output:Slot,push:Push,lower:u64,upper:u64);
    #[link_name="bootstrap.scalar_fold::range_set_hull"]
    fn outer(input:Borrowed,count:usize,read:Read)->Range;
    #[link_name="bootstrap.scalar_fold::range_set_contains"]
    fn member(input:Borrowed,count:usize,read:Read,value:u64)->bool;
    #[link_name="bootstrap.scalar_fold::range_set_union"]
    fn united(input:Borrowed,left:usize,right:usize,read:Read,output:Slot,push:Push);
    #[link_name="bootstrap.scalar_fold::range_set_restrict"]
    fn clipped(input:Borrowed,count:usize,read:Read,output:Slot,push:Push,lower:u64,upper:u64);
    #[link_name="bootstrap.scalar_fold::range_set_project"]
    fn projected(input:Borrowed,count:usize,read:Read,output:Slot,push:Push,projection:Projection);
    #[link_name="bootstrap.scalar_fold::range_set_project_contains"]
    fn projected_contains(input: Borrowed, count: usize, read: Read, output: Slot, push: Push,
        projection: Projection, point: Bits) -> bool;
    #[link_name="bootstrap.scalar_fold::range_set_projected_hull"]
    fn projected_hull(input: Borrowed, count: usize, read: Read, projection: Projection) -> Range;
    #[link_name="bootstrap.scalar_fold::range_set_covers_projected"]
    fn covers_projected(input: Borrowed, count: usize, read: Read, projection: Projection, requirement: Requirement) -> bool;
    #[link_name="bootstrap.scalar_fold::range_set_without"]
    fn excluded(input:Borrowed,count:usize,read:Read,output:Slot,push:Push,value:u64);
    #[link_name="bootstrap.scalar_fold::range_set_cast"]
    fn converted(input:Borrowed,count:usize,read:Read,output:Slot,host:Output,query:Cast)->usize;
    #[link_name="bootstrap.scalar_fold::range_set_cast_chain"]
    fn converted_chain(input: Borrowed, count: usize, read: Read, output: Slot, host: Output,
        first: Cast, second: CastStep, last: CastStep) -> usize;
    #[link_name="bootstrap.scalar_fold::range_set_cast_summary"]
    fn converted_summary(input: Borrowed, count: usize, read: Read,
        first: Cast, second: CastStep, last: CastStep) -> CastSummary;
    #[link_name="bootstrap.scalar_fold::range_set_counts"]
    fn counts(input:Borrowed,count:usize,read:Read,output:Slot,host:Output,width:u32,bias:u64)->usize;
    #[link_name="bootstrap.scalar_fold::range_set_apply"]
    fn applied(input:Borrowed,left:usize,right:usize,read:Read,operation:Slot,invoke:Invoke,output:Slot,host:Output,combine:u32)->usize;
}
pub(super) fn full()->ScalarRanges {output(|o|unsafe{all(o,push)})}
pub(super) fn interval(lower:u64,upper:u64)->ScalarRanges {output(|o|unsafe{single(o,push,lower,upper)})}
pub(super) fn from_intervals(values:impl IntoIterator<Item=Pair>)->ScalarRanges {
    let mut values:Vec<_>=values.into_iter().collect();let count=unsafe{normalize(ptr::from_mut(&mut values).cast(),values.len(),host())};values.truncate(count);ScalarRanges(values)
}
pub(super) fn from_sorted_intervals<I:IntoIterator<Item=Pair>>(values:I)->ScalarRanges {
    let mut stream=values.into_iter();output(|o|unsafe{sorted(ptr::from_mut(&mut stream).cast(),pull::<I::IntoIter>,o,push)})
}
pub(super) fn hull(values:&[Pair])->Pair {inputs(values,&[],|s,n,_|unsafe{outer(s,n,read)}.into())}
pub(super) fn contains(values:&[Pair],value:u64)->bool {inputs(values,&[],|s,n,_|unsafe{member(s,n,read,value)})}
pub(super) fn union(left:&[Pair],right:&[Pair])->ScalarRanges {inputs(left,right,|s,n,m|output(|o|unsafe{united(s,n,m,read,o,push)}))}
pub(super) fn intersection(left: &[Pair], right: &[Pair]) -> ScalarRanges {
    ScalarRanges(scalar_ranges::intersect_sorted(left, right))
}
pub(super) fn restrict(values:&[Pair],lower:u64,upper:u64)->ScalarRanges {inputs(values,&[],|s,n,_|output(|o|unsafe{clipped(s,n,read,o,push,lower,upper)}))}
pub(super) fn project(values: &[Pair], projection: Projection) -> ScalarRanges {
    inputs(values, &[], |source, count, _| {
        output(|dest| unsafe { projected(source, count, read, dest, push, projection) })
    })
}
pub(super) fn project_contains(values: &[Pair], projection: Projection, point: Bits) -> (ScalarRanges, bool) {
    let mut result = Vec::<Pair>::new();
    let contains = inputs(values, &[], |source, count, _| unsafe {
        projected_contains(source, count, read, ptr::from_mut(&mut result).cast(), push, projection, point)
    });
    (ScalarRanges(result), contains)
}
pub(super) fn restricted_hull(values: &[Pair], (lower, upper): Pair, excluded: Option<Bits>) -> Pair {
    let projection = Projection::new(lower, upper, excluded);
    inputs(values, &[], |source, count, _| unsafe {
        projected_hull(source, count, read, projection).into()
    })
}
pub(super) fn covers_element(
    values: &[Pair], scalar: Scalar, excluded: Option<Bits>, known: Pair, intervals: &[Pair], required: Pair,
) -> bool {
    let projection = Projection::new(0, scalar.bit_mask(), excluded);
    inputs(values, intervals, |source, count, required_count| {
        let requirement = Requirement {
            order_bias: scalar.order_bias(),
            known: known.into(),
            required: required.into(),
            intervals: source,
            count: required_count,
            interval: read_requirement,
        };
        unsafe { covers_projected(source, count, read, projection, requirement) }
    })
}
pub(super) fn without(values:&[Pair],value:u64)->ScalarRanges {inputs(values,&[],|s,n,_|output(|o|unsafe{excluded(s,n,read,o,push,value)}))}
pub(super) fn cast(values:&[Pair],source:Scalar,target:Scalar,nonzero:bool)->ScalarRanges {
    let query = Cast::new(source, target, nonzero);
    inputs(values,&[],|s,n,_|normalized_output(|o,h|unsafe{converted(s,n,read,o,h,query)}))
}
pub(super) fn cast_chain(values: &[Pair], [from, low, extended, to]: [Scalar; 4], nonzero: bool) -> ScalarRanges {
    let first = Cast::new(from, low, nonzero);
    let second = CastStep::new(low, extended);
    let last = CastStep::new(extended, to);
    inputs(values, &[], |source, count, _| {
        normalized_output(|out, host| unsafe {
            converted_chain(source, count, read, out, host, first, second, last)
        })
    })
}
pub(super) fn cast_summary(values: &[Pair], [from, low, extended, to]: [Scalar; 4], nonzero: bool) -> (Pair, u32) {
    let first = Cast::new(from, low, nonzero);
    let second = CastStep::new(low, extended);
    let last = CastStep::new(extended, to);
    let summary = inputs(values, &[], |source, count, _| unsafe {
        converted_summary(source, count, read, first, second, last)
    });
    (summary.hull.into(), summary.contains)
}
pub(super) fn shift_counts(values:&[Pair],scalar:Scalar)->ScalarRanges {inputs(values,&[],|s,n,_|normalized_output(|o,h|unsafe{counts(s,n,read,o,h,scalar.bit_width(),scalar.order_bias())}))}
fn apply<F:FnMut(Pair,Pair)->ScalarRanges>(left:&[Pair],right:&[Pair],combine:bool,mut operation:F)->ScalarRanges {
    inputs(left,right,|s,n,m|normalized_output(|o,h|unsafe{applied(s,n,m,read,ptr::from_mut(&mut operation).cast(),invoke::<F>,o,h,u32::from(combine))}))
}
pub(super) fn map(values:&[Pair],mut operation:impl FnMut(Pair)->ScalarRanges)->ScalarRanges {apply(values,&[],false,|a,_|operation(a))}
pub(super) fn combine(left:&[Pair],right:&[Pair],operation:impl FnMut(Pair,Pair)->ScalarRanges)->ScalarRanges {apply(left,right,true,operation)}
