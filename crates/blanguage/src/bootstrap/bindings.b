module bootstrap.bindings;
import bootstrap.coff;

struct BindingForest {address nodes;u64 count;u64 names;u64 imports;};
struct BindingInsert {u64 root;u64 node;};
BindingBytes empty() on cpu {return BindingBytes(unsafe_address(0u64),0u64);}
BindingNode node_at(BindingForest forest,u64 index) on cpu {
    return unsafe_position<BindingNode>(unsafe_address(address_bits(forest.nodes)+(index-1u64)*NODE_BYTES));
}
void node_put(BindingForest forest,u64 index,BindingNode value) on cpu {
    unsafe_position<BindingNode>(unsafe_address(address_bits(forest.nodes)+(index-1u64)*NODE_BYTES))=value;
}
i32 bytes_compare(BindingBytes left,BindingBytes right) on cpu {
    u64 count=left.length;if(right.length<count){count=right.length;}
    u64 i=0u64;
    while(i<count){
        u32 a=u32(unsafe_position<u8>(unsafe_address(address_bits(left.bytes)+i)));
        u32 b=u32(unsafe_position<u8>(unsafe_address(address_bits(right.bytes)+i)));
        if(a<b){return -1;}if(a>b){return i32(1);}i+=1u64;
    }
    if(left.length<right.length){return -1;}if(left.length>right.length){return i32(1);}return i32(0);
}
i32 key_compare(BindingKey left,BindingKey right) on cpu {
    i32 first=bytes_compare(left.first,right.first);if(first!=0){return first;}return bytes_compare(left.second,right.second);
}
u64 find(BindingForest forest,u64 root,BindingKey key) on cpu {
    u64 at=root;
    while(at!=0u64){
        BindingNode node=node_at(forest,at);i32 order=key_compare(key,node.key);
        if(order==0){return at;}if(order<0){at=node.left;}else{at=node.right;}
    }
    return 0u64;
}
u64 color(BindingForest forest,u64 at) on cpu {
    if(at==0u64){return 0u64;}return node_at(forest,at).red;
}
void paint(BindingForest forest,u64 at,u64 red) on cpu {
    BindingNode node=node_at(forest,at);node.red=red;node_put(forest,at,node);
}
u64 rotate_left(BindingForest forest,u64 root,u64 at) on cpu {
    BindingNode left=node_at(forest,at);u64 right_id=left.right;BindingNode right=node_at(forest,right_id);
    left.right=right.left;
    if(right.left!=0u64){
        BindingNode child=node_at(forest,right.left);child.parent=at;node_put(forest,right.left,child);
    }
    right.parent=left.parent;u64 result=root;
    if(left.parent==0u64){result=right_id;}
    else{
        BindingNode parent=node_at(forest,left.parent);
        if(parent.left==at){parent.left=right_id;}else{parent.right=right_id;}node_put(forest,left.parent,parent);
    }
    right.left=at;left.parent=right_id;node_put(forest,at,left);node_put(forest,right_id,right);return result;
}
u64 rotate_right(BindingForest forest,u64 root,u64 at) on cpu {
    BindingNode right=node_at(forest,at);u64 left_id=right.left;BindingNode left=node_at(forest,left_id);
    right.left=left.right;
    if(left.right!=0u64){
        BindingNode child=node_at(forest,left.right);child.parent=at;node_put(forest,left.right,child);
    }
    left.parent=right.parent;u64 result=root;
    if(right.parent==0u64){result=left_id;}
    else{
        BindingNode parent=node_at(forest,right.parent);
        if(parent.right==at){parent.right=left_id;}else{parent.left=left_id;}node_put(forest,right.parent,parent);
    }
    left.right=at;right.parent=left_id;node_put(forest,at,right);node_put(forest,left_id,left);return result;
}
u64 balance(BindingForest forest,u64 root,u64 inserted) on cpu {
    u64 result=root;u64 at=inserted;
    while(true){
        u64 parent_id=node_at(forest,at).parent;
        if(parent_id==0u64 || color(forest,parent_id)==0u64){break;}
        BindingNode parent=node_at(forest,parent_id);u64 grand_id=parent.parent;BindingNode grand=node_at(forest,grand_id);
        if(parent_id==grand.left){
            u64 uncle=grand.right;
            if(color(forest,uncle)!=0u64){
                paint(forest,parent_id,0u64);paint(forest,uncle,0u64);paint(forest,grand_id,1u64);at=grand_id;
            }else{
                if(at==parent.right){at=parent_id;result=rotate_left(forest,result,at);}
                parent_id=node_at(forest,at).parent;grand_id=node_at(forest,parent_id).parent;
                paint(forest,parent_id,0u64);paint(forest,grand_id,1u64);result=rotate_right(forest,result,grand_id);
            }
        }else{
            u64 uncle=grand.left;
            if(color(forest,uncle)!=0u64){
                paint(forest,parent_id,0u64);paint(forest,uncle,0u64);paint(forest,grand_id,1u64);at=grand_id;
            }else{
                if(at==parent.left){at=parent_id;result=rotate_right(forest,result,at);}
                parent_id=node_at(forest,at).parent;grand_id=node_at(forest,parent_id).parent;
                paint(forest,parent_id,0u64);paint(forest,grand_id,1u64);result=rotate_left(forest,result,grand_id);
            }
        }
    }
    paint(forest,result,0u64);return result;
}
BindingInsert put(inout BindingForest forest,u64 root,BindingKey key,u64 value) on cpu {
    u64 parent=0u64;u64 at=root;i32 order=i32(0);
    while(at!=0u64){
        parent=at;BindingNode node=node_at(forest,at);order=key_compare(key,node.key);
        if(order==0){node.value=value;node_put(forest,at,node);return BindingInsert(root,at);}
        if(order<0){at=node.left;}else{at=node.right;}
    }
    forest.count+=1u64;u64 index=forest.count;
    node_put(forest,index,BindingNode(key,value,parent,0u64,0u64,1u64));
    u64 next_root=root;
    if(parent==0u64){next_root=index;}
    else{
        BindingNode node=node_at(forest,parent);if(order<0){node.left=index;}else{node.right=index;}node_put(forest,parent,node);
    }
    next_root=balance(forest,next_root,index);return BindingInsert(next_root,index);
}
u64 first(BindingForest forest,u64 root) on cpu {
    u64 at=root;
    while(at!=0u64){u64 left=node_at(forest,at).left;if(left==0u64){break;}at=left;}return at;
}
u64 next(BindingForest forest,u64 index) on cpu {
    BindingNode node=node_at(forest,index);if(node.right!=0u64){return first(forest,node.right);}
    u64 at=index;u64 parent=node.parent;
    while(parent!=0u64){BindingNode value=node_at(forest,parent);if(value.left==at){return parent;}at=parent;parent=value.parent;}
    return 0u64;
}

