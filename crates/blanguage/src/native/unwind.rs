//! Windows x64展开字节编码；普通函数、范围和对象辅助入口消费同一职责。
pub(crate) fn encode(prolog: u8, mut operations: Vec<(u8, Vec<u8>)>) -> Vec<u8> {
    operations.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let slots: usize = operations.iter().map(|(_, bytes)| bytes.len() / 2).sum();
    let mut bytes = vec![1, prolog, slots as u8, 0];
    for (_, operation) in operations {
        bytes.extend(operation);
    }
    bytes.resize(bytes.len().div_ceil(4) * 4, 0);
    bytes
}
pub(super) fn allocation(at: u8, frame: u32) -> Vec<u8> {
    if frame <= 128 {
        vec![at, ((frame / 8 - 1) as u8) << 4 | 2]
    } else if frame / 8 <= u16::MAX as u32 {
        let mut v = vec![at, 1];
        v.extend(((frame / 8) as u16).to_le_bytes());
        v
    } else {
        let mut v = vec![at, 0x11];
        v.extend(frame.to_le_bytes());
        v
    }
}
pub(super) fn saved(at: u8, reg: u8, offset: u32, xmm: bool) -> Vec<u8> {
    let unit = if xmm { 16 } else { 8 };
    let op = if xmm { 8 } else { 4 };
    if offset / unit <= u16::MAX as u32 {
        let mut v = vec![at, reg << 4 | op];
        v.extend(((offset / unit) as u16).to_le_bytes());
        v
    } else {
        let mut v = vec![at, reg << 4 | (op + 1)];
        v.extend(offset.to_le_bytes());
        v
    }
}
