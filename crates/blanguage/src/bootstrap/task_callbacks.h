module bootstrap.task_callbacks;

struct InputHost {
 cfn<u32,address> frame;
 cfn<void,address,usize> parameter;
};
struct SelectionInputs {
 usize packet_components;
 usize packet_registers;
 usize result_components;
};
struct SelectionHost {
 cfn<void,address,usize> packet;
 cfn<void,address,usize> result;
};

extern "C" u32 callback_inputs(u32 frame,usize count,address context,InputHost host) on cpu
 requires nonzero(host.frame) requires nonzero(host.parameter);
extern "C" void selection_inputs(SelectionInputs inputs,address context,SelectionHost host) on cpu
 requires nonzero(host.packet) requires nonzero(host.result);
extern "C" void selection_ready(usize owners,u32 base,u32 stride,address context,cfn<void,address,usize,u32> emit) on cpu
 requires nonzero(emit);
