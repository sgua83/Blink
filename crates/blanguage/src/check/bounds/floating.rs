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

impl Proof {
    pub(in crate::check) fn texture_sample(
        format: Option<crate::ast::TextureFormat>, coordinate: &Value, lod: &Value,
    ) -> Self {
        use crate::ast::TextureFormat::*;
        if !matches!(format, Some(Rgba8Unorm | Rgba8Srgb | R8Unorm | Rg16Unorm | Rgb10A2Unorm)) {
            return Self::default();
        }
        // 当前共同证明覆盖所选原资源尺寸(u32)缩放后仍在原生最低数值范围内的输入。
        // 事实不足只保持未知，不改变合法采样域或生成运行检查。
        let coordinates = (0..coordinate.regs.len()).all(|i|
            finite(coordinate, i).is_some_and(|(a,b)| a >= -1.0 && b <= 1.0));
        let lod = finite(lod, 0).is_some_and(|(a,b)| a >= -4294967296.0 && b <= 4294967296.0);
        if !coordinates || !lod { return Self::default(); }
        // 原固定LINEAR过滤为非负权重组合。宽界包含CPU lerp及Vulkan过滤舍入，
        // 不将理想UNORM [0,1]冒充严格最终范围，也不扩大原采样精度许可。
        Self { ranges: vec![interval(Scalar::F32, 0.0, 2.0); 4], ..Self::default() }
    }
}

