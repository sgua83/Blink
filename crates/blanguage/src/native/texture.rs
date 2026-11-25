//! CPU 采样降低为原生标量 IR；每次取样不调用宿主纹理函数。
use super::*;
use super::operands::{ScalarLocations, ScalarValueHost};
use crate::ir::dataflow::View;
use std::ffi::c_void;

#[repr(C)]
struct TextureAccessHost {
    values: ScalarValueHost,
    table: *const c_void,
    offset: unsafe extern "C" fn(*const c_void, usize) -> u64,
    stride: u32,
}

struct TextureTable<'a> {
    buffers: usize,
    textures: &'a [TextureBinding],
}

unsafe extern "C" fn descriptor_offset(context: *const c_void, binding: usize) -> u64 {
    let table = unsafe { &*context.cast::<TextureTable<'_>>() };
    let preceding: usize = table.textures[..binding].iter().map(|binding| binding.elements() as usize).sum();
    (table.buffers + preceding) as u64 * u64::from(BUFFER_BYTES)
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::emit_texture_access"]
    fn emit_texture_access(instruction: View, host: TextureAccessHost, output: *mut u8) -> u64;
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum SampleScalar { Word, Float, Bool, Index }

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum SampleOp {
    Constant, Cast, Bitcast, Add, Divide, Multiply, LessEqual, Equal,
    ShiftRight, ShiftLeft, And, Or, Power, Select, Into, Set,
    Subtract, Less, GreaterEqual, NotEqual, Minimum, Maximum,
    Floor, Register, Format, AddressMode, Levels,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
enum SampleBlock {
    Output, FloatArm, FormatTest, ByteArm, PackedArm,
    FormatArm, PreviousArm, FormatChoice, SrgbArm, Empty,
}

#[allow(dead_code)]
#[repr(u32)]
enum SampleBlockAction { Select, Append, Branch }

#[allow(dead_code)]
#[repr(u32)]
enum SampleReadKind { Size, Float, Byte, Word, Half }

#[repr(C)]
struct SampleRead {
    kind: SampleReadKind,
    dimensions: u32,
    level: Reg,
    coordinate: [Reg; 4],
    result: [Reg; 4],
}

#[repr(C)]
struct SampleHost {
    context: *mut c_void,
    instruction: unsafe extern "C" fn(*mut c_void, SampleOp, SampleScalar, Reg, Reg, Reg) -> Reg,
    read_texture: unsafe extern "C" fn(*mut c_void, SampleRead) -> Reg,
    block: unsafe extern "C" fn(*mut c_void, SampleBlockAction, SampleBlock, SampleBlock, Reg),
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::texture_sample_ir"]
    fn texture_sample_ir(coordinate: *const Reg, dimensions: u32, lod: Reg, result: *const Reg, host: SampleHost);
}

struct Lower<'a> {
    registers: &'a mut Vec<Scalar>,
    origin: std::sync::Arc<GeneratedOrigin>,
    texture: TextureSource,
    blocks: [Vec<Inst>; SampleBlock::Empty as usize + 1],
    current: SampleBlock,
}

impl Lower<'_> {
    fn reg(&mut self, scalar: Scalar) -> Reg {
        let reg = self.registers.len();
        self.registers.push(scalar);
        reg
    }

    fn emit(&mut self, kind: InstKind) {
        self.blocks[self.current as usize].push(Inst::generated(kind, self.origin.clone()));
    }
}

unsafe extern "C" fn sample_instruction(
    context: *mut c_void, operation: SampleOp, scalar: SampleScalar, a: Reg, b: Reg, c: Reg,
) -> Reg {
    let lower = unsafe { &mut *context.cast::<Lower<'_>>() };
    let scalar = match scalar {
        SampleScalar::Word => Scalar::U32,
        SampleScalar::Float => Scalar::F32,
        SampleScalar::Bool => Scalar::Bool,
        SampleScalar::Index => Scalar::I64,
    };
    let dst = match operation {
        SampleOp::Into | SampleOp::Set | SampleOp::Format | SampleOp::AddressMode | SampleOp::Levels => a,
        _ => lower.reg(scalar),
    };
    let binary = |op| InstKind::Binary { dst, op, a, b };
    let compare = |op| binary(BinaryOp::Compare(op));
    let math = |op| InstKind::Math { dst, op, args: vec![a, b] };
    let texture = lower.texture;
    let kind = match operation {
        SampleOp::Register => return dst,
        SampleOp::Constant => InstKind::Constant(dst, scalar, a as u64),
        SampleOp::Set => InstKind::Constant(dst, scalar, b as u64),
        SampleOp::Cast => InstKind::Cast { dst, src: a },
        SampleOp::Into => InstKind::Cast { dst, src: b },
        SampleOp::Bitcast => InstKind::Unary { dst, op: UnaryOp::Bitcast, a },
        SampleOp::Select => InstKind::Select { dst, condition: a, a: b, b: c },
        SampleOp::Add => binary(if scalar == Scalar::F32 { BinaryOp::FAdd } else { BinaryOp::IAdd }),
        SampleOp::Subtract => binary(if scalar == Scalar::F32 { BinaryOp::FSub } else { BinaryOp::ISub }),
        SampleOp::Divide => binary(BinaryOp::FDiv),
        SampleOp::Multiply => binary(BinaryOp::FMul),
        SampleOp::LessEqual => compare(Comparison::LessEqual),
        SampleOp::Equal => compare(Comparison::Equal),
        SampleOp::Less => compare(Comparison::Less),
        SampleOp::GreaterEqual => compare(Comparison::GreaterEqual),
        SampleOp::NotEqual => compare(Comparison::NotEqual),
        SampleOp::ShiftRight => binary(BinaryOp::ShiftRightLogical),
        SampleOp::ShiftLeft => binary(BinaryOp::ShiftLeft),
        SampleOp::And => binary(BinaryOp::And),
        SampleOp::Or => binary(BinaryOp::Or),
        SampleOp::Power => math(MathOp::Pow),
        SampleOp::Minimum => math(MathOp::Min),
        SampleOp::Maximum => math(MathOp::Max),
        SampleOp::Floor => math(MathOp::Floor),
        SampleOp::Format => InstKind::TextureFormat { dst, texture },
        SampleOp::AddressMode => InstKind::TextureAddress { dst, texture },
        SampleOp::Levels => InstKind::TextureLevels { dst, texture },
    };
    lower.emit(kind);
    dst
}

unsafe extern "C" fn sample_read(context: *mut c_void, read: SampleRead) -> Reg {
    let lower = unsafe { &mut *context.cast::<Lower<'_>>() };
    let count = read.dimensions as usize;
    let coordinate = || read.coordinate[..count].to_vec();
    let texture = lower.texture;
    let level = read.level;
    let width = match read.kind {
        SampleReadKind::Size => {
            lower.emit(InstKind::TextureSize { result: read.result[..count].to_vec(), texture, level });
            return 0;
        }
        SampleReadKind::Float => {
            lower.emit(InstKind::TextureFetch { result: read.result, texture, coordinate: coordinate(), level });
            return 0;
        }
        SampleReadKind::Byte => crate::ir::TextureReadWidth::Byte,
        SampleReadKind::Half => crate::ir::TextureReadWidth::Half,
        SampleReadKind::Word => crate::ir::TextureReadWidth::Word,
    };
    let dst = lower.reg(Scalar::U32);
    lower.emit(InstKind::TextureRead { dst, texture, coordinate: coordinate(), level, width });
    dst
}

unsafe extern "C" fn sample_block(
    context: *mut c_void, action: SampleBlockAction, first: SampleBlock, second: SampleBlock, condition: Reg,
) {
    let lower = unsafe { &mut *context.cast::<Lower<'_>>() };
    match action {
        SampleBlockAction::Select => lower.current = first,
        SampleBlockAction::Append => {
            let source = std::mem::take(&mut lower.blocks[first as usize]);
            lower.blocks[lower.current as usize].extend(source);
        }
        SampleBlockAction::Branch => {
            let yes = std::mem::take(&mut lower.blocks[first as usize]);
            let no = std::mem::take(&mut lower.blocks[second as usize]);
            lower.emit(InstKind::If { condition, yes, no });
        }
    }
}

pub(super) fn sample(
    registers: &mut Vec<Scalar>, texture: TextureSource, coordinate: &[Reg],
    lod: Reg, result: &[Reg; 4], source: OperationOrigin,
) -> Vec<Inst> {
    let origin = std::sync::Arc::new(GeneratedOrigin {
        reason: GenerationReason::CpuTextureSample,
        trigger: Some(GenerationAnchor::CpuTextureSample { sample: source }),
    });
    let mut lower = Lower {
        registers, origin, texture,
        blocks: std::array::from_fn(|_| Vec::new()),
        current: SampleBlock::Output,
    };
    let host = SampleHost {
        context: std::ptr::from_mut(&mut lower).cast(), instruction: sample_instruction,
        read_texture: sample_read, block: sample_block,
    };
    unsafe {
        texture_sample_ir(coordinate.as_ptr(), coordinate.len() as u32, lod, result.as_ptr(), host);
    }
    let [output, ..] = lower.blocks;
    output
}

impl Emitter<'_> {
    pub(super) fn texture_access(&mut self, value: View) {
        let locations = ScalarLocations {
            allocation: &self.allocation, registers: self.registers, tiles: self.tiles,
            frame: &self.frame, workgroup: self.workgroup,
        };
        let table = TextureTable { buffers: self.buffers.len(), textures: self.textures };
        let host = TextureAccessHost {
            values: locations.host(), table: std::ptr::from_ref(&table).cast(),
            offset: descriptor_offset, stride: BUFFER_BYTES,
        };
        let start = self.code.len();
        self.code.reserve(248);
        unsafe {
            let length = emit_texture_access(value, host, self.code.as_mut_ptr().add(start));
            self.code.set_len(start + length as usize);
        }
    }
}
