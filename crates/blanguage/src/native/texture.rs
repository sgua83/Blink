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
        self.code.push(Inst::Constant(r, s, bits));
        r
    }
    fn binary(&mut self, s: Scalar, op: BinaryOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::Binary { dst, op, a, b });
        dst
    }
    fn cast(&mut self, s: Scalar, src: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::Cast { dst, src });
        dst
    }
    fn math(&mut self, op: MathOp, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(Scalar::F32);
        self.code.push(Inst::Math {
            dst,
            op,
            args: vec![a, b],
        });
        dst
    }
    fn min_u(&mut self, a: Reg, b: Reg) -> Reg {
        let condition = self.binary(Scalar::Bool, BinaryOp::Compare(Comparison::Less), a, b);
        let dst = self.reg(Scalar::U32);
        self.code.push(Inst::Select {
            dst,
            condition,
            a,
            b,
        });
        dst
    }
    fn lerp(&mut self, a: Reg, b: Reg, t: Reg) -> Reg {
        let difference = self.binary(Scalar::F32, BinaryOp::FSub, b, a);
        let scaled = self.binary(Scalar::F32, BinaryOp::FMul, difference, t);
        self.binary(Scalar::F32, BinaryOp::FAdd, a, scaled)
    }
    fn select(&mut self, s: Scalar, condition: Reg, a: Reg, b: Reg) -> Reg {
        let dst = self.reg(s);
        self.code.push(Inst::Select {
            dst,
            condition,
            a,
            b,
        });
        dst
    }
    fn level_sample(
        &mut self,
        texture: TextureSource,
        coordinate: [Reg; 2],
        level: Reg,
        address: Reg,
    ) -> [Reg; 4] {
        let dimensions = std::array::from_fn(|_| self.reg(Scalar::U32));
        self.code.push(Inst::TextureSize {
            result: dimensions,
            texture,
            level,
        });
        let one = self.constant(Scalar::U32, 1);
        let zero = self.constant(Scalar::F32, 0);
        let half = self.constant(Scalar::F32, 0.5f32.to_bits());
        let mut first = [0; 2];
        let mut next = [0; 2];
        let mut fraction = [0; 2];
        for axis in 0..2 {
            let last = self.binary(Scalar::U32, BinaryOp::ISub, dimensions[axis], one);
            let last_float = self.cast(Scalar::F32, last);
            let size_float = self.cast(Scalar::F32, dimensions[axis]);
            let scaled = self.binary(Scalar::F32, BinaryOp::FMul, coordinate[axis], size_float);
            let centered = self.binary(Scalar::F32, BinaryOp::FSub, scaled, half);
            let positive = self.math(MathOp::Max, centered, zero);
            let at = self.math(MathOp::Min, positive, last_float);
            first[axis] = self.cast(Scalar::U32, at);
            let base = self.cast(Scalar::F32, first[axis]);
            fraction[axis] = self.binary(Scalar::F32, BinaryOp::FSub, at, base);
            let adjacent = self.binary(Scalar::U32, BinaryOp::IAdd, first[axis], one);
            next[axis] = self.min_u(adjacent, last);
            // 先周期化归一坐标，再对四个真实tap分别寻址；边界两侧共同参与过滤。
            let floor = self.math(MathOp::Floor, coordinate[axis], coordinate[axis]);
            let unit = self.binary(Scalar::F32, BinaryOp::FSub, coordinate[axis], floor);
            let scaled = self.binary(Scalar::F32, BinaryOp::FMul, unit, size_float);
            let centered = self.binary(Scalar::F32, BinaryOp::FSub, scaled, half);
            let base = self.math(MathOp::Floor, centered, centered);
            let negative = self.binary(
                Scalar::Bool,
                BinaryOp::Compare(Comparison::Less),
                base,
                zero,
            );
            let index = self.cast(Scalar::U32, base);
            let zero_u = self.constant(Scalar::U32, 0);
            let at_limit = self.binary(
                Scalar::Bool,
                BinaryOp::Compare(Comparison::Equal),
                index,
                dimensions[axis],
            );
            let index = self.select(Scalar::U32, at_limit, zero_u, index);
            let wrapped = self.select(Scalar::U32, negative, last, index);
            let next_index = self.binary(Scalar::U32, BinaryOp::IAdd, wrapped, one);
            let at_end = self.binary(
                Scalar::Bool,
                BinaryOp::Compare(Comparison::Equal),
                wrapped,
                last,
            );
            let wrapped_next = self.select(Scalar::U32, at_end, zero_u, next_index);
            let weight = self.binary(Scalar::F32, BinaryOp::FSub, centered, base);
            let bit = self.constant(Scalar::U32, 1 << axis);
            let flag = self.binary(Scalar::U32, BinaryOp::And, address, bit);
            let repeat = self.binary(
                Scalar::Bool,
                BinaryOp::Compare(Comparison::NotEqual),
                flag,
                zero_u,
            );
            first[axis] = self.select(Scalar::U32, repeat, wrapped, first[axis]);
            next[axis] = self.select(Scalar::U32, repeat, wrapped_next, next[axis]);
            fraction[axis] = self.select(Scalar::F32, repeat, weight, fraction[axis]);
        }
        let mut taps = Vec::new();
        for coordinate in [
            [first[0], first[1]],
            [next[0], first[1]],
            [first[0], next[1]],
            [next[0], next[1]],
        ] {
            let result = std::array::from_fn(|_| self.reg(Scalar::F32));
            self.code.push(Inst::TextureFetch {
                result,
                texture,
                coordinate,
                level,
            });
            taps.push(result);
        }
        std::array::from_fn(|i| {
            let top = self.lerp(taps[0][i], taps[1][i], fraction[0]);
            let bottom = self.lerp(taps[2][i], taps[3][i], fraction[0]);
            self.lerp(top, bottom, fraction[1])
        })
    }
    fn sample(&mut self, result: [Reg; 4], texture: TextureSource, coordinate: [Reg; 2], lod: Reg) {
        let zero = self.constant(Scalar::F32, 0);
        let one = self.constant(Scalar::U32, 1);
        let levels = self.reg(Scalar::U32);
        let address = self.reg(Scalar::U32);
        self.code.push(Inst::TextureAddress {
            dst: address,
            texture,
        });
        self.code.push(Inst::TextureLevels {
            dst: levels,
            texture,
        });
        let last = self.binary(Scalar::U32, BinaryOp::ISub, levels, one);
        let last_float = self.cast(Scalar::F32, last);
        let positive = self.math(MathOp::Max, lod, zero);
        let clamped = self.math(MathOp::Min, positive, last_float);
        let first = self.cast(Scalar::U32, clamped);
        let first_float = self.cast(Scalar::F32, first);
        let fraction = self.binary(Scalar::F32, BinaryOp::FSub, clamped, first_float);
        let adjacent = self.binary(Scalar::U32, BinaryOp::IAdd, first, one);
        let next = self.min_u(adjacent, last);
        let a = self.level_sample(texture, coordinate, first, address);
        let b = self.level_sample(texture, coordinate, next, address);
        for i in 0..4 {
            let value = self.lerp(a[i], b[i], fraction);
            self.code.push(Inst::Cast {
                dst: result[i],
                src: value,
            });
        }
    }
    fn block(&mut self, code: Vec<Inst>) -> Vec<Inst> {
        let outer = std::mem::take(&mut self.code);
        for inst in code {
            match inst {
                Inst::TextureSample {
                    result,
                    texture,
                    coordinate,
                    lod,
                } => self.sample(result, texture, coordinate, lod),
                Inst::Scope(body) => {
                    let body = self.block(body);
                    self.code.push(Inst::Scope(body));
                }
                Inst::If { condition, yes, no } => {
                    let yes = self.block(yes);
                    let no = self.block(no);
                    self.code.push(Inst::If { condition, yes, no });
                }
                Inst::Loop {
                    condition,
                    test,
                    body,
                } => {
                    let condition = self.block(condition);
                    let body = self.block(body);
                    self.code.push(Inst::Loop {
                        condition,
                        test,
                        body,
                    });
                }
                other => self.code.push(other),
            }
        }
        std::mem::replace(&mut self.code, outer)
    }
}
pub(super) fn lower(stage: &Stage) -> std::borrow::Cow<'_, Stage> {
    fn samples(code: &[Inst]) -> bool {
        code.iter().any(|i| match i {
            Inst::TextureSample { .. } => true,
            Inst::Scope(body) => samples(body),
            Inst::If { yes, no, .. } => samples(yes) || samples(no),
            Inst::Loop {
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
            (self.program.buffers.len() + self.program.texture_offset(texture.binding)) as u64 * 8;
        // x64 位移为有符号 32 位，较远的绑定先用完整原生地址构造基址。
        if offset > i32::MAX as u64 {
            self.bytes(&[0x49, 0xbf]);
            self.bytes(&offset.to_le_bytes());
            self.bytes(&[0x4d, 0x01, 0xe7]);
            if let Some(element) = texture.element {
                self.load(element);
                self.bytes(&[0x4d, 0x8b, 0x3c, 0xc7]);
            } else {
                self.bytes(&[0x4d, 0x8b, 0x3f]);
            }
            return;
        }
        if let Some(element) = texture.element {
            self.load(element);
            self.bytes(&[0x4d, 0x8b, 0xbc, 0xc4]);
        } else {
            self.bytes(&[0x4d, 0x8b, 0xbc, 0x24]);
        }
        self.u32(offset as u32);
    }
    pub(super) fn texture_level(&mut self, texture: TextureSource, level: Reg) {
        self.texture_pointer(texture);
        self.bytes(&[0x4d, 0x8b, 0x7f, 8]);
        self.load(level);
        self.bytes(&[0x48, 0xc1, 0xe0, 4, 0x49, 0x01, 0xc7]);
    }
}
