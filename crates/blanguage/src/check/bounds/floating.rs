//! 同一Proof区间集合中的有限浮点次序；不拥有运行资源或另一份事实表。
use super::*;

fn order(scalar: Scalar, bits: Bits) -> Bits {
    let sign = 1 << (scalar.bit_width() - 1);
    if bits & sign == 0 { bits ^ sign } else { !bits & scalar.bit_mask() }
}
pub(super) fn bits(scalar: Scalar, ordered: Bits) -> Bits {
    let sign = 1 << (scalar.bit_width() - 1);
    if ordered & sign == 0 { !ordered & scalar.bit_mask() } else { ordered ^ sign }
}
fn number(scalar: Scalar, ordered: Bits) -> f64 {
    let raw = bits(scalar, ordered);
    if scalar == Scalar::F32 { f32::from_bits(raw as u32) as f64 } else { f64::from_bits(raw) }
}
pub(super) fn ranges(value: &Value, component: usize) -> ScalarRanges {
    let scalar = value.ty.components[component].0;
    let ranges = value.proof.range_set(component).restrict(0, scalar.bit_mask());
    match &value.constant {
        Some(values) => { let at = order(scalar, values[component]); ranges.restrict(at, at) }
        None => ranges,
    }
}
fn finite(value: &Value, component: usize) -> Option<(f64, f64)> {
    let scalar = value.ty.components[component].0;
    let (a,b) = ranges(value, component).hull();
    if a > b { return None; }
    let (mut lower, mut upper) = (number(scalar,a), number(scalar,b));
    if !lower.is_finite() || !upper.is_finite() { return None; }
    // 原GPU入口允许次正规输入及结果归零；数值区间包含这一实际可能值。
    let normal = if scalar == Scalar::F32 { f32::MIN_POSITIVE as f64 } else { f64::MIN_POSITIVE };
    if lower > 0.0 && lower < normal { lower = 0.0; }
    if upper < 0.0 && upper > -normal { upper = 0.0; }
    Some((lower,upper))
}
fn rounded(scalar: Scalar, value: f64, lower: bool) -> f64 {
    if scalar == Scalar::F64 { return value; }
    let result = value as f32;
    if lower && result as f64 > value { result.next_down() as f64 }
    else if !lower && (result as f64) < value { result.next_up() as f64 }
    else { result as f64 }
}
// F32加减的精确和由F64主值与残差交接；很小的第二项不能被宿主舍入抹去。
fn sum_bound(a: f64, b: f64, lower: bool) -> f64 {
    let sum = a + b;
    let back = sum - a;
    let tail = (a - (sum - back)) + (b - back);
    let value = rounded(Scalar::F32,sum,lower);
    if value == sum && (lower && tail < 0.0 || !lower && tail > 0.0) {
        if lower { (value as f32).next_down() as f64 } else { (value as f32).next_up() as f64 }
    } else { value }
}
fn interval(scalar: Scalar, lower: f64, upper: f64) -> ScalarRanges {
    let (mut lower, mut upper) = (rounded(scalar,lower,true), rounded(scalar,upper,false));
    if !lower.is_finite() || !upper.is_finite() || lower > upper { return ScalarRanges::default(); }
    let normal = if scalar == Scalar::F32 { f32::MIN_POSITIVE as f64 } else { f64::MIN_POSITIVE };
    if lower > 0.0 && lower < normal { lower = 0.0; }
    if upper < 0.0 && upper > -normal { upper = 0.0; }
    if lower == 0.0 { lower = -0.0; }
    if upper == 0.0 { upper = 0.0; }
    let raw = |x:f64| if scalar == Scalar::F32 { (x as f32).to_bits() as Bits } else { x.to_bits() };
    ScalarRanges::interval(order(scalar,raw(lower)),order(scalar,raw(upper)))
}
pub(super) fn cast(value: &Value, component: usize, target: Scalar) -> ScalarRanges {
    let source = value.ty.components[component].0;
    if source.is_integer() && target.is_float() {
        let (a,b) = value.component_ranges(component).hull();
        if a > b { return ScalarRanges(Vec::new()); }
        let decode = |n| if source.is_signed() { source.signed(n ^ source.order_bias()) as i128 } else { n as i128 };
        let (lower,upper) = (decode(a), decode(b));
        let convert = |n:i128, low:bool| {
            let f = if target == Scalar::F32 { n as f32 as f64 } else { n as f64 };
            if low && f as i128 > n {
                if target == Scalar::F32 { (f as f32).next_down() as f64 } else { f.next_down() }
            } else if !low && (f as i128) < n {
                if target == Scalar::F32 { (f as f32).next_up() as f64 } else { f.next_up() }
            } else { f }
        };
        return interval(target,convert(lower,true),convert(upper,false));
    }
    if source.is_float() && let Some((lower,upper)) = finite(value,component) {
        if target.is_float() { return interval(target,lower,upper); }
        if target.is_integer() {
            let (lower,upper) = (lower.trunc() as i128,upper.trunc() as i128);
            let minimum = if target.is_signed() { -(1i128 << (target.bit_width()-1)) } else { 0 };
            let maximum = if target.is_signed() { (1i128 << (target.bit_width()-1))-1 } else { (1i128 << target.bit_width())-1 };
            if lower >= minimum && upper <= maximum {
                return ScalarRanges::interval(target.truncate(lower as Bits)^target.order_bias(),target.truncate(upper as Bits)^target.order_bias());
            }
        }
    }
    ScalarRanges::default()
}
pub(super) fn arithmetic(op: &str, a: &Value, b: &Value, component: usize) -> ScalarRanges {
    let scalar = a.ty.components[0].0;
    let Some((al,ah)) = finite(a,component % a.regs.len()) else { return ScalarRanges::default(); };
    let Some((bl,bh)) = finite(b,component % b.regs.len()) else { return ScalarRanges::default(); };
    // 现行共同范围只分析F32四则；F64实际目标精度不凭宿主F64运算推定。
    if scalar != Scalar::F32 { return ScalarRanges::default(); }
    let (mut lower,mut upper) = match op {
        "+" => (sum_bound(al,bl,true),sum_bound(ah,bh,false)),
        "-" => (sum_bound(al,-bh,true),sum_bound(ah,-bl,false)),
        "*" => { let values=[al*bl,al*bh,ah*bl,ah*bh]; (values.into_iter().fold(f64::INFINITY,f64::min),values.into_iter().fold(f64::NEG_INFINITY,f64::max)) }
        "/" => {
            // Vulkan OpFDiv的2.5 ULP保证只覆盖原规格的分母范围。
            if bl <= 0.0 && bh >= 0.0 || bl.abs().min(bh.abs()) < f32::MIN_POSITIVE as f64
                || bl.abs().max(bh.abs()) > 2.0f64.powi(126) { return ScalarRanges::default(); }
            let values=[al/bl,al/bh,ah/bl,ah/bh];
            (values.into_iter().fold(f64::INFINITY,f64::min),values.into_iter().fold(f64::NEG_INFINITY,f64::max))
        }
        _ => return ScalarRanges::default(),
    };
    if op == "/" {
        // 2.5 ULP <= 2.5*2^-23相对误差；3覆盖端点F64求值及最终向外舍入。
        let error = |x:f64| 3.0*(x.abs()*2.0f64.powi(-23)).max(2.0f64.powi(-149));
        lower -= error(lower); upper += error(upper);
    }
    interval(scalar,lower,upper)
}
pub(super) fn math(op: MathOp, args: &[Value], component: usize) -> ScalarRanges {
    let a = &args[0]; let scalar = a.ty.components[0].0;
    let Some((al,ah)) = finite(a,component % a.regs.len()) else { return ScalarRanges::default(); };
    let (lower,upper) = match op {
        MathOp::Min | MathOp::Max => {
            let b=&args[1];
            let Some((bl,bh))=finite(b,component % b.regs.len()) else { return ScalarRanges::default(); };
            if op == MathOp::Min { (al.min(bl),ah.min(bh)) } else { (al.max(bl),ah.max(bh)) }
        }
        MathOp::Abs => (if al<=0.0 && ah>=0.0 { 0.0 } else { al.abs().min(ah.abs()) },al.abs().max(ah.abs())),
        MathOp::Floor => (al.floor(),ah.floor()),
        MathOp::Log | MathOp::Log2 if al > 0.0 => {
            // 正有限输入的对数仍有限；不以宿主数学库预测设备的精确对数区间。
            let maximum=if scalar == Scalar::F32 { f32::MAX as f64 } else { f64::MAX };
            (-maximum,maximum)
        }
        _ => return ScalarRanges::default(),
    };
    interval(scalar,lower,upper)
}
pub(super) fn negate(value: &Value, component: usize) -> ScalarRanges {
    match finite(value,component) {
        Some((lower,upper)) => interval(value.ty.components[component].0,-upper,-lower),
        None => ScalarRanges::default(),
    }
}
