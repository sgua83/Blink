//! 已选择COFF模块的编译后材料；运行装入不重新读取对象或选择符号。
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct ModuleCode {
    pub sections: Vec<ModuleSection>,
    pub symbols: BTreeMap<String, (usize, u32)>,
}
#[derive(Clone, Debug)]
pub struct ModuleSection {
    pub name: String,
    pub bytes: Vec<u8>,
    pub flags: u32,
    pub relocations: Vec<ModuleRelocation>,
}
#[derive(Clone, Debug)]
pub struct ModuleRelocation {
    pub offset: usize,
    pub kind: u16,
    pub target: ModuleTarget,
}
#[derive(Clone, Debug)]
pub enum ModuleTarget {
    Section { section: usize, value: u32 },
    Import(crate::ast::ExternalName),
}
/// PE写出与原始VM装入消费同一COFF地址计算；target／source相对于同一映像基址。
#[doc(hidden)]
pub fn coff_relocate(
    bytes: &mut [u8], offset: usize, kind: u16, base: u64, source: u64, target: u64,
) -> Result<(), String> {
    let width = if kind == 1 { 8 } else { 4 };
    let value = bytes.get_mut(offset..offset + width).ok_or("COFF 重定位位置越界")?;
    match kind {
        1 => {
            let add = u64::from_le_bytes(value.try_into().unwrap());
            value.copy_from_slice(&add.wrapping_add(base + target).to_le_bytes());
        }
        3 => {
            let add = u32::from_le_bytes(value.try_into().unwrap());
            let target = u32::try_from(target).map_err(|error| error.to_string())?;
            value.copy_from_slice(&target.wrapping_add(add).to_le_bytes());
        }
        4 => {
            let add = i32::from_le_bytes(value.try_into().unwrap());
            let displacement = i32::try_from(target as i64 - source as i64 - 4 + i64::from(add))
                .map_err(|error| error.to_string())?;
            value.copy_from_slice(&displacement.to_le_bytes());
        }
        _ => return Err(format!("不支持 AMD64 重定位类型 {kind}")),
    }
    Ok(())
}