// Outward F64 evaluation belongs to the existing F32 analysis, not generated code.
fn add_up(a: f64, b: f64) -> f64 {
    let sum = a + b; let back = sum - a;
    let tail = (a - (sum - back)) + (b - back);
    if tail > 0.0 { sum.next_up() } else { sum }
}
fn multiply_up(a: f64, b: f64) -> f64 {
    let product = a * b;
    if a == 0.0 || b == 0.0 { product } else { product.next_up() }
}
fn divide_up(a: f64, b: f64) -> f64 {
    let quotient = a / b;
    if a == 0.0 { quotient } else { quotient.next_up() }
}
fn integer_bound(value: Bits, lower: bool) -> f64 {
    let result = value as f64;
    if lower && result as u128 > value as u128 { result.next_down() }
    else if !lower && (result as u128) < value as u128 { result.next_up() }
    else { result }
}
fn conversion_error(upper: Bits) -> f64 {
    if upper <= 1 << 24 { 0.0 } else { 2.0f64.powi(-23) }
}
impl FloatingBound {
    fn coefficients(self) -> [f64; 3] { self.terms.map(f64::from_bits) }
    fn upper(self, terms: [f64; 3]) -> Self {
        Self { terms: terms.map(f64::to_bits), form: FloatingForm::Upper, ..self }
    }
    fn scaled(self, factor: f64, extra: f64) -> Self {
        let [a,b,c] = self.coefficients();
        self.upper([multiply_up(a,factor),add_up(multiply_up(b,factor),extra),multiply_up(c,factor)])
    }
    fn rounded(self) -> Self { self.scaled(1.0 + 2.0f64.powi(-23),2.0f64.powi(-149)) }
    fn common(self, other: Self) -> Option<Self> {
        if self.source != other.source || self.scalar != other.scalar { return None; }
        let bound = Self { lower: self.lower.max(other.lower), upper: self.upper.min(other.upper), ..self };
        (bound.lower <= bound.upper).then_some(bound)
    }
    fn maximum(self) -> f64 {
        let [a,b,c] = self.coefficients();
        // For positive d: c >= 0 is convex; c < 0 with a >= 0 is increasing.
        [integer_bound(self.lower,true),integer_bound(self.upper,false)].into_iter().map(|d|
            add_up(add_up(multiply_up(a,d),b),if c == 0.0 { 0.0 } else { divide_up(c,d) }))
            .fold(f64::NEG_INFINITY,f64::max)
    }
    fn restricted(self, input: &Proof) -> Option<Self> {
        let (lower,upper) = input.range_set(0).restrict(0,self.scalar.bit_mask()).hull();
        let bound = Self { lower: self.lower.max(lower), upper: self.upper.min(upper), ..self };
        (bound.lower <= bound.upper).then_some(bound)
    }
    pub(super) fn at_call(self, bindings: &BTreeMap<ValueIdentity,Proof>) -> Option<Self> {
        let input = bindings.get(&self.source)?;
        Some(Self { source: input.known_identity(0)?, ..self.restricted(input)? })
    }
    pub(super) fn range_at_call(self, bindings: &BTreeMap<ValueIdentity,Proof>) -> Option<ScalarRanges> {
        let bound = self.restricted(bindings.get(&self.source)?)?;
        (bound.form == FloatingForm::Upper).then(|| upper_range(bound.maximum()))
    }
}
fn upper_range(upper: f64) -> ScalarRanges {
    interval(Scalar::F32,-(f32::MAX as f64),upper)
}
pub(super) fn cast_domains(value: &Value, component: usize) -> Domains {
    let scalar = value.ty.components[component].0;
    if !scalar.is_integer() || scalar.is_signed() { return Domains::new(); }
    let (lower,upper) = value.component_ranges(component).hull();
    if lower > upper { return Domains::new(); }
    let cast = FloatingBound { source: value.component_identity(component), lower, upper,
        terms: [0;3], scalar, form: FloatingForm::Cast };
    let mut domains = Domains::from([IndexDomain::Floating(cast)]);
    for domain in value.proof.domains(component) {
        let (source,distance) = match domain {
            IndexDomain::Computed(IntegerExpression::Binary(IntegerOperation::Remainder,from,_,Operand::Value(divisor))) if from == scalar => (divisor,1),
            IndexDomain::Upper(limit,distance) => (limit,distance),
            _ => continue,
        };
        let factor = 1.0 + conversion_error(upper);
        let bound = FloatingBound { source, lower: Bits::from(distance), upper: scalar.bit_mask(), ..cast }
            .upper([factor,multiply_up(-(distance as f64),factor),0.0]);
        domains.insert(IndexDomain::Floating(bound));
    }
    domains
}
pub(super) fn arithmetic_domains(op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let (ai,bi) = (component % a.regs.len(),component % b.regs.len());
    let Some((al,ah)) = finite(a,ai) else { return Domains::new(); };
    let Some((bl,bh)) = finite(b,bi) else { return Domains::new(); };
    let mut domains = Domains::new();
    for (value,at,other_min,other_max) in [(a,ai,bl,bh),(b,bi,al,ah)] {
        for domain in value.proof.domains(at) {
            let IndexDomain::Floating(bound) = domain else { continue; };
            if bound.form != FloatingForm::Upper { continue; }
            let [x,y,z] = bound.coefficients();
            let result = match op {
                "+" if al >= 0.0 && bl >= 0.0 => Some(bound.upper([x,add_up(y,other_max),z]).rounded()),
                "*" if other_min >= 0.0 && other_min == other_max && finite(value,at).is_some_and(|(low,_)| low >= 0.0) => Some(bound.scaled(other_max,0.0).rounded()),
                _ => None,
            };
            if let Some(result) = result { domains.insert(IndexDomain::Floating(result)); }
        }
    }
    if op == "/" && al >= 0.0 && bl > 0.0 {
        for domain in b.proof.domains(bi) {
            let IndexDomain::Floating(divisor) = domain else { continue; };
            if divisor.form != FloatingForm::Cast || divisor.lower == 0 { continue; }
            let factor = divide_up(1.0 + 3.0*2.0f64.powi(-23),1.0-conversion_error(divisor.upper));
            domains.insert(IndexDomain::Floating(divisor.upper([0.0,3.0*2.0f64.powi(-149),multiply_up(ah,factor)])));
            for numerator in a.proof.domains(ai) {
                let IndexDomain::Floating(numerator) = numerator else { continue; };
                if numerator.form != FloatingForm::Upper { continue; }
                let [x,y,z] = numerator.coefficients();
                if z != 0.0 { continue; }
                if let Some(common) = numerator.common(divisor) {
                    domains.insert(IndexDomain::Floating(common.upper([0.0,add_up(multiply_up(x,factor),3.0*2.0f64.powi(-149)),multiply_up(y,factor)])));
                }
            }
        }
    }
    if op == "*" && al >= 0.0 && bl >= 0.0 {
        for (left,li,right,ri) in [(a,ai,b,bi),(b,bi,a,ai)] {
            for left in left.proof.domains(li) {
                let IndexDomain::Floating(left) = left else { continue; };
                if left.form != FloatingForm::Upper { continue; }
                let [a,b,c] = left.coefficients();
                if c != 0.0 { continue; }
                for right in right.proof.domains(ri) {
                    let IndexDomain::Floating(right) = right else { continue; };
                    if right.form != FloatingForm::Upper { continue; }
                    let [x,y,z] = right.coefficients();
                    if x != 0.0 || y < 0.0 { continue; }
                    if let Some(common) = left.common(right) {
                        let result = common.upper([multiply_up(a,y),add_up(multiply_up(a,z),multiply_up(b,y)),multiply_up(b,z)]).rounded();
                        domains.insert(IndexDomain::Floating(result));
                    }
                }
            }
        }
    }
    // A finite upper fact must not conceal an overflowing intermediate result.
    domains.retain(|domain| match domain {
        IndexDomain::Floating(bound) => bound.maximum().is_finite() && bound.maximum() <= f32::MAX as f64,
        _ => true,
    });
    domains
}
pub(super) fn refine(proof: &mut Proof) {
    for (component,domains) in proof.domains.iter().enumerate() {
        for domain in domains {
            if let IndexDomain::Floating(bound) = domain && bound.form == FloatingForm::Upper {
                let range = upper_range(bound.maximum());
                proof.ranges[component] = proof.ranges[component].combine(&range,
                    |(al,ah),(bl,bh)| ScalarRanges::interval(al.max(bl),ah.min(bh)));
            }
        }
    }
}
