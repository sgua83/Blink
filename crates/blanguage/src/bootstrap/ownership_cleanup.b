module bootstrap.ownership_cleanup;
extern "C" Cursor start() on cpu {return Cursor(0,0u64);}
extern "C" Kind binding(Kind kind) on cpu {
 if(kind==Kind::Owned||kind==Kind::Data){return kind;}return Kind::Unknown;
}
extern "C" Step<OwnerNeed> owner_step(Cursor state,Info info) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){if((info.flags&1)==0){return Step<OwnerNeed>(cursor,OwnerNeed::Complete);}cursor.phase=1;return Step<OwnerNeed>(cursor,OwnerNeed::Views);}
 if(cursor.phase==1){cursor.phase=2;return Step<OwnerNeed>(cursor,OwnerNeed::Children);}
 if(cursor.phase==2){if((info.flags&2)!=0){cursor.phase=3;return Step<OwnerNeed>(cursor,OwnerNeed::Restrict);}cursor.phase=4;return Step<OwnerNeed>(cursor,OwnerNeed::Union);}
 if(cursor.phase==3){cursor.phase=4;return Step<OwnerNeed>(cursor,OwnerNeed::Union);}
 return Step<OwnerNeed>(cursor,OwnerNeed::Complete);
}
extern "C" Step<ChildNeed> child_step(Cursor state,ChildFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){if(cursor.index>=facts.count){return Step<ChildNeed>(cursor,ChildNeed::Complete);}cursor.phase=1;return Step<ChildNeed>(cursor,ChildNeed::Inspect);}
 if(cursor.phase==1){
  cursor.phase=2;
  if(facts.kind==Kind::Data){return Step<ChildNeed>(cursor,ChildNeed::Data);}
  if(facts.kind==Kind::Owned&&facts.active!=0){return Step<ChildNeed>(cursor,ChildNeed::Owned);}
  return Step<ChildNeed>(cursor,ChildNeed::Next);
 }
 cursor.index+=1u64;cursor.phase=0;return Step<ChildNeed>(cursor,ChildNeed::Next);
}
extern "C" Step<DataNeed> data_step(Cursor state,DataFacts facts) on cpu {
 Cursor cursor=state;
 if(cursor.phase==0){
  if(facts.active==0||facts.nonempty==0){return Step<DataNeed>(cursor,DataNeed::Complete);}
  cursor.phase=1;return Step<DataNeed>(cursor,DataNeed::Inspect);
 }
 if(cursor.phase==1){
  cursor.phase=3;
  if(facts.kind==Kind::Owned){return Step<DataNeed>(cursor,DataNeed::Owned);}
  if(facts.kind==Kind::Data){return Step<DataNeed>(cursor,DataNeed::Data);}
  if(facts.kind==Kind::Unknown){cursor.phase=2;return Step<DataNeed>(cursor,DataNeed::Stored);}
  return Step<DataNeed>(cursor,DataNeed::Next);
 }
 if(cursor.phase==2){cursor.phase=3;if(facts.stored!=0){return Step<DataNeed>(cursor,DataNeed::Unresolved);}return Step<DataNeed>(cursor,DataNeed::Next);}
 if(cursor.phase==3){cursor.phase=4;if(facts.conditional!=0){return Step<DataNeed>(cursor,DataNeed::Restrict);}return Step<DataNeed>(cursor,DataNeed::Next);}
 if(cursor.phase==4){cursor.phase=5;return Step<DataNeed>(cursor,DataNeed::Union);}
 return Step<DataNeed>(cursor,DataNeed::Complete);
}
extern "C" void views(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset) {
 if((info.flags&1)!=0){views_active(node,offset,path,context,info,host);}
}
extern "C" void views_active(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset) {
 if((info.flags&2)!=0){host.selected(node,offset,path,context,info);}
 else{views_selected(node,offset,path,context,info,host);}
}
extern "C" void views_selected(address node,u32 offset,address path,address context,Info info,Host host) on cpu
 requires nonzero(host.inspect) requires nonzero(host.selected) requires nonzero(host.fields)
 requires nonzero(host.child) requires nonzero(host.offset) {
 if((info.flags&12)!=0){host.fields(node,offset,path,context);return;}
 u64 i=0u64;
 while(i<info.count){
  address child=host.child(node,i);
  if(address_bits(child)!=0u64){
   Info next=host.inspect(child);
   if((next.flags&1)!=0){views_active(child,offset+host.offset(node,i,info.flags&16),path,context,next,host);}
  }
  i+=1u64;
 }
}
extern "C" void fields(address list,u64 count,u32 offset,address path,address context,Host host) on cpu
 requires nonzero(host.word) requires nonzero(host.field) requires nonzero(host.emit) {
 u64 i=0u64;
 while(i<count){
  Field field=host.field(list,i);u32 word=offset+field.word;u64 component=0u64;
  while(host.word(context,component)!=word){component+=1u64;}
  host.emit(context,component,field.ty,path);
  i+=1u64;
 }
}
