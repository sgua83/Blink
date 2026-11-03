//! 原生调用共用实际值与位置 ABI；模块函数和外部符号明确区分。
use super::*;
/// Original allocation roots reached through a CPU address; these do not own storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryAlias {
    Local { base: usize, len: usize },
    Data(usize),
}
impl MemoryAlias {
    pub(crate) fn map_locals(&mut self, s: &impl Fn(usize) -> usize) {
        if let Self::Local { base, .. } = self { *base = s(*base); }
    }
    pub(crate) fn local_range(self) -> Option<std::ops::Range<usize>> {
        match self {
            Self::Local { base, len } => Some(base..base + len),
            Self::Data(_) => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MemoryAccess {
    pub alias: MemoryAlias,
    pub access: Access,
}
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
    Indirect(Reg),
}
#[derive(Clone, Debug)]
pub enum CallParameter {
    Value(Ty),
    Reference { ty: Ty, access: Access },
}
#[derive(Clone, Debug)]
pub struct FunctionBody {
    pub name: String,
    #[cfg(feature = "compiler")]
    pub source_origin: Option<SourceOrigin>,
    pub parameters: Vec<CallParameter>,
    pub result: Option<Ty>,
    pub body: ValueCode,
}
#[derive(Clone, Debug)]
pub struct Call {
    pub target: CallTarget,
    pub arguments: Vec<CallArgument>,
    pub memory: Vec<MemoryAccess>,
    pub memory_use: Option<usize>,
    pub result: Option<Ty>,
    pub values: Vec<Reg>,
}
impl CallReference {
    pub(crate) fn aliases(&self) -> impl Iterator<Item = MemoryAlias> + '_ {
        let local = match self.storage {
            CallStorage::Local { base, len } => Some(MemoryAlias::Local { base, len }),
            _ => None,
        };
        let indexed = match &self.storage {
            CallStorage::Indexed(source) => Some(source),
            _ => None,
        };
        local.into_iter().chain(indexed.into_iter().flat_map(IndexedSource::aliases))
    }
    pub(crate) fn local_ranges(&self) -> impl Iterator<Item = std::ops::Range<usize>> + '_ {
        self.aliases().filter_map(MemoryAlias::local_range)
    }
    pub(crate) fn map_locals(&mut self, s: &impl Fn(usize) -> usize) {
        match &mut self.storage {
            CallStorage::Local { base, .. } => *base = s(*base),
            CallStorage::Indexed(source) => source.map_locals(s),
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
        if let CallStorage::Indexed(source) = &mut self.storage { source.map(r); }
    }
}
impl Call {
    pub(crate) fn map_locals(&mut self, s: &impl Fn(usize) -> usize) {
        for argument in &mut self.arguments {
            if let CallArgument::Reference { address, .. } = argument { address.map_locals(s); }
        }
        for effect in &mut self.memory { effect.alias.map_locals(s); }
    }
    pub(crate) fn all_operands(&self, f: &mut impl FnMut(Reg) -> bool) -> bool {
        if let CallTarget::Indirect(reg) = &self.target && !f(*reg) { return false; }
        self.arguments.iter().all(|argument| match argument {
            CallArgument::Value { values, .. } => values.iter().all(|reg| f(*reg)),
            CallArgument::Reference { address, .. } => address.all_operands(f),
        })
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        if let CallTarget::Indirect(reg) = &mut self.target { *reg = r(*reg); }
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
                    address.local_ranges().flatten().collect()
                } else {
                    Vec::new()
                }
            })
            .chain(self.memory.iter().filter(|effect|
                if writing { effect.access.writes() } else { effect.access.reads() }
            ).flat_map(|effect| effect.alias.local_range().into_iter().flatten()))
            .collect()
    }
}
