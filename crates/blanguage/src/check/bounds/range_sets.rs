//! Borrows of the canonical scalar interval records owned by B.
use super::{Bits, CallBindings, IntegerExpression, IntegerOperation, Operand, Scalar, ScalarRanges, ValueIdentity};
use std::{ffi::c_void, ops::Deref, ptr, slice};
use crate::ir::scalar_ranges::{self, BitPattern as Pattern, BitRange as Range, Pair, RangeInput};
type Slot=*mut c_void;
type Borrowed=*const c_void;
#[repr(C)]
struct Next {value:Range,present:u32}
#[repr(C)]
struct CastSummary { hull: Range, contains: u32 }
#[repr(C)]
struct Sources { left: *const Range, right: *const Range }
type Read=unsafe extern "C" fn(Borrowed,u32,usize)->Range;
type Push=unsafe extern "C" fn(Slot,Range);
type Pull=unsafe extern "C" fn(Slot)->Next;
type Invoke=unsafe extern "C" fn(Slot,Range,Slot)->usize;
#[repr(C)]
struct Output {
    get:unsafe extern "C" fn(Slot,usize)->Range,
    set:unsafe extern "C" fn(Slot,usize,Range),
    push:Push,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct HeapRanges {
    capacity: usize,
    records: *mut Range,
}

#[repr(C)]
union Payload {
    inline: Range,
    heap: HeapRanges,
}

/// The only owner of this inline interval or heap-backed interval sequence.
/// B updates the count and payload together under an exclusive borrow.
#[repr(C)]
pub(super) struct RangeStorage {
    count: usize,
    payload: Payload,
}

impl RangeStorage {
    pub(super) fn empty() -> Self {
        Self { count: 0, payload: Payload { inline: Range::default() } }
    }

    pub(super) fn as_slice(&self) -> &[Range] {
        match self.count {
            0 => &[],
            1 => slice::from_ref(unsafe { &self.payload.inline }),
            count => unsafe { slice::from_raw_parts(self.payload.heap.records, count) },
        }
    }

    pub(super) fn to_pairs(&self) -> Vec<Pair> {
        self.as_slice().iter().copied().map(Pair::from).collect()
    }

    fn slot(&mut self) -> Slot {
        ptr::from_mut(self).cast()
    }

