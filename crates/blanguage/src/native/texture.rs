//! CPU 采样降低为原生标量 IR；每次取样不调用宿主纹理函数。
use super::*;
use allocation::Location;
use operands::MoveLocation;

#[repr(u32)]
pub(super) enum TextureAccessKind {
    Binding,
    Levels,
    Address,
    Format,
    Size,
    Byte,
    Word,
    Rgba,
}

#[repr(C)]
struct TextureAccess {
    base: MoveLocation,
    element: MoveLocation,
    element_scalar: ScalarEncoding,
    level: MoveLocation,
    coordinates: [MoveLocation; 3],
    in_table: u32,
    stride: u32,
    kind: TextureAccessKind,
    dimensions: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::texture_access"]
    fn encode_texture_access(access: TextureAccess, results: *const MoveLocation,
        count: usize, output: *mut u8) -> u64;
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum TextureColor { Rgba8 = 1, Srgb, R8, Rg16, Rgb10A2, Rg11B10 }

#[allow(dead_code)]
#[repr(u32)]
enum ColorScalar { Word, Float, Bool }

#[allow(dead_code)]
#[repr(u32)]
enum ColorOp {
    Constant, Cast, Bitcast, Add, Divide, Multiply, LessEqual, Equal,
    ShiftRight, ShiftLeft, And, Or, Power, Select, Into, Set,
}

#[repr(C)]
struct ColorHost {
    context: *mut std::ffi::c_void,
    instruction: unsafe extern "C" fn(*mut std::ffi::c_void, ColorOp, ColorScalar, Reg, Reg, Reg) -> Reg,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::texture_color"]
    fn lower_texture_color(format: TextureColor, raw: Reg, result: *const Reg, host: ColorHost);
}

unsafe extern "C" fn color_instruction(
    context: *mut std::ffi::c_void, operation: ColorOp, scalar: ColorScalar, a: Reg, b: Reg, c: Reg,
) -> Reg {
    let lower = unsafe { &mut *context.cast::<Lower<'_>>() };
    let scalar = match scalar {
        ColorScalar::Word => Scalar::U32,
        ColorScalar::Float => Scalar::F32,
        ColorScalar::Bool => Scalar::Bool,
    };
    match operation {
        ColorOp::Constant => lower.constant(scalar, a as u32),
        ColorOp::Cast => lower.cast(scalar, a),
        ColorOp::Power => lower.math(MathOp::Pow, a, b),
        ColorOp::Select => lower.select(scalar, a, b, c),
        ColorOp::Bitcast => {
            let dst = lower.reg(scalar);
            lower.code.push(Inst::generated(InstKind::Unary {
                dst, op: UnaryOp::Bitcast, a,
            }, lower.origin.clone()));
            dst
        }
        ColorOp::Into => {
            lower.code.push(Inst::generated(InstKind::Cast { dst: a, src: b }, lower.origin.clone()));
            a
        }
        ColorOp::Set => {
            lower.code.push(Inst::generated(InstKind::Constant(a, scalar, Bits::from(b as u32)), lower.origin.clone()));
            a
        }
        ColorOp::Add => lower.binary(scalar, if scalar == Scalar::F32 { BinaryOp::FAdd } else { BinaryOp::IAdd }, a, b),
        ColorOp::Divide => lower.binary(scalar, BinaryOp::FDiv, a, b),
        ColorOp::Multiply => lower.binary(scalar, BinaryOp::FMul, a, b),
        ColorOp::LessEqual => lower.binary(scalar, BinaryOp::Compare(Comparison::LessEqual), a, b),
        ColorOp::Equal => lower.binary(scalar, BinaryOp::Compare(Comparison::Equal), a, b),
        ColorOp::ShiftRight => lower.binary(scalar, BinaryOp::ShiftRightLogical, a, b),
        ColorOp::ShiftLeft => lower.binary(scalar, BinaryOp::ShiftLeft, a, b),
        ColorOp::And => lower.binary(scalar, BinaryOp::And, a, b),
        ColorOp::Or => lower.binary(scalar, BinaryOp::Or, a, b),
    }
}

struct Lower<'a> {
    registers: &'a mut Vec<Scalar>,
    origin: std::sync::Arc<GeneratedOrigin>,
    code: Vec<Inst>,
}
impl Lower<'_> {
    fn reg(&mut self, s: Scalar) -> Reg {
        let r = self.registers.len();
        self.registers.push(s);
        r
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> Reg {
        let r = self.reg(s);
        self.code.push(Inst::generated(InstKind::Constant(r, s, Bits::from(bits)), self.origin.clone()));
        r
    }
    fn binary(&mut self, s: Scalar, op: BinaryOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::generated(InstKind::Binary { dst, op, a, b }, self.origin.clone()));
        dst
    }
    fn cast(&mut self, s: Scalar, src: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::generated(InstKind::Cast { dst, src }, self.origin.clone()));
        dst
    }
    fn math(&mut self, op: MathOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(Scalar::F32);
        self.code.push(Inst::generated(InstKind::Math {
            dst,
            op,
            args: vec![a, b],
        }, self.origin.clone()));
        dst
    }
    fn min_u(&mut self, a: Reg, b: Reg) -> Reg {
        let condition = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Less), a, b);
        let dst = self.reg(self.registers[a]);
        self.code.push(Inst::generated(InstKind::Select {
            dst,
            condition,
            a,
            b,
        }, self.origin.clone()));
        dst
    }
    fn lerp(&mut self, a: Reg, b: Reg, t: Reg) -> Reg {
        let difference = self.binary(Scalar::F32, BinaryOp::FSub, b, a);
        let scaled = self.binary(Scalar::F32, BinaryOp::FMul, difference, t);
        self.binary(Scalar::F32, BinaryOp::FAdd, a, scaled)
    }
    fn select(&mut self, s: Scalar, condition: Reg, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::generated(InstKind::Select {
            dst,
            condition,
            a,
            b,
        }, self.origin.clone()));
        dst
    }
    fn color(&mut self, format: TextureColor, raw: Reg, result: &[Reg; 4]) {
        let host = ColorHost { context: std::ptr::from_mut(self).cast(), instruction: color_instruction };
        unsafe { lower_texture_color(format, raw, result.as_ptr(), host); }
    }
    fn texel(
        &mut self,
        texture: TextureSource,
        coordinate: &[Reg],
        level: Reg,
        format: Reg,
    ) -> [Reg; 4] {
        let result = std::array::from_fn(|_| self.reg(Scalar::F32));
        let zero_format = self.constant(Scalar::U32, 0);
        let floating = self.binary(
            Scalar::Bool,
            BinaryOp::Compare(Comparison::Equal),
            format,
            zero_format,
        );
        let outer = std::mem::take(&mut self.code);
        self.code.push(Inst::generated(InstKind::TextureFetch {
            result,
            texture,
            coordinate: coordinate.to_vec(),
            level,
        }, self.origin.clone()));
        let floats = std::mem::take(&mut self.code);
        let byte_format = self.constant(Scalar::U32, 3);
        let single = self.binary(
            Scalar::Bool,
            BinaryOp::Compare(Comparison::Equal),
            format,
            byte_format,
        );
        let packed_outer = std::mem::take(&mut self.code);
        let raw = self.reg(Scalar::U32);
        self.code.push(Inst::generated(InstKind::TextureRead {
            dst: raw,
            texture,
            coordinate: coordinate.to_vec(),
            level,
            byte: true,
        }, self.origin.clone()));
        self.color(TextureColor::R8, raw, &result);
        let singles = std::mem::take(&mut self.code);
        let raw = self.reg(Scalar::U32);
        self.code.push(Inst::generated(InstKind::TextureRead {
            dst: raw,
            texture,
            coordinate: coordinate.to_vec(),
            level,
            byte: false,
        }, self.origin.clone()));
        let unpack_outer = std::mem::take(&mut self.code);
        let mut rgba = Vec::new();
        for color in [TextureColor::Rgba8, TextureColor::Rg16, TextureColor::Rgb10A2, TextureColor::Rg11B10] {
            self.color(color, raw, &result);
            let tag = color as u32;
            let unpacked = std::mem::take(&mut self.code);
            if tag == 1 { rgba = unpacked; } else {
                let packed_format = self.constant(Scalar::U32, tag);
                let packed = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Equal), format, packed_format);
                self.code.push(Inst::generated(InstKind::If { condition: packed, yes: unpacked, no: rgba }, self.origin.clone()));
                rgba = std::mem::take(&mut self.code);
            }
        }
        self.code = unpack_outer;
        self.code.extend(rgba);
        let srgb_format = self.constant(Scalar::U32, 2);
        let encoded = self.binary(
            Scalar::Bool,
            BinaryOp::Compare(Comparison::Equal),
            format,
            srgb_format,
        );
        let rgba = std::mem::take(&mut self.code);
        self.color(TextureColor::Srgb, 0, &result);
        let decode = std::mem::replace(&mut self.code, rgba);
        self.code.push(Inst::generated(InstKind::If {
            condition: encoded,
            yes: decode,
            no: Vec::new(),
        }, self.origin.clone()));
        let rgba = std::mem::replace(&mut self.code, packed_outer);
        self.code.push(Inst::generated(InstKind::If {
            condition: single,
            yes: singles,
            no: rgba,
        }, self.origin.clone()));
        let packed = std::mem::replace(&mut self.code, outer);
        self.code.push(Inst::generated(InstKind::If {
            condition: floating,
            yes: floats,
            no: packed,
        }, self.origin.clone()));
        result
    }
    fn level_sample(
        &mut self,
        texture: TextureSource,
        coordinate: &[Reg],
        level: Reg,
        address: Reg,
        format: Reg,
    ) -> [Reg; 4] {
        let dimensions: Vec<_> = coordinate.iter().map(|_| self.reg(Scalar::U32)).collect();
        self.code.push(Inst::generated(InstKind::TextureSize {
            result: dimensions.clone(), texture, level,
        }, self.origin.clone()));
        let one = self.constant(Scalar::I64, 1);
        let zero_i = self.constant(Scalar::I64, 0);
        let zero_u = self.constant(Scalar::U32, 0);
        let zero = self.constant(Scalar::F32, 0);
        let half = self.constant(Scalar::F32, 0.5f32.to_bits());
        let mut first = Vec::new();
        let mut next = Vec::new();
        let mut fraction = Vec::new();
        for axis in 0..coordinate.len() {
            // u32资源尺寸在64位地址域中运算；f32舍入不能制造越界texel。
            let size = self.cast(Scalar::I64, dimensions[axis]);
            let last = self.binary(Scalar::I64, BinaryOp::ISub, size, one);
            let size_float = self.cast(Scalar::F32, dimensions[axis]);
            let scaled = self.binary(Scalar::F32, BinaryOp::FMul, coordinate[axis], size_float);
            let centered = self.binary(Scalar::F32, BinaryOp::FSub, scaled, half);
            let positive = self.math(MathOp::Max, centered, zero);
            let at = self.math(MathOp::Min, positive, size_float);
            let raw = self.cast(Scalar::I64, at);
            let clamped = self.min_u(raw, last);
            let base = self.cast(Scalar::F32, raw);
            let weight_clamped = self.binary(Scalar::F32, BinaryOp::FSub, at, base);
            let adjacent = self.binary(Scalar::I64, BinaryOp::IAdd, raw, one);
            let clamped_next = self.min_u(adjacent, last);
            // 归一坐标先周期化，各真实tap分别寻址，跨边界共同参与过滤。
            let floor = self.math(MathOp::Floor, coordinate[axis], coordinate[axis]);
            let unit = self.binary(Scalar::F32, BinaryOp::FSub, coordinate[axis], floor);
            let scaled = self.binary(Scalar::F32, BinaryOp::FMul, unit, size_float);
            let centered = self.binary(Scalar::F32, BinaryOp::FSub, scaled, half);
            let base = self.math(MathOp::Floor, centered, centered);
            let index = self.cast(Scalar::I64, base);
            let negative = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Less), index, zero_i);
            let at_limit = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::GreaterEqual), index, size);
            let reduced = self.binary(Scalar::I64, BinaryOp::ISub, index, size);
            let wrapped = self.select(Scalar::I64, at_limit, reduced, index);
            let wrapped = self.select(Scalar::I64, negative, last, wrapped);
            let next_index = self.binary(Scalar::I64, BinaryOp::IAdd, wrapped, one);
            let at_end = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Equal), wrapped, last);
            let wrapped_next = self.select(Scalar::I64, at_end, zero_i, next_index);
            let weight = self.binary(Scalar::F32, BinaryOp::FSub, centered, base);
            let bit = self.constant(Scalar::U32, 1 << axis);
            let flag = self.binary(Scalar::U32, BinaryOp::And, address, bit);
            let repeat = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::NotEqual), flag, zero_u);
            first.push(self.select(Scalar::I64, repeat, wrapped, clamped));
            next.push(self.select(Scalar::I64, repeat, wrapped_next, clamped_next));
            fraction.push(self.select(Scalar::F32, repeat, weight, weight_clamped));
        }
        let mut taps = Vec::new();
        for mask in 0..1 << coordinate.len() {
            let at: Vec<_> = (0..coordinate.len()).map(|axis|
                if mask & (1 << axis) == 0 { first[axis] } else { next[axis] }).collect();
            taps.push(self.texel(texture, &at, level, format));
        }
        for weight in fraction {
            taps = taps.chunks_exact(2).map(|pair|
                std::array::from_fn(|i| self.lerp(pair[0][i], pair[1][i], weight))).collect();
        }
        taps[0]
    }
    fn sample(&mut self, result: [Reg; 4], texture: TextureSource, coordinate: &[Reg], lod: Reg) {
        let zero = self.constant(Scalar::F32, 0);
        let one = self.constant(Scalar::U32, 1);
        let levels = self.reg(Scalar::U32);
        let address = self.reg(Scalar::U32);
        let format = self.reg(Scalar::U32);
        self.code.push(Inst::generated(InstKind::TextureFormat {
            dst: format,
            texture,
        }, self.origin.clone()));
        self.code.push(Inst::generated(InstKind::TextureAddress {
            dst: address,
            texture,
        }, self.origin.clone()));
        self.code.push(Inst::generated(InstKind::TextureLevels {
            dst: levels,
            texture,
        }, self.origin.clone()));
        let last = self.binary(Scalar::U32, BinaryOp::ISub, levels, one);
        let last_float = self.cast(Scalar::F32, last);
        let positive = self.math(MathOp::Max, lod, zero);
        let clamped = self.math(MathOp::Min, positive, last_float);
        let first = self.cast(Scalar::U32, clamped);
        let first_float = self.cast(Scalar::F32, first);
        let fraction = self.binary(Scalar::F32, BinaryOp::FSub, clamped, first_float);
        let adjacent = self.binary(Scalar::U32, BinaryOp::IAdd, first, one);
        let next = self.min_u(adjacent, last);
        let a = self.level_sample(texture, coordinate, first, address, format);
        let b = self.level_sample(texture, coordinate, next, address, format);
        for i in 0..4 {
            let value = self.lerp(a[i], b[i], fraction);
            self.code.push(Inst::generated(InstKind::Cast {
                dst: result[i],
                src: value,
            }, self.origin.clone()));
        }
    }
}
fn block(code: Vec<Inst>, registers: &mut Vec<Scalar>) -> Vec<Inst> {
    let mut out = Vec::new();
    for inst in code {
        match inst.kind {
            InstKind::TextureSample { result, texture, coordinate, lod } => {
                let origin = std::sync::Arc::new(GeneratedOrigin {
                    reason: GenerationReason::CpuTextureSample,
                    trigger: Some(GenerationAnchor::CpuTextureSample { sample: OperationOrigin {
                        source_site: inst.source_site, generated_origin: inst.generated_origin,
                    } }),
                });
                let mut lower = Lower { registers, code: Vec::new(), origin };
                lower.sample(result, texture, &coordinate, lod);
                out.extend(lower.code);
            }
            InstKind::Scope(body) => {
                out.push(Inst { kind: InstKind::Scope(block(body, registers)), ..inst });
            }
            InstKind::If { condition, yes, no } => {
                let yes = block(yes, registers);
                let no = block(no, registers);
                out.push(Inst { kind: InstKind::If { condition, yes, no }, ..inst });
            }
            InstKind::Loop { condition, test, body } => {
                let condition = block(condition, registers);
                let body = block(body, registers);
                out.push(Inst { kind: InstKind::Loop { condition, test, body }, ..inst });
            }
            other => out.push(Inst { kind: other, ..inst }),
        }
    }
    out
}

