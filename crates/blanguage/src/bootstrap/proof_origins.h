module bootstrap.proof_origins;

extern "C" void origin_list_insert(address list,usize index,address identity) on cpu;
extern "C" void origin_list_copy(address original,address output) on cpu;
extern "C" void origin_list_truncate(address list,usize length) on cpu;
extern "C" void origin_list_retain(address list,address context,cfn<u32,address,address> accept) on cpu
    requires nonzero(accept);

extern "C" void origin_rows_push(address rows,address list) on cpu;
extern "C" void origin_rows_resize(address rows,usize length) on cpu;
extern "C" void origin_rows_copy(address original,address output) on cpu;
extern "C" void origin_rows_clear(address rows) on cpu;
