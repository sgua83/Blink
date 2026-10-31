//! 原生调用共用实际值与位置 ABI；模块函数和外部符号明确区分。
use super::*;
#[derive(Clone, Debug)]
pub enum CallStorage {
    Local { base: usize, len: usize },
    Indexed(IndexedSource),
}
#[derive(Clone, Debug)]
pub struct CallReference {
    pub storage: CallStorage,
    pub offset: Option<Reg>,
    pub word: u32,
}
#[derive(Clone, Debug)]
pub enum CallArgument {
    Value {
        ty: Ty,
        values: Vec<Reg>,
    },
    Reference {
        ty: Ty,
        access: Access,
        address: CallReference,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallTarget {
    External(crate::ast::ExternalName),
    Defined(String),
}
#[derive(Clone, Debug)]
pub enum CallParameter {
    Value(Ty),
    Reference { ty: Ty, access: Access },
}
#[derive(Clone, Debug)]
pub struct FunctionBody {
    pub name: String,
    pub parameters: Vec<CallParameter>,
    pub result: Option<Ty>,
    pub body: ValueCode,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub target: CallTarget,
    pub arguments: Vec<CallArgument>,
    pub result: Option<Ty>,
    pub values: Vec<Reg>,
}
impl CallReference {
    pub(crate) fn local_range(&self) -> Option<std::ops::Range<usize>> {
        match self.storage {
            CallStorage::Local { base, len } => Some(base..base + len),
            _ => None,
        }
    }
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        self.offset.iter().all(|reg| f(*reg)) && match &self.storage {
            CallStorage::Local { .. } => true,
            CallStorage::Indexed(source) => source.all_operands(f),
        }
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        self.offset = self.offset.map(r);
        match &mut self.storage {
            CallStorage::Local { .. } => {}
            CallStorage::Indexed(
                IndexedSource::Data { index, .. } | IndexedSource::Tile { index, .. },
            ) => *index = r(*index),
            CallStorage::Indexed(IndexedSource::View { address, index, .. }) => {
                for a in address {
                    *a = r(*a);
                }
                *index = r(*index);
            }
        }
    }
}
impl Call {
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        self.arguments.iter().all(|argument| match argument {
            CallArgument::Value { values, .. } => values.iter().all(|reg| f(*reg)),
            CallArgument::Reference { address, .. } => address.all_operands(f),
        })
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        for argument in &mut self.arguments {
            match argument {
                CallArgument::Value { values, .. } => {
                    for v in values {
                        *v = r(*v)
                    }
                }
                CallArgument::Reference { address, .. } => address.map_operands(r),
            }
        }
    }
    pub(crate) fn locals(&self, writing: bool) -> Vec<usize> {
        self.arguments
            .iter()
            .flat_map(|arg| {
                if let CallArgument::Reference {
                    address, access, ..
                } = arg
                    && (if writing {
                        access.writes()
                    } else {
                        access.reads()
                    })
                {
                    address.local_range().into_iter().flatten().collect()
                } else {
                    Vec::new()
                }
            })
            .collect()
    }
}