BindingSection section_at(BindingInput input,u64 index) on cpu {
    return unsafe_position<BindingSection>(unsafe_address(address_bits(input.sections)+index*SECTION_BYTES));
}
void section_put(BindingInput input,u64 index,BindingSection value) on cpu {
    unsafe_position<BindingSection>(unsafe_address(address_bits(input.sections)+index*SECTION_BYTES))=value;
}
BindingDefinition definition_at(BindingInput input,u64 index) on cpu {
    return unsafe_position<BindingDefinition>(unsafe_address(address_bits(input.definitions)+index*DEFINITION_BYTES));
}
BindingRelocation relocation_at(BindingSection section,u64 index) on cpu {
    return unsafe_position<BindingRelocation>(unsafe_address(address_bits(section.relocations)+index*INPUT_RELOCATION_BYTES));
}
CoffSymbol symbol_at(BindingWork work,u64 index) on cpu {
    return unsafe_position<CoffSymbol>(unsafe_address(address_bits(work.symbols)+index*SYMBOL_BYTES));
}
void symbol_put(BindingWork work,u64 index,CoffSymbol value) on cpu {
    unsafe_position<CoffSymbol>(unsafe_address(address_bits(work.symbols)+index*SYMBOL_BYTES))=value;
}
void relocation_put(BindingWork work,u64 index,BoundRelocation value) on cpu {
    unsafe_position<BoundRelocation>(unsafe_address(address_bits(work.relocations)+index*OUTPUT_RELOCATION_BYTES))=value;
}
CoffSymbol symbol(BindingBytes name,u64 section,u64 value,u64 function,u64 external,u64 auxiliary,u64 owner,u64 generated,u64 source) on cpu {
    return CoffSymbol(name.bytes,name.length,section,value,function,external,auxiliary,owner,generated,source);
}
u64 digits(u64 value) on cpu {
    u64 remaining=value;u64 count=1u64;while(remaining>=10u64){remaining/=10u64;count+=1u64;}return count;
}
BindingPlan invalid(u32 error) on cpu {return BindingPlan(0u64,0u64,0u64,error);}
extern "C" BindingPlan prepare(BindingInput input,BindingWork work) on cpu {
    if(input.section_count>32767u64){return invalid(2);}
    BindingForest forest=BindingForest(work.nodes,0u64,0u64,0u64);
    u64 i=0u64;
    while(i<input.section_count){
        BindingSection section=section_at(input,i);u64 j=0u64;
        while(j<section.associated_count){
            u64 child=unsafe_position<u64>(unsafe_address(address_bits(section.associated)+j*8u64));
            BindingSection value=section_at(input,child);value.owner=i+1u64;section_put(input,child,value);j+=1u64;
        }
        i+=1u64;
    }
    u64 count=0u64;u64 row=0u64;u64 names=0u64;i=0u64;
    while(i<input.section_count){
        BindingSection section=section_at(input,i);section.symbol=row;section_put(input,i,section);
        u64 auxiliary=0u64;if((section.flags&4096u64)!=0u64){auxiliary=1u64;}
        symbol_put(work,count,symbol(section.name,i+1u64,0u64,0u64,0u64,auxiliary,section.owner,0u64,0u64));
        count+=1u64;row+=1u64+auxiliary;i+=1u64;
    }
    i=0u64;
    while(i<input.section_count){
        BindingSection section=section_at(input,i);
        if((section.flags&4096u64)!=0u64){
            u64 length=17u64+input.name.length+digits(i);
            symbol_put(work,count,symbol(BindingBytes(unsafe_address(0u64),length),i+1u64,0u64,0u64,0u64,0u64,0u64,1u64,i));
            count+=1u64;row+=1u64;names+=length;
        }
        i+=1u64;
    }
    if(input.application!=0u64){
        symbol_put(work,count,symbol(BindingBytes(unsafe_address(0u64),14u64),1u64,0u64,1u64,1u64,0u64,0u64,2u64,0u64));
        count+=1u64;row+=1u64;names+=14u64;
    }
    i=0u64;
    while(i<input.definition_count){
        BindingDefinition value=definition_at(input,i);
        if(value.section>=32767u64){return invalid(2);}
        BindingInsert inserted=put(forest,forest.names,BindingKey(value.name,empty()),row);forest.names=inserted.root;
        symbol_put(work,count,symbol(value.name,value.section+1u64,value.value,value.function,1u64,0u64,0u64,0u64,0u64));
        count+=1u64;row+=1u64;i+=1u64;
    }
    i=0u64;
    while(i<input.section_count){
        BindingSection section=section_at(input,i);u64 j=0u64;
        while(j<section.relocation_count){
            BindingRelocation value=relocation_at(section,j);
            if(value.target==1u64){
                BindingKey key=BindingKey(value.name,empty());
                if(find(forest,forest.names,key)==0u64){
                    BindingInsert inserted=put(forest,forest.names,key,row);forest.names=inserted.root;
                    symbol_put(work,count,symbol(value.name,0u64,0u64,1u64,1u64,0u64,0u64,0u64,0u64));
                    count+=1u64;row+=1u64;
                }
            }else if(value.target==2u64){
                BindingKey key=BindingKey(value.dll,value.name);
                BindingInsert inserted=put(forest,forest.imports,key,0u64);forest.imports=inserted.root;
            }
            j+=1u64;
        }
        i+=1u64;
    }
    u64 at=first(forest,forest.imports);
    while(at!=0u64){
        BindingNode node=node_at(forest,at);
        u64 length=16u64+digits(node.key.first.length)+node.key.first.length+node.key.second.length;
        symbol_put(work,count,symbol(BindingBytes(unsafe_address(0u64),length),0u64,0u64,1u64,1u64,0u64,0u64,3u64,at));
        node.value=row;node_put(forest,at,node);count+=1u64;row+=1u64;names+=length;
        
        at=next(forest,at);
    }
    u64 bound=0u64;i=0u64;
    while(i<input.section_count){
        BindingSection section=section_at(input,i);u64 j=0u64;
        while(j<section.relocation_count){
            BindingRelocation value=relocation_at(section,j);
            if(value.offset>4294967295u64){return invalid(5);}
            u64 target=0u64;
            if(value.target==0u64){target=section_at(input,value.section).symbol;}
            else if(value.target==1u64){target=node_at(forest,find(forest,forest.names,BindingKey(value.name,empty()))).value;}
            else{target=node_at(forest,find(forest,forest.imports,BindingKey(value.dll,value.name))).value;}
            relocation_put(work,bound,BoundRelocation(value.offset,target,value.kind));bound+=1u64;j+=1u64;
        }
        i+=1u64;
    }
    return BindingPlan(count,names,bound,0);
}