    fn truncate(&mut self, count: usize) {
        unsafe { truncate(self.slot(), count) }
    }
}

impl Deref for RangeStorage {
    type Target = [Range];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl PartialEq for RangeStorage {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Clone for RangeStorage {
    fn clone(&self) -> Self {
        let mut result = Self::empty();
        unsafe { clone_ranges(ptr::from_ref(self).cast(), result.slot()) }
        result
    }
}

impl Drop for RangeStorage {
    fn drop(&mut self) {
        unsafe { drop_ranges(self.slot()) }
    }
}

// Shared borrows only read initialized records. A move transfers the whole
// allocation, and cloning gives each owner independent storage.
unsafe impl Send for RangeStorage {}
unsafe impl Sync for RangeStorage {}
#[derive(Clone, Copy)]
#[repr(u32)]
enum BinaryOperation {
    Minimum,
    Maximum,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
    Unknown,
}

impl BinaryOperation {
    fn from_operator(op: &str) -> Self {
        match op {
            "min" => Self::Minimum,
            "max" => Self::Maximum,
            "+" => Self::Add,
            "-" => Self::Subtract,
            "*" => Self::Multiply,
            "/" => Self::Divide,
            "%" => Self::Remainder,
            "<<" => Self::ShiftLeft,
            ">>" => Self::ShiftRight,
            "&" => Self::BitAnd,
            "|" => Self::BitOr,
            "^" => Self::BitXor,
            _ => Self::Unknown,
        }
    }
}

pub(super) struct BinaryEvidence {
    pub(super) same_value: bool,
    pub(super) zero_quotient: bool,
    pub(super) minimum_difference: u32,
    pub(super) right_scalar: Scalar,
}

#[repr(C)]
struct BinaryProof {
    operation: BinaryOperation,
    width: u32,
    signed: u32,
    same_value: u32,
    bias: u64,
    mask: u64,
    minimum_difference: u64,
    right_bias: u64,
    right_width: u32,
    zero_quotient: u32,
    track_origin: u32,
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum Origin {
    Neither,
    Left,
    Right,
}

#[repr(C)]
struct BinaryResult {
    count: usize,
    origin: Origin,
}

#[repr(C)]
struct ComputedType {
    width: u32,
    signed: u32,
    floating: u32,
    mask: u64,
    bias: u64,
}

#[repr(C)]
struct CallOperand {
    kind: u32,
    identity: Borrowed,
    bits: u64,
}

impl CallOperand {
    fn borrowed(value: &Operand) -> Self {
        match value {
            Operand::Value(id) => Self { kind: 1, identity: ptr::from_ref(id).cast(), bits: 0 },
            Operand::Constant(_, bits) => Self { kind: 2, identity: ptr::null(), bits: *bits },
        }
    }

    unsafe fn projected(self, original: Operand, scalar: Scalar) -> Option<Operand> {
        match self.kind {
            0 => None,
            1 => Some(Operand::Value(unsafe { *self.identity.cast::<ValueIdentity>() })),
            2 => Some(match original {
                Operand::Constant(..) => original,
                Operand::Value(_) => Operand::Constant(scalar, self.bits),
            }),
            _ => unreachable!("invalid integer call operand"),
        }
    }
}

#[repr(C)]
struct CallBinding {
    present: u32,
    identity: Borrowed,
    intervals: Borrowed,
    count: usize,
}

struct CallInputs<'a, P> {
    bindings: &'a P,
    identities: [Option<ValueIdentity>; 2],
}

unsafe extern "C" fn call_input<P: CallBindings>(context: Slot, side: u32, source: Borrowed) -> CallBinding {
    let call = context.cast::<CallInputs<'_, P>>();
    let bindings = unsafe { (*call).bindings };
    let source = unsafe { *source.cast::<ValueIdentity>() };
    let Some(input) = bindings.input(source) else {
        return CallBinding { present: 0, identity: ptr::null(), intervals: ptr::null(), count: 0 };
    };
    // B may already borrow the other identity slot.
    let slot = unsafe { &mut *ptr::addr_of_mut!((*call).identities).cast::<Option<ValueIdentity>>().add(side as usize) };
    *slot = input.identity();
    let identity = slot.as_ref().map_or(ptr::null(), |id| ptr::from_ref(id).cast());
    let intervals = input.intervals();
    CallBinding { present: 1, identity, intervals: intervals.as_ptr().cast(), count: intervals.len() }
}

#[repr(C)]
struct CallStorage {
    left: Slot,
    right: Slot,
    counts: Slot,
    output: Slot,
}

#[repr(C)]
struct CallHost {
    input: unsafe extern "C" fn(Slot, u32, Borrowed) -> CallBinding,
    interval: unsafe extern "C" fn(Borrowed, usize) -> Range,
    ranges: Output,
}

#[repr(C)]
struct CallResult {
    present: u32,
    has_ranges: u32,
    count: usize,
    left: CallOperand,
    right: CallOperand,
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
unsafe extern "C" fn pull<I:Iterator<Item=Pair>>(slot:Slot)->Next {
    unsafe{&mut *slot.cast::<I>()}.next().map(|v|Next{value:v.into(),present:1}).unwrap_or(Next{value:Range::default(),present:0})
}
unsafe extern "C" fn invoke<F: FnMut(Pair) -> ScalarRanges>(slot: Slot, input: Range, output: Slot) -> usize {
    let value = unsafe { &mut *slot.cast::<F>() }(input.into());
    unsafe { append_ranges(output, ptr::from_ref(&value.0).cast()) }
}
fn host() -> Output { Output { get, set, push } }
fn inputs<R>(left: &[Range], right: &[Range], run: impl FnOnce(Borrowed, usize, usize) -> R) -> R {
    let source = Sources { left: left.as_ptr(), right: right.as_ptr() };
    run(ptr::from_ref(&source).cast(), left.len(), right.len())
}
fn output(run: impl FnOnce(Slot)) -> ScalarRanges {
    let mut values = RangeStorage::empty();
    run(values.slot());
    ScalarRanges(values)
}
fn normalized_output(run: impl FnOnce(Slot, Output) -> usize) -> ScalarRanges {
    let mut values = RangeStorage::empty();
    let count = run(values.slot(), host());
    values.truncate(count);
    ScalarRanges(values)
}
unsafe extern "C" {
    #[link_name="bootstrap.scalar_fold::proof_range_record"]
    pub(super) fn read_record(records: Borrowed, index: usize) -> Range;
    #[link_name="bootstrap.scalar_fold::proof_range_side"]
    fn read(sides: Borrowed, side: u32, index: usize) -> Range;
    #[link_name="bootstrap.scalar_fold::proof_range_get"]
    fn get(storage: Slot, index: usize) -> Range;
    #[link_name="bootstrap.scalar_fold::proof_range_set"]
    fn set(storage: Slot, index: usize, value: Range);
    #[link_name="bootstrap.scalar_fold::proof_range_push"]
    fn push(storage: Slot, value: Range);
    #[link_name="bootstrap.scalar_fold::proof_range_truncate"]
    fn truncate(storage: Slot, count: usize);
    #[link_name="bootstrap.scalar_fold::proof_range_clone"]
    fn clone_ranges(storage: Borrowed, output: Slot);
    #[link_name="bootstrap.scalar_fold::proof_range_drop"]
    fn drop_ranges(storage: Slot);
    #[link_name="bootstrap.scalar_fold::proof_range_append"]
    fn append_ranges(storage: Slot, source: Borrowed) -> usize;
    #[link_name="bootstrap.scalar_fold::proof_range_collect"]
    fn collect_ranges(input: Slot, pull: Pull, output: Slot);
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
    fn counts(input: RangeInput, output: Slot, host: Output, width: u32, bias: u64) -> usize;
    #[link_name="bootstrap.scalar_fold::range_set_map"]
    fn mapped(input: RangeInput, operation: Slot, invoke: Invoke, output: Slot, host: Output) -> usize;
    #[link_name="bootstrap.scalar_fold::range_set_integer_binary"]
    fn integer_binary(left: RangeInput, right: RangeInput, right_counts: RangeInput,
        output: Slot, host: Output, proof: BinaryProof) -> BinaryResult;
    #[link_name="bootstrap.scalar_fold::project_integer_call"]
    fn project_call(context: Slot, operation: u32, scalar: ComputedType,
        left: CallOperand, right: CallOperand, with_ranges: u32, storage: CallStorage, host: CallHost) -> CallResult;
}
pub(super) fn full()->ScalarRanges {output(|o|unsafe{all(o,push)})}
pub(super) fn interval(lower:u64,upper:u64)->ScalarRanges {output(|o|unsafe{single(o,push,lower,upper)})}
pub(super) fn from_intervals<I: IntoIterator<Item = Pair>>(values: I) -> ScalarRanges {
    let mut stream = values.into_iter();
    output(|dest| unsafe { collect_ranges(ptr::from_mut(&mut stream).cast(), pull::<I::IntoIter>, dest) })
}
pub(super) fn from_sorted_intervals<I:IntoIterator<Item=Pair>>(values:I)->ScalarRanges {
    let mut stream=values.into_iter();output(|o|unsafe{sorted(ptr::from_mut(&mut stream).cast(),pull::<I::IntoIter>,o,push)})
}
pub(super) fn hull(values:&[Range])->Pair {inputs(values,&[],|s,n,_|unsafe{outer(s,n,read)}.into())}
pub(super) fn contains(values:&[Range],value:u64)->bool {inputs(values,&[],|s,n,_|unsafe{member(s,n,read,value)})}
pub(super) fn union(left:&[Range],right:&[Range])->ScalarRanges {inputs(left,right,|s,n,m|output(|o|unsafe{united(s,n,m,read,o,push)}))}
pub(super) fn intersection(left: &[Range], right: &[Range]) -> ScalarRanges {
    output(|dest| unsafe {
        scalar_ranges::intersect_ranges(RangeInput::records(left, read_record),
            RangeInput::records(right, read_record), dest, push);
    })
}
pub(super) fn restrict(values:&[Range],lower:u64,upper:u64)->ScalarRanges {inputs(values,&[],|s,n,_|output(|o|unsafe{clipped(s,n,read,o,push,lower,upper)}))}
pub(super) fn project(values: &[Range], projection: Projection) -> ScalarRanges {
    inputs(values, &[], |source, count, _| {
        output(|dest| unsafe { projected(source, count, read, dest, push, projection) })
    })
}
pub(super) fn project_contains(values: &[Range], projection: Projection, point: Bits) -> (ScalarRanges, bool) {
    let mut result = RangeStorage::empty();
    let contains = inputs(values, &[], |source, count, _| unsafe {
        projected_contains(source, count, read, result.slot(), push, projection, point)
    });
    (ScalarRanges(result), contains)
}
pub(super) fn restricted_hull(values: &[Range], (lower, upper): Pair, excluded: Option<Bits>) -> Pair {
    let projection = Projection::new(lower, upper, excluded);
    inputs(values, &[], |source, count, _| unsafe {
        projected_hull(source, count, read, projection).into()
    })
}
pub(super) fn covers_element(
    values: &[Range], scalar: Scalar, excluded: Option<Bits>, known: Pair, intervals: &[Pair], required: Pair,
) -> bool {
    let projection = Projection::new(0, scalar.bit_mask(), excluded);
    inputs(values, &[], |source, count, _| {
        let requirement = Requirement {
            order_bias: scalar.order_bias(),
            known: known.into(),
            required: required.into(),
            intervals: ptr::from_ref(&intervals).cast(),
            count: intervals.len(),
            interval: scalar_ranges::interval,
        };
        unsafe { covers_projected(source, count, read, projection, requirement) }
    })
}
pub(super) fn without(values:&[Range],value:u64)->ScalarRanges {inputs(values,&[],|s,n,_|output(|o|unsafe{excluded(s,n,read,o,push,value)}))}
pub(super) fn cast(values:&[Range],source:Scalar,target:Scalar,nonzero:bool)->ScalarRanges {
    let query = Cast::new(source, target, nonzero);
    inputs(values,&[],|s,n,_|normalized_output(|o,h|unsafe{converted(s,n,read,o,h,query)}))
}
pub(super) fn cast_chain(values: &[Range], [from, low, extended, to]: [Scalar; 4], nonzero: bool) -> ScalarRanges {
    let first = Cast::new(from, low, nonzero);
    let second = CastStep::new(low, extended);
    let last = CastStep::new(extended, to);
    inputs(values, &[], |source, count, _| {
        normalized_output(|out, host| unsafe {
            converted_chain(source, count, read, out, host, first, second, last)
        })
    })
}
pub(super) fn cast_summary(values: &[Range], [from, low, extended, to]: [Scalar; 4], nonzero: bool) -> (Pair, u32) {
    let first = Cast::new(from, low, nonzero);
    let second = CastStep::new(low, extended);
    let last = CastStep::new(extended, to);
    let summary = inputs(values, &[], |source, count, _| unsafe {
        converted_summary(source, count, read, first, second, last)
    });
    (summary.hull.into(), summary.contains)
}
pub(super) fn shift_counts(values: &[Range], scalar: Scalar) -> ScalarRanges {
    normalized_output(|output, host| unsafe {
        counts(RangeInput::records(values, read_record), output, host, scalar.bit_width(), scalar.order_bias())
    })
}

pub(super) fn computed_at_call<P: CallBindings>(
    op: IntegerOperation, scalar: Scalar, left: Operand, right: Operand, bindings: &P, with_ranges: bool,
) -> Option<(Option<IntegerExpression>, Option<ScalarRanges>)> {
    let mut input = CallInputs { bindings, identities: [None; 2] };
    let mut left_scratch = RangeStorage::empty();
    let mut right_scratch = RangeStorage::empty();
    let mut shift_scratch = RangeStorage::empty();
    let mut output = RangeStorage::empty();
    let storage = CallStorage {
        left: left_scratch.slot(),
        right: right_scratch.slot(),
        counts: shift_scratch.slot(),
        output: output.slot(),
    };
    let encoding = ComputedType {
        width: scalar.bit_width(), signed: u32::from(scalar.is_signed()), floating: u32::from(scalar.is_float()),
        mask: scalar.bit_mask(), bias: scalar.order_bias(),
    };
    let result = unsafe {
        project_call(ptr::from_mut(&mut input).cast(), op as u32, encoding,
            CallOperand::borrowed(&left), CallOperand::borrowed(&right), u32::from(with_ranges),
            storage, CallHost { input: call_input::<P>, interval: read_record, ranges: host() })
    };
    if result.present == 0 {
        return None;
    }
    let (a, b) = unsafe { (result.left.projected(left, scalar), result.right.projected(right, scalar)) };
    let expression = a.zip(b).map(|(a, b)| IntegerExpression::Binary(op, scalar, a, b));
    let ranges = (result.has_ranges != 0).then(|| {
        output.truncate(result.count);
        ScalarRanges(output)
    });
    Some((expression, ranges))
}

pub(super) fn binary(
    op: &str, scalar: Scalar, left: &ScalarRanges, right: &ScalarRanges, evidence: Option<BinaryEvidence>,
) -> (ScalarRanges, Origin) {
    let operation = BinaryOperation::from_operator(op);
    let projected;
    let left = left.0.as_slice();
    let right = right.0.as_slice();
    let right_counts = if matches!(operation, BinaryOperation::ShiftLeft | BinaryOperation::ShiftRight) {
        projected = shift_counts(right, scalar);
        projected.0.as_slice()
    } else {
        right
    };
    let right_scalar = evidence.as_ref().map_or(scalar, |facts| facts.right_scalar);
    let proof = BinaryProof {
        operation,
        width: scalar.bit_width(),
        signed: u32::from(scalar.is_signed()),
        same_value: u32::from(evidence.as_ref().is_some_and(|facts| facts.same_value)),
        bias: scalar.order_bias(),
        mask: scalar.bit_mask(),
        minimum_difference: evidence.as_ref().map_or(0, |facts| u64::from(facts.minimum_difference)),
        right_bias: right_scalar.order_bias(),
        right_width: right_scalar.bit_width(),
        zero_quotient: u32::from(evidence.as_ref().is_some_and(|facts| facts.zero_quotient)),
        track_origin: u32::from(evidence.is_some()),
    };
    let mut output = RangeStorage::empty();
    let result = unsafe {
        integer_binary(
            RangeInput::records(left, read_record), RangeInput::records(right, read_record),
            RangeInput::records(right_counts, read_record), output.slot(), host(), proof,
        )
    };
    output.truncate(result.count);
    (ScalarRanges(output), result.origin)
}
pub(super) fn map<F: FnMut(Pair) -> ScalarRanges>(values: &[Range], mut operation: F) -> ScalarRanges {
    normalized_output(|output, host| unsafe {
        mapped(RangeInput::records(values, read_record), ptr::from_mut(&mut operation).cast(), invoke::<F>, output, host)
    })
}
