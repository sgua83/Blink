#[repr(C)]
pub struct Pair {
    left: u32,
    right: u32,
}

/// # Safety
/// value 在调用期间指向独占、已初始化的 Pair；函数不保留该借用。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scale_values(value: *mut Pair, factor: u32) {
    let value = unsafe { &mut *value };
    value.left = value.left.wrapping_mul(factor);
    value.right = value.right.wrapping_mul(factor);
}
