module bootstrap.task_callbacks;
extern "C" Cursor start() on cpu {return Cursor(0,0u64,0u64);}
extern "C" Step<CallbackNeed> callback_step(Cursor state,u32 frame) on cpu {
 Cursor cursor=state;cursor.phase+=1;
 if(state.phase==0){return Step<CallbackNeed>(cursor,CallbackNeed::Builder);}
 if(state.phase==1){if(frame!=0){return Step<CallbackNeed>(cursor,CallbackNeed::Frame);}return Step<CallbackNeed>(cursor,CallbackNeed::Next);}
 if(state.phase==2){return Step<CallbackNeed>(cursor,CallbackNeed::Registers);}
 if(state.phase==3){return Step<CallbackNeed>(cursor,CallbackNeed::Inputs);}
 if(state.phase==4){return Step<CallbackNeed>(cursor,CallbackNeed::Abi);}
 if(state.phase==5){return Step<CallbackNeed>(cursor,CallbackNeed::Functions);}
 if(state.phase==6){return Step<CallbackNeed>(cursor,CallbackNeed::Emit);}
 if(state.phase==7){return Step<CallbackNeed>(cursor,CallbackNeed::Finish);}
 if(state.phase==8){return Step<CallbackNeed>(cursor,CallbackNeed::Publish);}
 if(state.phase==9){if(frame!=0){return Step<CallbackNeed>(cursor,CallbackNeed::RuntimeContract);}return Step<CallbackNeed>(cursor,CallbackNeed::Next);}
 return Step<CallbackNeed>(cursor,CallbackNeed::Complete);
}
extern "C" Step<SelectionNeed> selection_step(Cursor state,SelectionFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){cursor.phase=1;return Step<SelectionNeed>(cursor,SelectionNeed::Zero);}
 if(cursor.phase==1){
  if(cursor.index<facts.packet_components&&cursor.index<facts.packet_registers){cursor.phase=2;return Step<SelectionNeed>(cursor,SelectionNeed::Packet);}
  cursor.index=0u64;cursor.phase=3;return Step<SelectionNeed>(cursor,SelectionNeed::Next);
 }
 if(cursor.phase==2){cursor.index+=1u64;cursor.phase=1;return Step<SelectionNeed>(cursor,SelectionNeed::Next);}
 if(cursor.phase==3){
  if(cursor.index<facts.result_components){cursor.phase=4;return Step<SelectionNeed>(cursor,SelectionNeed::Result);}
  cursor.index=0u64;cursor.phase=5;return Step<SelectionNeed>(cursor,SelectionNeed::Returned);
 }
 if(cursor.phase==4){cursor.index+=1u64;cursor.phase=3;return Step<SelectionNeed>(cursor,SelectionNeed::Next);}
 if(cursor.phase==5){
  if(cursor.index<facts.outputs){cursor.phase=6;return Step<SelectionNeed>(cursor,SelectionNeed::Guard);}
  cursor.phase=10;return Step<SelectionNeed>(cursor,SelectionNeed::Complete);
 }
 if(cursor.phase==6){cursor.phase=7;return Step<SelectionNeed>(cursor,SelectionNeed::Owners);}
 if(cursor.phase==7){cursor.part=0u64;cursor.phase=8;return Step<SelectionNeed>(cursor,SelectionNeed::Next);}
 if(cursor.phase==8){
  if(cursor.part<facts.owners){cursor.phase=9;return Step<SelectionNeed>(cursor,SelectionNeed::Ready);}
  cursor.index+=1u64;cursor.phase=5;return Step<SelectionNeed>(cursor,SelectionNeed::Next);
 }
 if(cursor.phase==9){cursor.part+=1u64;cursor.phase=8;return Step<SelectionNeed>(cursor,SelectionNeed::Next);}
 return Step<SelectionNeed>(cursor,SelectionNeed::Complete);
}
