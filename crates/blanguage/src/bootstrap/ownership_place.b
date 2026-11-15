module bootstrap.ownership_place;
struct ComponentContext { address owner; address output; };
struct PositionMap { address selected; address output; };
struct BorrowedOutput { Node node; address output; };

extern "C" u32 has_owned(address place,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
return host.inspect(place).kind==2 ? 0 : 1;
}
extern "C" u32 fill_borrowed(address borrow,address data,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
BorrowedOutput context=unsafe_position<BorrowedOutput>(data);u64 at=0u64;
 while(at<context.node.words){host.append_borrowed(context.output,borrow,context.node.layout,u32(host.word(context.node.word_values,at,0)));at+=1u64;}
 return 1;
}
extern "C" u32 positions(address output,address place,u32 owned,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
Node node=host.inspect(place);
 u64 count=node.words;
 if(owned!=0){
  if(node.kind==2){return 0;}
  if(node.kind==0){
   if((node.flags&1)!=0){host.reference_positions(output,node.data_values,node.layout,node.word_values,node.words);return 1;}
   count=(node.flags&2)!=0 ? node.words : 0u64;
  }else if(node.kind==1){count=(node.flags&4)!=0 ? node.words : 0u64;}
  else if(node.kind==7){count=node.owned;}
 }else{
  if(node.kind==0||node.kind==2||node.kind==5||node.kind==6){return 1;}
  if(node.kind==7){count=node.data;}
 }
 if(count==0u64){return 1;}
 if(owned==0&&node.kind==1){BorrowedOutput context=BorrowedOutput(node,output);return host.borrow_scope(node.primary,address_of(context),host,c_function(fill_borrowed));}
 u64 at=0u64;
 while(at<count){
  if(node.kind==5||node.kind==6){host.append_local(output,host.word(node.word_values,at,1));}
  else if(node.kind==7){host.append_existing(output,owned!=0 ? node.owned_values : node.data_values,at);}
  else{
   u32 value=u32(host.word(node.word_values,at,0));
   if(node.kind==0){host.append_parameter(output,node.first,value);}
   else if(node.kind==1){host.append_application(output,node.secondary,value);}
   else if(node.kind==3){host.append_view(output,node.primary,node.first,node.secondary,value);}
   else if(node.kind==4){host.append_tile(output,node.first,node.second,value);}
   else{host.append_element(output,node.first,value);}
  }
  at+=1u64;
 }
 return 1;
}
u32 map_words(address output,Node selected,Node root,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
u64 at=0u64;
 while(at<selected.words){
  u64 value=host.word(selected.word_values,at,(selected.flags&8)!=0 ? 1 : 0);
  u64 found=0u64;
  while(found<root.words){
   if(value==host.word(root.word_values,found,(root.flags&8)!=0 ? 1 : 0)){break;}
   found+=1u64;
  }
  if(found==root.words){return 0;}
  host.append_component(output,found);at+=1u64;
 }
 return 1;
}
extern "C" u32 map_positions(address root,address data,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
PositionMap context=unsafe_position<PositionMap>(data);
 u64 at=0u64;u64 count=host.position_count(context.selected);u64 roots=host.position_count(root);
 while(at<count){
  u64 found=0u64;
  while(found<roots){if(host.position_equal(context.selected,at,root,found)!=0){break;}found+=1u64;}
  if(found==roots){return 0;}
  host.append_component(context.output,found);at+=1u64;
 }
 return 1;
}
extern "C" u32 selected_positions(address selected,address data,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
ComponentContext context=unsafe_position<ComponentContext>(data);
 PositionMap next=PositionMap(selected,context.output);
 return host.owned_scope(context.owner,address_of(next),host,c_function(map_positions));
}
extern "C" u32 components(address output,address place,address owner,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
Node selected=host.inspect(place);Node root=host.inspect(owner);
 if((selected.kind==0&&(selected.flags&1)!=0)||(root.kind==0&&(root.flags&1)!=0)){
  ComponentContext context=ComponentContext(owner,output);
  return host.owned_scope(place,address_of(context),host,c_function(selected_positions));
 }
 u32 matches=0;
 if(selected.kind==0&&root.kind==0){matches=host.equal(0,selected.primary,root.primary);}
 else if(selected.kind==1&&root.kind==1){
  matches=host.equal(1,selected.primary,root.primary)!=0&&(((selected.flags&4)!=0&&(root.flags&4)!=0&&host.equal(2,selected.secondary,root.secondary)!=0)||selected.first==root.first) ? 1 : 0;
 }else if(selected.kind==3&&root.kind==3){matches=host.equal(3,selected.primary,root.primary)!=0&&selected.first==root.first&&host.equal(4,selected.secondary,root.secondary)!=0 ? 1 : 0;}
 else if(selected.kind==4&&root.kind==4){matches=selected.first==root.first&&selected.second==root.second ? 1 : 0;}
 else if(selected.kind==7&&root.kind==7){matches=host.equal(5,selected.primary,root.primary)!=0&&selected.first==root.first ? 1 : 0;}
 else if((selected.kind==5||selected.kind==6)&&(root.kind==5||root.kind==6)){matches=1;}
 else if(selected.kind==8&&root.kind==8){matches=selected.first==root.first&&selected.second==root.second ? 1 : 0;}
 if(matches==0){return 0;}
 return map_words(output,selected,root,host);
}
u32 common_words(Node a,Node b,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
u64 i=0u64;
 while(i<a.words){
  u64 value=host.word(a.word_values,i,(a.flags&8)!=0 ? 1 : 0);u64 j=0u64;
  while(j<b.words){if(value==host.word(b.word_values,j,(b.flags&8)!=0 ? 1 : 0)){return 1;}j+=1u64;}
  i+=1u64;
 }
 return 0;
}
extern "C" u32 overlaps(address a,address b,Host host) on cpu
    requires nonzero(host.inspect)
    requires nonzero(host.word)
    requires nonzero(host.equal)
    requires nonzero(host.source_equal)
    requires nonzero(host.reference_overlap)
    requires nonzero(host.storage)
    requires nonzero(host.append_component)
    requires nonzero(host.append_element)
    requires nonzero(host.append_view)
    requires nonzero(host.append_tile)
    requires nonzero(host.append_existing)
    requires nonzero(host.append_application)
    requires nonzero(host.append_parameter)
    requires nonzero(host.append_local)
    requires nonzero(host.borrow_scope)
    requires nonzero(host.append_borrowed)
    requires nonzero(host.reference_positions)
    requires nonzero(host.owned_scope)
    requires nonzero(host.position_count)
    requires nonzero(host.position_equal) {
Node left=host.inspect(a);Node right=host.inspect(b);
 if(left.kind==0&&right.kind==0){return host.reference_overlap(left,right);}
 if(left.kind==1&&right.kind==1){
  u64 i=0u64;u32 found=0;
  while(i<left.sources&&found==0){
   u64 j=0u64;
   while(j<right.sources){if(host.source_equal(left.source_values,i,right.source_values,j)!=0){found=1;break;}j+=1u64;}
   i+=1u64;
  }
  return found!=0&&left.high>=right.low&&right.high>=left.low&&common_words(left,right,host)!=0 ? 1 : 0;
 }
 if(left.kind==7){return overlaps(left.primary,host.storage(b),host);}
 if(right.kind==7){return overlaps(host.storage(a),right.primary,host);}
 if(left.kind==5&&right.kind==5){return common_words(left,right,host);}
 if(left.kind==8&&right.kind==8){return left.first==right.first&&left.second==right.second&&common_words(left,right,host)!=0 ? 1 : 0;}
 return 0;
}
