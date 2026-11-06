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
        if let Self::Local { base, len } = self && *len != 0 { *base = s(*base); }
    }
    pub(crate) fn local_range(self) -> Option<std::ops::Range<usize>> {
        match self {
            Self::Local { base, len } => (len != 0).then_some(base..base + len),
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
pub struct DatasetBorrow {
    pub frame: [Reg; 2],
    pub handle: Reg,
    pub input: usize,
    pub call: usize,
}
#[derive(Clone, Debug)]
pub enum CallArgument {
    Value {
        ty: Ty,
        values: Vec<Reg>,
        dataset_borrow: Option<Box<DatasetBorrow>>,
        #[cfg(feature = "compiler")]
        origin: Option<std::sync::Arc<ArgumentOrigin>>,
    },
    Reference {
        ty: Ty,
        access: Access,
        address: CallReference,
        #[cfg(feature = "compiler")]
        origin: Option<std::sync::Arc<ArgumentOrigin>>,
    },
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub enum ArgumentOrigin {
    Frame,
    Parameter {
        index: usize,
        name: String,
        site: Option<std::sync::Arc<SourceSite>>,
    },
}
#[cfg(feature = "compiler")]
impl CallArgument {
    pub fn origin(&self) -> Option<&ArgumentOrigin> {
        match self { Self::Value { origin, .. } | Self::Reference { origin, .. } => origin.as_deref() }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallTarget {
    External(crate::ast::ExternalName),
    Defined(String),
    Indirect(Reg),
}
#[derive(Clone, Debug)]
pub enum CallParameterKind {
    Value(Ty),
    Reference { ty: Ty, access: Access },
}
/// The same logical ABI parameter owns its optional compiler observation facts.
#[derive(Clone, Debug)]
pub struct CallParameter {
    pub kind: CallParameterKind,
    #[cfg(feature = "compiler")]
    pub(crate) origin: Option<std::sync::Arc<ParameterOrigin>>,
}
#[cfg(feature = "compiler")]
#[derive(Clone, Debug)]
pub enum ParameterOrigin {
    Frame,
    Parameter {
        index: usize,
        name: String,
        instance: std::sync::Arc<str>,
        generated: bool,
        site: Option<std::sync::Arc<SourceSite>>,
    },
}
impl CallParameter {
    pub fn new(kind: CallParameterKind) -> Self {
        Self { kind, #[cfg(feature = "compiler")] origin: None }
    }
    pub fn value(ty: Ty) -> Self { Self::new(CallParameterKind::Value(ty)) }
    pub fn reference(ty: Ty, access: Access) -> Self { Self::new(CallParameterKind::Reference { ty, access }) }
    #[cfg(feature = "compiler")]
    pub fn origin(&self) -> Option<&ParameterOrigin> { self.origin.as_deref() }
    #[cfg(feature = "compiler")]
    pub(crate) fn with_origin(mut self, origin: Option<std::sync::Arc<ParameterOrigin>>) -> Self {
        self.origin = origin; self
    }
}
#[derive(Clone, Debug)]
pub struct FunctionBody {
    /// 可失败B函数返回唯一拥有错误，业务结果只在成功时写出。
    pub fallible: bool,
    pub name: String,
    #[cfg(feature = "compiler")]
    pub source_origin: Option<SourceOrigin>,
    pub parameters: Vec<CallParameter>,
    pub result: Option<Ty>,
    pub body: ValueCode,
}
impl FunctionBody {
    /// Logical IR inputs only; final machine register and stack locations belong to the backend.
    #[cfg(feature = "compiler")]
    pub fn parameter_inputs(&self) -> impl ExactSizeIterator<Item = (&CallParameter, &[Reg])> {
        let mut offset = 0;
        self.parameters.iter().map(move |parameter| {
            let width = match &parameter.kind {
                CallParameterKind::Value(ty) => ty.components.len(),
                CallParameterKind::Reference { .. } => 2,
            };
            let inputs = &self.body.inputs[offset..offset + width];
            offset += width;
            (parameter, inputs)
        })
    }
}
#[derive(Clone, Debug)]
pub struct Call {
    /// 同次实际调用交回的拥有错误；None保持普通调用ABI。
    pub error: Option<Reg>,
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
            CallStorage::Local { base, len } => { if *len != 0 { *base = s(*base); } }
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
            CallArgument::Value { values, dataset_borrow, .. } => values.iter().all(|reg| f(*reg))
                && dataset_borrow.iter().all(|borrow| borrow.frame.iter().chain([&borrow.handle]).all(|reg| f(*reg))),
            CallArgument::Reference { address, .. } => address.all_operands(f),
        })
    }
    pub(crate) fn map_operands(&mut self, r: &impl Fn(Reg) -> Reg) {
        if let CallTarget::Indirect(reg) = &mut self.target { *reg = r(*reg); }
        for argument in &mut self.arguments {
            match argument {
                CallArgument::Value { values, dataset_borrow, .. } => {
                    if let Some(borrow)=dataset_borrow {borrow.frame=borrow.frame.map(r);borrow.handle=r(borrow.handle);}
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
