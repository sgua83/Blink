//! CPU 采样降低为原生标量 IR；每次取样不调用宿主纹理函数。
use super::*;
use allocation::Location;
use operands::MoveLocation;

#[repr(C)]
struct TextureTexelAddress {
    level: MoveLocation,
    coordinates: [MoveLocation; 3],
    dimensions: u32,
    texel_bytes: u32,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::texture_table_address"]
    fn encode_texture_table_address(offset: u64, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::texture_descriptor"]
    fn encode_texture_descriptor(output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::texture_level_address"]
    fn encode_texture_level_address(level: MoveLocation, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::texture_texel_address"]
    fn encode_texture_texel_address(input: TextureTexelAddress, output: *mut u8) -> u64;
    #[link_name = "bootstrap.x64_instruction::texture_read_byte"]
    fn encode_texture_read_byte(dst: MoveLocation, output: *mut u8) -> u64;
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
    fn srgb(&mut self, value: Reg) -> Reg {
        let threshold = self.constant(Scalar::F32, 0.04045f32.to_bits());
        let condition = self.binary(
            Scalar::Bool,
            BinaryOp::Compare(Comparison::LessEqual),
            value,
            threshold,
        );
        let divisor = self.constant(Scalar::F32, 12.92f32.to_bits());
        let linear = self.binary(Scalar::F32, BinaryOp::FDiv, value, divisor);
        let offset = self.constant(Scalar::F32, 0.055f32.to_bits());
        let scale = self.constant(Scalar::F32, 1.055f32.to_bits());
        let exponent = self.constant(Scalar::F32, 2.4f32.to_bits());
        let shifted = self.binary(Scalar::F32, BinaryOp::FAdd, value, offset);
        let unit = self.binary(Scalar::F32, BinaryOp::FDiv, shifted, scale);
        let curve = self.math(MathOp::Pow, unit, exponent);
        self.select(Scalar::F32, condition, linear, curve)
    }
    fn unsigned_float(&mut self, value: Reg, mantissa_bits: u32) -> Reg {
        let shift = self.constant(Scalar::U32, mantissa_bits);
        let exponent = self.binary(Scalar::U32, BinaryOp::ShiftRightLogical, value, shift);
        let mask = self.constant(Scalar::U32, (1 << mantissa_bits) - 1);
        let fraction = self.binary(Scalar::U32, BinaryOp::And, value, mask);
        let shift = self.constant(Scalar::U32, 23 - mantissa_bits);
        let payload = self.binary(Scalar::U32, BinaryOp::ShiftLeft, fraction, shift);
        let bias = self.constant(Scalar::U32, 112);
        let biased = self.binary(Scalar::U32, BinaryOp::IAdd, exponent, bias);
        let shift = self.constant(Scalar::U32, 23);
        let biased = self.binary(Scalar::U32, BinaryOp::ShiftLeft, biased, shift);
        let normal = self.binary(Scalar::U32, BinaryOp::Or, biased, payload);
        let maximum = self.constant(Scalar::U32, 31);
        let special = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Equal), exponent, maximum);
        let infinity = self.constant(Scalar::U32, 0x7f800000);
        let exceptional = self.binary(Scalar::U32, BinaryOp::Or, infinity, payload);
        let bits = self.select(Scalar::U32, special, exceptional, normal);
        let converted = self.reg(Scalar::F32);
        self.code.push(Inst::generated(InstKind::Unary {
            dst: converted, op: UnaryOp::Bitcast, a: bits,
        }, self.origin.clone()));
        let integer = self.cast(Scalar::F32, fraction);
        let scale = self.constant(Scalar::F32, (127 - 14 - mantissa_bits) << 23);
        let subnormal = self.binary(Scalar::F32, BinaryOp::FMul, integer, scale);
        let zero = self.constant(Scalar::U32, 0);
        let tiny = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Equal), exponent, zero);
        self.select(Scalar::F32, tiny, subnormal, converted)
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
        let value = self.cast(Scalar::F32, raw);
        let denominator = self.constant(Scalar::F32, 255.0f32.to_bits());
        let red = self.binary(Scalar::F32, BinaryOp::FDiv, value, denominator);
        self.code.push(Inst::generated(InstKind::Cast {
            dst: result[0],
            src: red,
        }, self.origin.clone()));
        for channel in 1..4 {
            let bits = if channel == 3 { 1.0f32.to_bits() } else { 0 };
            self.code
                .push(Inst::generated(InstKind::Constant(result[channel], Scalar::F32, Bits::from(bits)), self.origin.clone()));
        }
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
        for (tag, widths) in [(1, &[8,8,8,8][..]), (4, &[16,16][..]), (5, &[10,10,10,2][..]), (6, &[11,11,10][..])] {
            let mut offset = 0;
            for (channel, &bits) in widths.iter().enumerate() {
                let shift = self.constant(Scalar::U32, offset);
                offset += bits;
                let shifted = self.binary(Scalar::U32, BinaryOp::ShiftRightLogical, raw, shift);
                let mask = self.constant(Scalar::U32, (1 << bits) - 1);
                let byte = self.binary(Scalar::U32, BinaryOp::And, shifted, mask);
                let value = if tag == 6 {
                    self.unsigned_float(byte, bits - 5)
                } else {
                    let value = self.cast(Scalar::F32, byte);
                    let denominator = self.constant(Scalar::F32, (((1 << bits) - 1) as f32).to_bits());
                    self.binary(Scalar::F32, BinaryOp::FDiv, value, denominator)
                };
                self.code.push(Inst::generated(InstKind::Cast {
                    dst: result[channel],
                    src: value,
                }, self.origin.clone()));
            }
            for channel in widths.len()..4 {
                self.code.push(Inst::generated(InstKind::Constant(result[channel], Scalar::F32, Bits::from(if channel == 3 { 1.0f32.to_bits() } else { 0 })), self.origin.clone()));
            }
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
        for channel in 0..3 {
            let value = self.srgb(result[channel]);
            self.code.push(Inst::generated(InstKind::Cast {
                dst: result[channel],
                src: value,
            }, self.origin.clone()));
        }
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
    pub(super) fn texture_binding_address(&mut self, texture: TextureSource) {
        if let Some(parameter) = texture.parameter {
            self.copy_scalar(Location::Gpr(15), self.location(parameter), Scalar::U64);
        } else {
            let preceding: usize = self.textures[..texture.binding].iter()
                .map(|binding| binding.elements() as usize)
                .sum();
            let offset = (self.buffers.len() + preceding) as u64 * u64::from(BUFFER_BYTES);
            self.emit_bytes(16, |output| unsafe { encode_texture_table_address(offset, output) });
        }
        if let Some(element) = texture.element {
            self.load(element);
            self.offset_address(BUFFER_BYTES);
        }
    }

    pub(super) fn texture_pointer(&mut self, texture: TextureSource) {
        self.texture_binding_address(texture);
        self.emit_bytes(8, |output| unsafe { encode_texture_descriptor(output) });
    }

    pub(super) fn texture_level(&mut self, texture: TextureSource, level: Reg) {
        self.texture_binding_address(texture);
        let level = self.location(level).into();
        self.emit_bytes(40, |output| unsafe { encode_texture_level_address(level, output) });
    }

    pub(super) fn texture_texel(
        &mut self, texture: TextureSource, level: Reg, coordinate: &[Reg], texel_bytes: u32,
    ) {
        self.texture_binding_address(texture);
        let input = TextureTexelAddress {
            level: self.location(level).into(),
            coordinates: std::array::from_fn(|axis| {
                coordinate.get(axis).map_or(Location::Immediate(0), |&reg| self.location(reg)).into()
            }),
            dimensions: coordinate.len() as u32,
            texel_bytes,
        };
        self.emit_bytes(96, |output| unsafe { encode_texture_texel_address(input, output) });
    }

    pub(super) fn texture_read_byte(&mut self, dst: Reg) {
        let dst = self.location(dst).into();
        self.emit_bytes(24, |output| unsafe { encode_texture_read_byte(dst, output) });
    }
}
