module bootstrap.task_callbacks;

extern "C" u32 callback_inputs(u32 frame,usize count,address context,InputHost host) on cpu
 requires nonzero(host.frame) requires nonzero(host.parameter) {
 if(frame!=0){
  u32 failed=host.frame(context);
  if(failed!=0){return failed;}
 }
 usize index=0u64;
 while(index<count){
  host.parameter(context,index);
  index+=1u64;
 }
 return 0;
}

extern "C" void selection_inputs(SelectionInputs inputs,address context,SelectionHost host) on cpu
 requires nonzero(host.packet) requires nonzero(host.result) {
 usize index=0u64;
 while(index<inputs.packet_components&&index<inputs.packet_registers){
  host.packet(context,index);
  index+=1u64;
 }
 index=0u64;
 while(index<inputs.result_components){
  host.result(context,index);
  index+=1u64;
 }
}

extern "C" void selection_ready(usize owners,u32 base,u32 stride,address context,cfn<void,address,usize,u32> emit) on cpu
 requires nonzero(emit) {
 usize owner=0u64;
 while(owner<owners){
  emit(context,owner,base+u32(owner)*stride);
  owner+=1u64;
 }
}
