//! CPU 采样降低为原生标量 IR；每次取样不调用宿主纹理函数。
use super::*;
struct Lower {
    registers: Vec<Scalar>,
    code: Vec<Inst>,
}
impl Lower {
    fn reg(&mut self, s: Scalar) -> Reg {
        let r = self.registers.len();
        self.registers.push(s);
        r
    }
    fn constant(&mut self, s: Scalar, bits: u32) -> Reg {
        let r = self.reg(s);
        self.code.push(Inst::new(InstKind::Constant(r, s, Bits::from(bits))));
        r
    }
    fn binary(&mut self, s: Scalar, op: BinaryOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::new(InstKind::Binary { dst, op, a, b }));
        dst
    }
    fn cast(&mut self, s: Scalar, src: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::new(InstKind::Cast { dst, src }));
        dst
    }
    fn math(&mut self, op: MathOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(Scalar::F32);
        self.code.push(Inst::new(InstKind::Math {
            dst,
            op,
            args: vec![a, b],
        }));
        dst
    }
    fn min_u(&mut self, a: Reg, b: Reg) -> Reg {
        let condition = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Less), a, b);
        let dst = self.reg(self.registers[a]);
        self.code.push(Inst::new(InstKind::Select {
            dst,
            condition,
            a,
            b,
        }));
        dst
    }
    fn lerp(&mut self, a: Reg, b: Reg, t: Reg) -> Reg {
        let difference = self.binary(Scalar::F32, BinaryOp::FSub, b, a);
        let scaled = self.binary(Scalar::F32, BinaryOp::FMul, difference, t);
        self.binary(Scalar::F32, BinaryOp::FAdd, a, scaled)
    }
    fn select(&mut self, s: Scalar, condition: Reg, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::new(InstKind::Select {
            dst,
            condition,
            a,
            b,
        }));
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
        self.code.push(Inst::new(InstKind::TextureFetch {
            result,
            texture,
            coordinate: coordinate.to_vec(),
            level,
        }));
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
        self.code.push(Inst::new(InstKind::TextureRead {
            dst: raw,
            texture,
            coordinate: coordinate.to_vec(),
            level,
            byte: true,
        }));
        let value = self.cast(Scalar::F32, raw);
        let denominator = self.constant(Scalar::F32, 255.0f32.to_bits());
        let red = self.binary(Scalar::F32, BinaryOp::FDiv, value, denominator);
        self.code.push(Inst::new(InstKind::Cast {
            dst: result[0],
            src: red,
        }));
        for channel in 1..4 {
            let bits = if channel == 3 { 1.0f32.to_bits() } else { 0 };
            self.code
                .push(Inst::new(InstKind::Constant(result[channel], Scalar::F32, Bits::from(bits))));
        }
        let singles = std::mem::take(&mut self.code);
        let raw = self.reg(Scalar::U32);
        self.code.push(Inst::new(InstKind::TextureRead {
            dst: raw,
            texture,
            coordinate: coordinate.to_vec(),
            level,
            byte: false,
        }));
        let unpack_outer = std::mem::take(&mut self.code);
        let mut rgba = Vec::new();
        for (tag, widths) in [(1, &[8,8,8,8][..]), (4, &[16,16][..]), (5, &[10,10,10,2][..])] {
            let mut offset = 0;
            for (channel, &bits) in widths.iter().enumerate() {
                let shift = self.constant(Scalar::U32, offset);
                offset += bits;
                let shifted = self.binary(Scalar::U32, BinaryOp::ShiftRightLogical, raw, shift);
                let mask = self.constant(Scalar::U32, (1 << bits) - 1);
                let byte = self.binary(Scalar::U32, BinaryOp::And, shifted, mask);
                let value = self.cast(Scalar::F32, byte);
                let denominator = self.constant(Scalar::F32, (((1 << bits) - 1) as f32).to_bits());
                let value = self.binary(Scalar::F32, BinaryOp::FDiv, value, denominator);
                self.code.push(Inst::new(InstKind::Cast {
                    dst: result[channel],
                    src: value,
                }));
            }
            for channel in widths.len()..4 {
                self.code.push(Inst::new(InstKind::Constant(result[channel], Scalar::F32, Bits::from(if channel == 3 { 1.0f32.to_bits() } else { 0 }))));
            }
            let unpacked = std::mem::take(&mut self.code);
            if tag == 1 { rgba = unpacked; } else {
                let packed_format = self.constant(Scalar::U32, tag);
                let packed = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Equal), format, packed_format);
                self.code.push(Inst::new(InstKind::If { condition: packed, yes: unpacked, no: rgba }));
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
            self.code.push(Inst::new(InstKind::Cast {
                dst: result[channel],
                src: value,
            }));
        }
        let decode = std::mem::replace(&mut self.code, rgba);
        self.code.push(Inst::new(InstKind::If {
            condition: encoded,
            yes: decode,
            no: Vec::new(),
        }));
        let rgba = std::mem::replace(&mut self.code, packed_outer);
        self.code.push(Inst::new(InstKind::If {
            condition: single,
            yes: singles,
            no: rgba,
        }));
        let packed = std::mem::replace(&mut self.code, outer);
        self.code.push(Inst::new(InstKind::If {
            condition: floating,
            yes: floats,
            no: packed,
        }));
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
        self.code.push(Inst::new(InstKind::TextureSize {
            result: dimensions.clone(), texture, level,
        }));
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
        self.code.push(Inst::new(InstKind::TextureFormat {
            dst: format,
            texture,
        }));
        self.code.push(Inst::new(InstKind::TextureAddress {
            dst: address,
            texture,
        }));
        self.code.push(Inst::new(InstKind::TextureLevels {
            dst: levels,
            texture,
        }));
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
            self.code.push(Inst::new(InstKind::Cast {
                dst: result[i],
                src: value,
            }));
        }
    }
    fn block(&mut self, code: Vec<Inst>) -> Vec<Inst> {
        let outer = std::mem::take(&mut self.code);
        for inst in code {
            match inst.kind {
                InstKind::TextureSample {
                    result,
                    texture,
                    coordinate,
                    lod,
                } => self.sample(result, texture, &coordinate, lod),
                InstKind::Scope(body) => {
                    let body = self.block(body);
                    self.code.push(Inst { kind: InstKind::Scope(body), ..inst });
                }
                InstKind::If { condition, yes, no } => {
                    let yes = self.block(yes);
                    let no = self.block(no);
                    self.code.push(Inst { kind: InstKind::If { condition, yes, no }, ..inst });
                }
                InstKind::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let condition = self.block(condition);
                    let body = self.block(body);
                    self.code.push(Inst { kind: InstKind::Loop {
                        condition,
                        test,
                        body,
                    }, ..inst });
                }
                other => self.code.push(Inst { kind: other, ..inst }),
            }
        }
        std::mem::replace(&mut self.code, outer)
    }
}
pub(super) fn lower(stage: &Stage) -> std::borrow::Cow<'_, Stage> {
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
        return std::borrow::Cow::Borrowed(stage);
    }
    let mut stage = stage.clone();
    let mut lower = Lower {
        registers: stage.registers.clone(),
        code: Vec::new(),
    };
    stage.code = lower.block(stage.code);
    stage.registers = lower.registers;
    std::borrow::Cow::Owned(stage)
}
impl Emitter<'_> {
    pub(super) fn texture_pointer(&mut self, texture: TextureSource) {
        let offset =
            (self.program.buffers.len() + self.program.texture_offset(texture.binding)) as u64 * u64::from(BUFFER_BYTES);
        // x64 位移为有符号 32 位，较远的绑定先用完整原生地址构造基址。
        if offset > i32::MAX as u64 {
            self.bytes(&[0x49, 0xbf]);
            self.bytes(&offset.to_le_bytes());
            self.bytes(&[0x4d, 0x01, 0xe7]);
            if let Some(element) = texture.element {
                self.load(element);
                self.bytes(&[0x48, 0xc1, 0xe0, BUFFER_BYTES.trailing_zeros() as u8]);
                self.bytes(&[0x4d, 0x8b, 0x3c, 0x07]);
            } else {
                self.bytes(&[0x4d, 0x8b, 0x3f]);
            }
            return;
        }
        if let Some(element) = texture.element {
            self.load(element);
            self.bytes(&[0x48, 0xc1, 0xe0, BUFFER_BYTES.trailing_zeros() as u8]);
            self.bytes(&[0x4d, 0x8b, 0xbc, 0x04]);
        } else {
            self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
        }
        self.u32(offset as u32);
    }
    pub(super) fn texture_level(&mut self, texture: TextureSource, level: Reg) {
        self.texture_pointer(texture);
        self.bytes(&[0x4d, 0x8b, 0x7f, 8]);
        self.load(level);
        self.bytes(&[0x48, 0x6b, 0xc0, 24, 0x49, 0x01, 0xc7]);
    }
    pub(super) fn texture_texel(&mut self, texture: TextureSource, level: Reg, coordinate: &[Reg], shift: u8) {
        self.texture_level(texture, level);
        self.load(coordinate[coordinate.len() - 1]);
        for axis in (0..coordinate.len() - 1).rev() {
            self.bytes(&[0x41, 0x8b, 0x4f, axis as u8 * 4]);
            self.bytes(&[0x48, 0x0f, 0xaf, 0xc1, 0x48, 0x89, 0xc2]);
            self.load(coordinate[axis]);
            self.bytes(&[0x48, 0x01, 0xd0]);
        }
        if shift != 0 { self.bytes(&[0x48, 0xc1, 0xe0, shift]); }
        self.bytes(&[0x4d, 0x8b, 0x7f, 16, 0x49, 0x01, 0xc7]);
    }
}
