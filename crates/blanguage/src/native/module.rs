//! 已选择COFF模块的编译后材料；运行装入不重新读取对象或选择符号。
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleCode {
    pub sections: Vec<ModuleSection>,
    pub symbols: BTreeMap<String, (usize, u32)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleSection {
    pub name: String,
    pub bytes: Vec<u8>,
    pub flags: u32,
    pub relocations: Vec<ModuleRelocation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRelocation {
    pub offset: usize,
    pub kind: u16,
    pub target: ModuleTarget,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleTarget {
    Section { section: usize, value: u32 },
    Import(crate::ast::ExternalName),
}
#[allow(dead_code, reason = "The B COFF implementation returns these ABI statuses.")]
#[repr(u32)]
pub(crate) enum CoffRelocationStatus {
    Applied,
    Unsupported,
    OutsideSection,
    OutsideField,
}

impl CoffRelocationStatus {
    pub(crate) fn result(self, kind: u32) -> Result<(), String> {
        match self {
            Self::Applied => Ok(()),
            Self::Unsupported => Err(format!("不支持 AMD64 重定位类型 {kind}")),
            Self::OutsideSection => Err("COFF 重定位位置越界".into()),
            Self::OutsideField => Err("out of range integral type conversion attempted".into()),
        }
    }
}

unsafe extern "C" {
    #[cfg(feature = "compiler")]
    #[link_name = "blink.windows.coff::relocation_span"]
    fn relocation_span(length: usize, offset: usize, kind: u32) -> CoffRelocationStatus;

    #[link_name = "blink.windows.coff::apply_relocation"]
    fn apply_relocation(
        bytes: *mut u8, length: usize, offset: usize, kind: u32,
        base: u64, source: u64, target: u64,
    ) -> CoffRelocationStatus;
}

#[cfg(feature = "compiler")]
pub(crate) fn coff_relocation_within_section(length: usize, offset: usize, kind: u16) -> bool {
    !matches!(
        unsafe { relocation_span(length, offset, u32::from(kind)) },
        CoffRelocationStatus::OutsideSection
    )
}

/// PE写出与原始VM装入消费同一COFF地址计算；target／source相对于同一映像基址。
#[doc(hidden)]
pub fn coff_relocate(
    bytes: &mut [u8], offset: usize, kind: u16, base: u64, source: u64, target: u64,
) -> Result<(), String> {
    // The slice exclusively owns the same section until B finishes the write.
    unsafe { apply_relocation(bytes.as_mut_ptr(), bytes.len(), offset, kind.into(), base, source, target) }
        .result(kind.into())
}