pub(super) fn lower(stage: &mut Stage) {
    fn samples(code: &[Inst]) -> bool {
        code.iter().any(|i| match &(i).kind {
            InstKind::TextureSample { .. } => true,
            InstKind::Scope(body) => samples(body),
            InstKind::If { yes, no, .. } => samples(yes) || samples(no),
            InstKind::Loop {
                condition, body, ..
            } => samples(condition) || samples(body),
            _ => false,
        })
    }
    if !samples(&stage.code) {
        return;
    }
    stage.code = block(std::mem::take(&mut stage.code), &mut stage.registers);
}
impl Emitter<'_> {
    pub(super) fn texture_access(
        &mut self, texture: TextureSource, kind: TextureAccessKind,
        level: Option<Reg>, coordinate: &[Reg], results: &[Reg],
    ) {
        let (base, in_table) = if let Some(parameter) = texture.parameter {
            (self.location(parameter), 0)
        } else {
            let preceding: usize = self.textures[..texture.binding].iter()
                .map(|binding| binding.elements() as usize)
                .sum();
            let offset = (self.buffers.len() + preceding) as u64 * u64::from(BUFFER_BYTES);
            (Location::Immediate(offset), 1)
        };
        let immediate = Location::Immediate(0);
        let (element, scalar, stride) = texture.element.map_or((immediate, Scalar::U32, 0),
            |reg| (self.location(reg), self.stage.registers[reg], BUFFER_BYTES));
        let access = TextureAccess {
            base: base.into(),
            element: element.into(),
            element_scalar: scalar.into(),
            level: level.map_or(immediate, |reg| self.location(reg)).into(),
            coordinates: std::array::from_fn(|axis| {
                coordinate.get(axis).map_or(immediate, |&reg| self.location(reg)).into()
            }),
            in_table,
            stride,
            kind,
            dimensions: coordinate.len() as u32,
        };
        let mut locations: [MoveLocation; 4] = std::array::from_fn(|_| immediate.into());
        for (location, &reg) in locations.iter_mut().zip(results) {
            *location = self.location(reg).into();
        }
        // Binding plus index, mip and texel address, and four scalar loads.
        self.emit_bytes(16 + 24 + 16 + 96 + 4 * 24, |output| unsafe {
            encode_texture_access(access, locations.as_ptr(), results.len(), output)
        });
    }
}
