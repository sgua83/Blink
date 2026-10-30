//! 原子操作保留显式内存序；它们是内存效果，不是可转发的普通值表达式。
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    AcquireRelease,
}
impl MemoryOrder {
    pub fn from_source(name: &str) -> Option<Self> {
        Some(match name {
            "relaxed" => Self::Relaxed,
            "acquire" => Self::Acquire,
            "release" => Self::Release,
            "acq_rel" => Self::AcquireRelease,
            _ => return None,
        })
    }
}
#[derive(Clone, Debug)]
pub enum AtomicOp {
    Load,
    Store(Reg),
    Add(Reg),
    Sub(Reg),
    Exchange(Reg),
    CompareExchange {
        expected: Reg,
        desired: Reg,
        failure: MemoryOrder,
    },
    Min(Reg),
    Max(Reg),
    And(Reg),
    Or(Reg),
    Xor(Reg),
}
impl AtomicOp {
    pub fn reads(&self) -> bool {
        !matches!(self, Self::Store(_))
    }
    pub fn writes(&self) -> bool {
        !matches!(self, Self::Load)
    }
    pub fn operands(&self) -> Vec<Reg> {
        match self {
            Self::Load => Vec::new(),
            Self::Store(r)
            | Self::Add(r)
            | Self::Sub(r)
            | Self::Exchange(r)
            | Self::Min(r)
            | Self::Max(r)
            | Self::And(r)
            | Self::Or(r)
            | Self::Xor(r) => vec![*r],
            Self::CompareExchange {
                expected, desired, ..
            } => vec![*expected, *desired],
        }
    }
    pub(crate) fn map(&mut self, f: &impl Fn(Reg) -> Reg) {
        match self {
            Self::Load => {}
            Self::Store(r)
            | Self::Add(r)
            | Self::Sub(r)
            | Self::Exchange(r)
            | Self::Min(r)
            | Self::Max(r)
            | Self::And(r)
            | Self::Or(r)
            | Self::Xor(r) => *r = f(*r),
            Self::CompareExchange {
                expected, desired, ..
            } => {
                *expected = f(*expected);
                *desired = f(*desired);
            }
        }
    }
}
#[derive(Clone, Debug)]
pub struct Atomic {
    pub result: Option<Reg>,
    pub scalar: Scalar,
    pub target: AtomicTarget,
    pub index: Reg,
    pub word: u32,
    pub offset: Option<Reg>,
    pub op: AtomicOp,
    pub order: MemoryOrder,
}
#[derive(Clone, Copy, Debug)]
pub enum AtomicTarget {
    Dataset(usize),
    Tile(usize),
}
impl Stage {
    pub(crate) fn requires_atomic64(&self) -> bool {
        fn uses(code: &[Inst]) -> bool {
            code.iter().any(|inst| match inst {
                Inst::Atomic(a) => a.scalar.size() == 8,
                Inst::Scope(body) => uses(body),
                Inst::If { yes, no, .. } => uses(yes) || uses(no),
                Inst::Loop { condition, body, .. } => uses(condition) || uses(body),
                _ => false,
            })
        }
        uses(&self.code)
    }
}
