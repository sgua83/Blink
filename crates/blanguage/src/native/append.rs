//! 单次有界接纳使用原计数和输出地址，成功后直接写入完整记录。
use super::*;

#[repr(C)]
struct AppendBranch {
    length: u64,
    full: u64,
}

unsafe extern "C" {
    #[link_name = "bootstrap.x64_instruction::reserve_append"]
    fn reserve_append(
        counter_slot: u32,
        counter_word: u32,
        capacity_slot: u32,
        target_slot: u32,
        output: *mut u8,
    ) -> AppendBranch;
    #[link_name = "bootstrap.x64_instruction::append_result"]
    fn append_result(output: *mut u8) -> AppendBranch;
}

impl Emitter<'_> {
    pub(super) fn append(&mut self, a: &Append) {
        let start = self.code.len();
        let mut full_offset = 0;
        self.emit_bytes(96, |output| {
            let reservation = unsafe {
                reserve_append(
                    a.count.binding as u32 * BUFFER_BYTES,
                    a.count.word,
                    8 + a.target as u32 * 4,
                    a.target as u32 * BUFFER_BYTES,
                    output,
                )
            };
            full_offset = reservation.full as usize;
            reservation.length
        });
        let full_branch = start + full_offset;
        self.offset_buffer_address(a.target);
        for (word, value) in &a.values {
            self.write_scalar(*value, 15, *word);
        }
        let result_start = self.code.len();
        self.emit_bytes(16, |output| {
            let result = unsafe { append_result(output) };
            full_offset = result.full as usize;
            result.length
        });
        self.patch(full_branch, result_start + full_offset);
        self.store(a.accepted);
    }
}