void literal(address output,u64 at,u64 value,u64 length) on cpu {
    u64 i=0u64;while(i<length){unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=u8(u32(value>>(i*8u64))&255);i+=1u64;}
}
void copy(address output,u64 at,BindingBytes value) on cpu {
    u64 i=0u64;while(i<value.length){unsafe_position<u8>(unsafe_address(address_bits(output)+at+i))=unsafe_position<u8>(unsafe_address(address_bits(value.bytes)+i));i+=1u64;}
}
void decimal(address output,u64 at,u64 value) on cpu {
    u64 remaining=value;u64 count=digits(value);
    while(count>0u64){count-=1u64;unsafe_position<u8>(unsafe_address(address_bits(output)+at+count))=u8(48+u32(remaining%10u64));remaining/=10u64;}
}
extern "C" void materialize(BindingInput input,BindingWork work,BindingPlan plan,address names) on cpu {
    BindingForest forest=BindingForest(work.nodes,0u64,0u64,0u64);
    u64 at=0u64;u64 i=0u64;
    while(i<plan.symbols){
        CoffSymbol value=symbol_at(work,i);
        if(value.generated!=0u64){
            value.name=unsafe_address(address_bits(names)+at);
            if(value.generated==1u64){
                // "__blink_section$"
                literal(names,at,6875710655180201823u64,8u64);literal(names,at+8u64,2625158131518498163u64,8u64);
                copy(names,at+16u64,input.name);literal(names,at+16u64+input.name.length,36u64,1u64);
                decimal(names,at+17u64+input.name.length,value.source);
            }else if(value.generated==2u64){
                // "mainCRTStartup"
                literal(names,at,6004514652760400237u64,8u64);literal(names,at+8u64,123649767137652u64,6u64);
            }else{
                BindingKey key=node_at(forest,value.source).key;
                // "__blink_import$<dll byte length>$<dll><name>"
                literal(names,at,6875710655180201823u64,8u64);literal(names,at+8u64,10261134006316393u64,7u64);
                decimal(names,at+15u64,key.first.length);u64 begin=at+15u64+digits(key.first.length);
                literal(names,begin,36u64,1u64);copy(names,begin+1u64,key.first);copy(names,begin+1u64+key.first.length,key.second);
            }
            symbol_put(work,i,value);at+=value.name_length;
        }
        i+=1u64;
    }
}
