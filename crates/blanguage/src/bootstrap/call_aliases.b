module bootstrap.call_aliases;

bool incompatible(ArgumentInfo a,ArgumentInfo b) on cpu {
 return a.exclusive!=0||b.exclusive!=0
  ||(a.declared_dataset!=0&&a.value!=0)
  ||(b.declared_dataset!=0&&b.value!=0)
  ||(a.declared_dataset!=0&&b.declared_dataset!=0
    &&((a.atomic!=0&&b.atomic==0)||(b.atomic!=0&&a.atomic==0)))
  ||(a.value!=0&&a.owned_value!=0)||(b.value!=0&&b.owned_value!=0);
}
extern "C" u32 handles_overlap(HandleValues a,HandleValues b,cfn<u32,address,u64,address,u64> equal) on cpu
 requires nonzero(equal) {
 u64 i=0u64;
 while(i<a.count){
  u64 j=0u64;
  while(j<b.count){
   if(equal(a.values,i,b.values,j)!=0){return 1;}
   j+=1u64;
  }
  i+=1u64;
 }
 return 0;
}
u32 overlaps(address context,u64 a,u64 b,ArgumentInfo first,ArgumentInfo second,AliasHost host) on cpu
 requires nonzero(host.stage)
 requires nonzero(host.stage_word)
 requires nonzero(host.place_overlap)
 requires nonzero(host.region)
 requires nonzero(host.source_equal)
 requires nonzero(host.region_word)
 requires nonzero(host.with_handles) {
 if(first.stage_dataset!=0||second.stage_dataset!=0){
  StageInfo left=host.stage(context,a);StageInfo right=host.stage(context,b);
  if(left.present!=0&&right.present!=0){
   if(left.binding!=right.binding){return 0;}
   u64 i=0u64;
   while(i<left.count){
    StageWord word=host.stage_word(context,a,i);u64 j=0u64;
    while(j<right.count){
     StageWord other=host.stage_word(context,b,j);
     if(word.start<other.end&&other.start<word.end){return 1;}
     j+=1u64;
    }
    i+=1u64;
   }
   return 0;
  }
 }
 if((first.reference!=0||second.reference!=0)&&host.place_overlap(context,a,b)!=0){return 1;}
 RegionInfo left=host.region(context,a);RegionInfo right=host.region(context,b);
 if(left.present!=0&&right.present!=0){
  bool shared=false;u64 i=0u64;
  while(i<left.sources&&!shared){
   u64 j=0u64;
   while(j<right.sources&&!shared){
    shared=host.source_equal(context,a,i,b,j)!=0;j+=1u64;
   }
   i+=1u64;
  }
  if(!shared){return 0;}
  i=0u64;
  while(i<left.words){
   u32 word=host.region_word(context,a,i);u64 j=0u64;
   while(j<right.words){
    if(word==host.region_word(context,b,j)){return 1;}
    j+=1u64;
   }
   i+=1u64;
  }
  return 0;
 }
 return host.with_handles(context,a,b,c_function(handles_overlap));
}
extern "C" u64 first_conflict(address context,u64 count,AliasHost host) on cpu
 requires nonzero(host.argument)
 requires nonzero(host.stage)
 requires nonzero(host.stage_word)
 requires nonzero(host.place_overlap)
 requires nonzero(host.region)
 requires nonzero(host.source_equal)
 requires nonzero(host.region_word)
 requires nonzero(host.with_handles) {
 u64 index=0u64;
 while(index<count){
  ArgumentInfo first=host.argument(context,index);u64 other=0u64;
  while(other<index){
   ArgumentInfo before=host.argument(context,other);
   if(incompatible(first,before)&&overlaps(context,index,other,first,before,host)!=0){return index;}
   other+=1u64;
  }
  index+=1u64;
 }
 return count;
}
