//! 同一Proof区间集合中的有限浮点次序；不拥有运行资源或另一份事实表。
use super::*;
use crate::ir::scalar_ranges::RangeInput;
#[path="floating/scalar.rs"]
mod scalar;
pub(super) use scalar::{order,bits,comparison_possible,finite,interval,bitcast,cast,arithmetic,math,sum,negate,add_up,multiply_up,divide_up,integer_bound,conversion_error};

unsafe extern "C" {
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
        // Four linear dimensions (3D plus mip) have 16 nonnegative texel terms.
        // Each contribution has at most 25 rounding factors, including conversions.
        // Use four decimal digits, wider than Vulkan's minimum about 1 part in 10^5.
        // CPU affine UNORM decode and four three-operation lerps fit inside this bound.
        let upper = if matches!(format, Some(Rgba8Srgb)) { 2.0 } else {
            let factor = add_up(1.0, 1.0e-4);
            (0..25).fold(1.0, |value, _| multiply_up(value, factor))
        };
        Self { ranges: vec![interval(Scalar::F32, 0.0, upper); 4], ..Self::default() }
    }
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
    fn restricted(self, input: CallInput<'_>) -> Option<Self> {
        let (lower,upper) = range_sets::restrict(input.intervals(), 0, self.scalar.bit_mask()).hull();
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
pub(super) fn cast_domains(value: &Value, component: usize) -> Domains {
    let scalar = value.ty.components[component].0;
    if !scalar.is_integer() || scalar.is_signed() { return Domains::new(); }
    let (lower,upper) = value.component_ranges(component).hull();
    if lower > upper { return Domains::new(); }
    let cast = FloatingBound { source: value.component_identity(component), lower, upper,
        terms: [0;3], scalar, form: FloatingForm::Cast };
    let mut domains = Domains::from([IndexDomain::Floating(cast)]);
    for domain in value.proof.domains(component).iter().copied() {
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
pub(super) fn arithmetic_domains(builder: &Builder<'_>, op: &str, a: &Value, b: &Value, component: usize) -> Domains {
    let (ai,bi) = (component % a.regs.len(),component % b.regs.len());
    let Some((al,ah)) = finite(a,ai) else { return Domains::new(); };
    let Some((bl,bh)) = finite(b,bi) else { return Domains::new(); };
    let mut domains = Domains::new();
    for (value,at,other_min,other_max) in [(a,ai,bl,bh),(b,bi,al,ah)] {
        for domain in value.proof.domains(at).iter().copied() {
            let IndexDomain::Floating(bound) = domain else { continue; };
            let bound = if bound.form == FloatingForm::Cast && op == "+" {
                bound.upper([1.0 + conversion_error(bound.upper), 0.0, 0.0])
            } else { bound };
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
        for domain in b.proof.domains(bi).iter().copied() {
            let IndexDomain::Floating(divisor) = domain else { continue; };
            if divisor.form != FloatingForm::Cast || divisor.lower == 0 { continue; }
            let factor = divide_up(1.0 + 3.0*2.0f64.powi(-23),1.0-conversion_error(divisor.upper));
            domains.insert(IndexDomain::Floating(divisor.upper([0.0,3.0*2.0f64.powi(-149),multiply_up(ah,factor)])));
            for numerator in a.proof.domains(ai).iter().copied() {
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
            for left in left.proof.domains(li).iter().copied() {
                let IndexDomain::Floating(left) = left else { continue; };
                if left.form != FloatingForm::Upper { continue; }
                let [a,b,c] = left.coefficients();
                if c != 0.0 { continue; }
                for right in right.proof.domains(ri).iter().copied() {
                    let IndexDomain::Floating(right) = right else { continue; };
                    if right.form != FloatingForm::Upper { continue; }
                    let [x,y,z] = right.coefficients();
                    if x != 0.0 || y < 0.0 { continue; }
                    let mut left = left;
                    let mut b = b;
                    if left.source != right.source && left.scalar == right.scalar && a >= 0.0 && z != 0.0
                        && builder.integer_below(left.source, right.source, left.scalar)
                    {
                        b = add_up(b, -a);
                        left = FloatingBound { source: right.source, lower: right.lower, upper: right.upper, ..left }.upper([a,b,0.0]);
                    }
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
                proof.ranges[component] = proof.ranges[component].intersection(&range);
            }
        }
    }
}
