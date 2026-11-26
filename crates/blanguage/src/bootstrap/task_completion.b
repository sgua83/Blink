module bootstrap.task_completion;

extern "C" Release next_release(Cursor state,Completion completion,address context,Host host) on cpu
 requires nonzero(host.begin_scope) requires nonzero(host.end_scope) requires nonzero(host.position)
 requires nonzero(host.begin_owner) requires nonzero(host.end_owner) requires nonzero(host.dataset)
 requires nonzero(host.descriptor) requires nonzero(host.guard) {
 Cursor cursor=state;
 while(true){
  if(cursor.phase==Phase::Begin){
   if(completion.result_owns!=0 || completion.positions!=0u64){
    host.begin_scope(context);
    cursor.phase=Phase::Position;
    if(completion.result_owns!=0){return Release(cursor,Need::Result);}
   }else{cursor.phase=Phase::Dataset;}
  }else if(cursor.phase==Phase::Position){
   if(cursor.index==completion.positions){
    host.end_scope(context,1);
    cursor.index=0u64;
    cursor.phase=Phase::Dataset;
   }else{
    Ready ready=host.position(context,cursor.index);
    if(ready.present==0){
     cursor.phase=Phase::AfterPosition;
     return Release(cursor,Need::Position);
    }
    cursor.owner=ready.owners;
    cursor.phase=Phase::Owner;
   }
  }else if(cursor.phase==Phase::AfterPosition){
   cursor.index+=1u64;
   cursor.phase=Phase::Position;
  }else if(cursor.phase==Phase::Owner){
   if(cursor.owner==0u64){
    cursor.phase=Phase::AfterPosition;
   }else{
    cursor.owner-=1u64;
    host.begin_owner(context,cursor.owner);
    cursor.phase=Phase::AfterOwner;
    return Release(cursor,Need::Owner);
   }
  }else if(cursor.phase==Phase::AfterOwner){
   host.end_owner(context);
   cursor.phase=Phase::Owner;
  }else if(cursor.phase==Phase::Dataset){
   if(cursor.index==completion.datasets){return Release(cursor,Need::Complete);}
   Dataset dataset=host.dataset(context,cursor.index);
   if(dataset.writes!=0){host.begin_scope(context);}
   host.descriptor(context,cursor.index);
   usize owner=0u64;
   while(owner<dataset.owners){
    host.guard(context,cursor.index,owner);
    owner+=1u64;
   }
   cursor.phase=dataset.writes!=0 ? Phase::CloseDataset : Phase::AfterDataset;
   return Release(cursor,Need::Dataset);
  }else{
   if(cursor.phase==Phase::CloseDataset){host.end_scope(context,0);}
   cursor.index+=1u64;
   cursor.phase=Phase::Dataset;
  }
 }
}
